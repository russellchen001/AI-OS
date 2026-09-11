use crate::{
    planner::{
        execution::prepare_plan_start, Plan, PlanExecutionCoordinator, PlanExecutionError, PlanId,
        PlanRepository, PlanRepositoryError, PlanStatus, PlanStepId, PlanStepStatus,
    },
    runtime::plan_runtime_bridge::{
        PlanRuntimeExecutionError, PlanRuntimeExecutionRequest, PlanRuntimeExecutor,
    },
    task_engine::{
        Task, TaskId, TaskPlanExecutionError, TaskPlanExecutionPolicy, TaskPlanSynchronization,
        TaskRepository, TaskRepositoryError, TaskStatus,
    },
};
use std::{error::Error, fmt};

pub struct TaskPlanExecutionOrchestrator<T, P> {
    tasks: T,
    plans: P,
}

impl<T, P> TaskPlanExecutionOrchestrator<T, P>
where
    T: TaskRepository,
    P: PlanRepository,
{
    pub fn new(tasks: T, plans: P) -> Self {
        Self { tasks, plans }
    }

    #[cfg(test)]
    fn task_repository(&self) -> &T {
        &self.tasks
    }

    #[cfg(test)]
    fn plan_repository(&self) -> &P {
        &self.plans
    }

    pub fn start_execution(
        &self,
        task_id: &TaskId,
        plan_id: &PlanId,
    ) -> Result<TaskPlanExecutionStart, TaskPlanOrchestrationError> {
        let mut task = self.load_task(task_id)?;
        let mut plan = self.load_plan(plan_id)?;

        TaskPlanExecutionPolicy::validate_start(&task, &plan)
            .map_err(TaskPlanOrchestrationError::Policy)?;
        prepare_plan_start(&mut plan).map_err(TaskPlanOrchestrationError::Planner)?;
        TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan)
            .map_err(TaskPlanOrchestrationError::Policy)?;

        self.plans.update(plan.clone()).map_err(|source| {
            TaskPlanOrchestrationError::PlanPersistence {
                plan_id: plan.id.clone(),
                source,
            }
        })?;

        self.tasks.update(task.clone()).map_err(|source| {
            TaskPlanOrchestrationError::PartialPersistence {
                persisted: TaskPlanAggregate::Plan,
                failed: TaskPlanAggregate::Task,
                source,
            }
        })?;

        Ok(TaskPlanExecutionStart { task, plan })
    }

    pub fn synchronize_after_plan_change(
        &self,
        task_id: &TaskId,
        plan_id: &PlanId,
    ) -> Result<TaskPlanSynchronization, TaskPlanOrchestrationError> {
        let mut task = self.load_task(task_id)?;
        let plan = self.load_plan(plan_id)?;
        let synchronization =
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan)
                .map_err(TaskPlanOrchestrationError::Policy)?;

        if synchronization != TaskPlanSynchronization::Unchanged {
            self.tasks.update(task).map_err(|source| {
                TaskPlanOrchestrationError::TaskPersistence {
                    task_id: task_id.clone(),
                    source,
                }
            })?;
        }

        Ok(synchronization)
    }

    pub fn execute_plan<R>(
        &self,
        task_id: &TaskId,
        plan_id: &PlanId,
        runtime: &R,
    ) -> Result<TaskPlanExecutionStart, TaskPlanOrchestrationError>
    where
        R: PlanRuntimeExecutor + ?Sized,
    {
        self.start_execution(task_id, plan_id)?;
        let coordinator = PlanExecutionCoordinator::new(&self.plans);

        loop {
            let plan = self.load_plan(plan_id)?;
            if plan.status.is_terminal() {
                break;
            }
            let Some(step) = plan
                .steps
                .iter()
                .find(|step| step.status == PlanStepStatus::Ready)
                .cloned()
            else {
                return Err(TaskPlanOrchestrationError::NoReadyStep { plan_id: plan.id });
            };

            coordinator
                .start_step(plan_id, &step.id)
                .map_err(TaskPlanOrchestrationError::Planner)?;
            let running_state = DurableTaskPlanState {
                task_status: TaskStatus::Executing,
                plan_status: PlanStatus::Executing,
                step_id: Some(step.id.clone()),
                step_status: Some(PlanStepStatus::Running),
            };

            let runtime_result = runtime.execute_step(PlanRuntimeExecutionRequest {
                task_id: task_id.clone(),
                plan_id: plan_id.clone(),
                step_id: step.id.clone(),
                agent_id: plan.agent_id.clone(),
                goal: plan.objective.clone(),
                capability: step.capability,
                input: step.input,
                user_confirmed: step.user_confirmed,
            });

            match runtime_result {
                Ok(result) => {
                    let completion = match result.output {
                        Some(output) => coordinator.complete_step(plan_id, &step.id, output),
                        None => coordinator.complete_step_without_output(plan_id, &step.id),
                    };
                    if let Err(persistence_error) = completion {
                        return Err(
                            TaskPlanOrchestrationError::ExecutionSucceededButPersistenceFailed {
                                operation_id: result.operation_id,
                                persistence_error,
                                durable_state: running_state,
                            },
                        );
                    }
                }
                Err(source) => {
                    let failed_plan =
                        match coordinator.fail_step(plan_id, &step.id) {
                            Ok(plan) => plan,
                            Err(persistence_error) => return Err(
                                TaskPlanOrchestrationError::ExecutionFailedAndCompensationFailed {
                                    runtime_error: source,
                                    persistence_error,
                                    durable_state: running_state,
                                },
                            ),
                        };
                    if let Err(synchronization_error) =
                        self.synchronize_after_plan_change(task_id, plan_id)
                    {
                        return Err(
                            TaskPlanOrchestrationError::PlanTerminalButTaskSynchronizationFailed {
                                runtime_error: Some(source),
                                synchronization_error: Box::new(synchronization_error),
                                durable_state: DurableTaskPlanState {
                                    task_status: TaskStatus::Executing,
                                    plan_status: failed_plan.status,
                                    step_id: Some(step.id),
                                    step_status: Some(PlanStepStatus::Failed),
                                },
                            },
                        );
                    }
                    return Err(TaskPlanOrchestrationError::Runtime { source });
                }
            }
        }

        let terminal_plan = self.load_plan(plan_id)?;
        if let Err(synchronization_error) = self.synchronize_after_plan_change(task_id, plan_id) {
            return Err(
                TaskPlanOrchestrationError::PlanTerminalButTaskSynchronizationFailed {
                    runtime_error: None,
                    synchronization_error: Box::new(synchronization_error),
                    durable_state: DurableTaskPlanState {
                        task_status: TaskStatus::Executing,
                        plan_status: terminal_plan.status,
                        step_id: None,
                        step_status: None,
                    },
                },
            );
        }
        Ok(TaskPlanExecutionStart {
            task: self.load_task(task_id)?,
            plan: self.load_plan(plan_id)?,
        })
    }

    fn load_task(&self, task_id: &TaskId) -> Result<Task, TaskPlanOrchestrationError> {
        self.tasks
            .get(task_id)
            .map_err(TaskPlanOrchestrationError::TaskLoad)?
            .ok_or_else(|| {
                TaskPlanOrchestrationError::TaskLoad(TaskRepositoryError::NotFound(task_id.clone()))
            })
    }

    fn load_plan(&self, plan_id: &PlanId) -> Result<Plan, TaskPlanOrchestrationError> {
        self.plans
            .get(plan_id)
            .map_err(TaskPlanOrchestrationError::PlanLoad)?
            .ok_or_else(|| {
                TaskPlanOrchestrationError::PlanLoad(PlanRepositoryError::NotFound(plan_id.clone()))
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskPlanExecutionStart {
    pub task: Task,
    pub plan: Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPlanAggregate {
    Task,
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableTaskPlanState {
    pub task_status: TaskStatus,
    pub plan_status: PlanStatus,
    pub step_id: Option<PlanStepId>,
    pub step_status: Option<PlanStepStatus>,
}

impl fmt::Display for TaskPlanAggregate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task => formatter.write_str("Task"),
            Self::Plan => formatter.write_str("Plan"),
        }
    }
}

#[derive(Debug)]
pub enum TaskPlanOrchestrationError {
    TaskLoad(TaskRepositoryError),
    PlanLoad(PlanRepositoryError),
    Policy(TaskPlanExecutionError),
    Planner(PlanExecutionError),
    Runtime {
        source: PlanRuntimeExecutionError,
    },
    ExecutionSucceededButPersistenceFailed {
        operation_id: String,
        persistence_error: PlanExecutionError,
        durable_state: DurableTaskPlanState,
    },
    ExecutionFailedAndCompensationFailed {
        runtime_error: PlanRuntimeExecutionError,
        persistence_error: PlanExecutionError,
        durable_state: DurableTaskPlanState,
    },
    PlanTerminalButTaskSynchronizationFailed {
        runtime_error: Option<PlanRuntimeExecutionError>,
        synchronization_error: Box<TaskPlanOrchestrationError>,
        durable_state: DurableTaskPlanState,
    },
    NoReadyStep {
        plan_id: PlanId,
    },
    PlanPersistence {
        plan_id: PlanId,
        source: PlanRepositoryError,
    },
    TaskPersistence {
        task_id: TaskId,
        source: TaskRepositoryError,
    },
    PartialPersistence {
        persisted: TaskPlanAggregate,
        failed: TaskPlanAggregate,
        source: TaskRepositoryError,
    },
}

impl fmt::Display for TaskPlanOrchestrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskLoad(error) => write!(formatter, "failed to load Task: {error}"),
            Self::PlanLoad(error) => write!(formatter, "failed to load Plan: {error}"),
            Self::Policy(error) => {
                write!(formatter, "Task–Plan policy rejected operation: {error}")
            }
            Self::Planner(error) => write!(formatter, "Planner rejected operation: {error}"),
            Self::Runtime { source } => write!(formatter, "Runtime rejected PlanStep: {source}"),
            Self::ExecutionSucceededButPersistenceFailed { operation_id, .. } => write!(
                formatter,
                "Runtime operation {operation_id} succeeded, but Plan completion was not persisted"
            ),
            Self::ExecutionFailedAndCompensationFailed { runtime_error, .. } => write!(
                formatter,
                "Runtime failed ({runtime_error}) and Plan failure compensation was not persisted"
            ),
            Self::PlanTerminalButTaskSynchronizationFailed { .. } => formatter.write_str(
                "Plan reached a durable terminal state, but Task synchronization was not persisted",
            ),
            Self::NoReadyStep { plan_id } => {
                write!(formatter, "executing Plan {plan_id} has no ready step")
            }
            Self::PlanPersistence { plan_id, source } => {
                write!(formatter, "failed to persist Plan {plan_id}: {source}")
            }
            Self::TaskPersistence { task_id, source } => {
                write!(formatter, "failed to persist Task {task_id}: {source}")
            }
            Self::PartialPersistence {
                persisted,
                failed,
                source,
            } => write!(
                formatter,
                "partial persistence: {persisted} was persisted but {failed} failed: {source}"
            ),
        }
    }
}

impl Error for TaskPlanOrchestrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TaskLoad(error) => Some(error),
            Self::PlanLoad(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Planner(error) => Some(error),
            Self::Runtime { source } => Some(source),
            Self::ExecutionSucceededButPersistenceFailed {
                persistence_error, ..
            }
            | Self::ExecutionFailedAndCompensationFailed {
                persistence_error, ..
            } => Some(persistence_error),
            Self::PlanTerminalButTaskSynchronizationFailed {
                synchronization_error,
                ..
            } => Some(synchronization_error.as_ref()),
            Self::NoReadyStep { .. } => None,
            Self::PlanPersistence { source, .. } => Some(source),
            Self::TaskPersistence { source, .. } => Some(source),
            Self::PartialPersistence { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        planner::{PlanStatus, PlanStep},
        task_engine::{TaskStatus, TaskType},
    };
    use serde_json::json;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    #[derive(Clone)]
    struct RecordingTaskRepository {
        state: Arc<Mutex<TaskRepositoryState>>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    struct TaskRepositoryState {
        task: Option<Task>,
        get_error: Option<TaskRepositoryError>,
        update_error: Option<TaskRepositoryError>,
        fail_update_at: Option<usize>,
        writes: usize,
    }

    impl RecordingTaskRepository {
        fn new(task: Option<Task>, order: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                state: Arc::new(Mutex::new(TaskRepositoryState {
                    task,
                    get_error: None,
                    update_error: None,
                    fail_update_at: None,
                    writes: 0,
                })),
                order,
            }
        }

        fn fail_get(&self, error: TaskRepositoryError) {
            self.state.lock().unwrap().get_error = Some(error);
        }

        fn fail_update(&self, error: TaskRepositoryError) {
            self.state.lock().unwrap().update_error = Some(error);
        }

        fn fail_update_at(&self, write: usize, error: TaskRepositoryError) {
            let mut state = self.state.lock().unwrap();
            state.fail_update_at = Some(write);
            state.update_error = Some(error);
        }

        fn snapshot(&self) -> Option<Task> {
            self.state.lock().unwrap().task.clone()
        }

        fn writes(&self) -> usize {
            self.state.lock().unwrap().writes
        }
    }

    impl TaskRepository for RecordingTaskRepository {
        fn create(&self, task: Task) -> Result<TaskId, TaskRepositoryError> {
            let mut state = self.state.lock().unwrap();
            state.task = Some(task.clone());
            Ok(task.id)
        }

        fn get(&self, _task_id: &TaskId) -> Result<Option<Task>, TaskRepositoryError> {
            let mut state = self.state.lock().unwrap();
            if let Some(error) = state.get_error.take() {
                return Err(error);
            }
            Ok(state.task.clone())
        }

        fn list(&self) -> Result<Vec<Task>, TaskRepositoryError> {
            Ok(self.snapshot().into_iter().collect())
        }

        fn update(&self, task: Task) -> Result<(), TaskRepositoryError> {
            self.order.lock().unwrap().push("task");
            let mut state = self.state.lock().unwrap();
            state.writes += 1;
            if state.fail_update_at.is_none() || state.fail_update_at == Some(state.writes) {
                if let Some(error) = state.update_error.take() {
                    return Err(error);
                }
            }
            state.task = Some(task);
            Ok(())
        }

        fn delete(&self, task_id: &TaskId) -> Result<Task, TaskRepositoryError> {
            self.state
                .lock()
                .unwrap()
                .task
                .take()
                .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))
        }
    }

    #[derive(Clone)]
    struct RecordingPlanRepository {
        state: Arc<Mutex<PlanRepositoryState>>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    struct PlanRepositoryState {
        plan: Option<Plan>,
        get_error: Option<PlanRepositoryError>,
        update_error: Option<PlanRepositoryError>,
        fail_update_at: Option<usize>,
        writes: usize,
    }

    impl RecordingPlanRepository {
        fn new(plan: Option<Plan>, order: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                state: Arc::new(Mutex::new(PlanRepositoryState {
                    plan,
                    get_error: None,
                    update_error: None,
                    fail_update_at: None,
                    writes: 0,
                })),
                order,
            }
        }

        fn fail_get(&self, error: PlanRepositoryError) {
            self.state.lock().unwrap().get_error = Some(error);
        }

        fn fail_update(&self, error: PlanRepositoryError) {
            self.state.lock().unwrap().update_error = Some(error);
        }

        fn fail_update_at(&self, write: usize, error: PlanRepositoryError) {
            let mut state = self.state.lock().unwrap();
            state.fail_update_at = Some(write);
            state.update_error = Some(error);
        }

        fn snapshot(&self) -> Option<Plan> {
            self.state.lock().unwrap().plan.clone()
        }

        fn writes(&self) -> usize {
            self.state.lock().unwrap().writes
        }
    }

    impl PlanRepository for RecordingPlanRepository {
        fn create(&self, plan: Plan) -> Result<PlanId, PlanRepositoryError> {
            let mut state = self.state.lock().unwrap();
            state.plan = Some(plan.clone());
            Ok(plan.id)
        }

        fn get(&self, _plan_id: &PlanId) -> Result<Option<Plan>, PlanRepositoryError> {
            let mut state = self.state.lock().unwrap();
            if let Some(error) = state.get_error.take() {
                return Err(error);
            }
            Ok(state.plan.clone())
        }

        fn list(&self) -> Result<Vec<Plan>, PlanRepositoryError> {
            Ok(self.snapshot().into_iter().collect())
        }

        fn list_by_task(&self, task_id: &TaskId) -> Result<Vec<Plan>, PlanRepositoryError> {
            Ok(self
                .snapshot()
                .filter(|plan| &plan.task_id == task_id)
                .into_iter()
                .collect())
        }

        fn update(&self, plan: Plan) -> Result<(), PlanRepositoryError> {
            self.order.lock().unwrap().push("plan");
            let mut state = self.state.lock().unwrap();
            state.writes += 1;
            if state.fail_update_at.is_none() || state.fail_update_at == Some(state.writes) {
                if let Some(error) = state.update_error.take() {
                    return Err(error);
                }
            }
            state.plan = Some(plan);
            Ok(())
        }

        fn delete(&self, plan_id: &PlanId) -> Result<Plan, PlanRepositoryError> {
            self.state
                .lock()
                .unwrap()
                .plan
                .take()
                .ok_or_else(|| PlanRepositoryError::NotFound(plan_id.clone()))
        }
    }

    type TestOrchestrator =
        TaskPlanExecutionOrchestrator<RecordingTaskRepository, RecordingPlanRepository>;

    struct RecordingRuntime {
        plans: RecordingPlanRepository,
        order: Arc<Mutex<Vec<&'static str>>>,
        calls: Mutex<Vec<PlanRuntimeExecutionRequest>>,
        outcomes: Mutex<
            VecDeque<
                Result<
                    crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult,
                    PlanRuntimeExecutionError,
                >,
            >,
        >,
    }

    impl RecordingRuntime {
        fn new(
            plans: RecordingPlanRepository,
            order: Arc<Mutex<Vec<&'static str>>>,
            outcomes: impl IntoIterator<
                Item = Result<
                    crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult,
                    PlanRuntimeExecutionError,
                >,
            >,
        ) -> Self {
            Self {
                plans,
                order,
                calls: Mutex::new(Vec::new()),
                outcomes: Mutex::new(outcomes.into_iter().collect()),
            }
        }
    }

    impl PlanRuntimeExecutor for RecordingRuntime {
        fn execute_step(
            &self,
            request: PlanRuntimeExecutionRequest,
        ) -> Result<
            crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult,
            PlanRuntimeExecutionError,
        > {
            let plan = self.plans.snapshot().unwrap();
            assert_eq!(
                plan.steps
                    .iter()
                    .find(|step| step.id == request.step_id)
                    .unwrap()
                    .status,
                PlanStepStatus::Running
            );
            self.order.lock().unwrap().push("runtime");
            self.calls.lock().unwrap().push(request);
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("test runtime outcome")
        }
    }

    fn runtime_success(
        operation_id: &str,
        output: serde_json::Value,
    ) -> crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult {
        crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult {
            operation_id: operation_id.to_owned(),
            output: Some(output),
        }
    }

    fn runtime_success_without_output(
        operation_id: &str,
    ) -> crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult {
        crate::runtime::plan_runtime_bridge::PlanRuntimeExecutionResult {
            operation_id: operation_id.to_owned(),
            output: None,
        }
    }

    fn ready_pair() -> (Task, Plan) {
        let mut task = Task::new(TaskType::Do, "execute workflow").unwrap();
        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();

        let mut plan = Plan::new(task.id.clone(), 1, "execute workflow").unwrap();
        plan.add_step(PlanStep::new("execute", "test.execute").unwrap())
            .unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();

        task.activate_plan(plan.id.clone());
        task.transition_to(TaskStatus::Ready).unwrap();
        (task, plan)
    }

    fn executing_pair() -> (Task, Plan) {
        let (mut task, mut plan) = ready_pair();
        prepare_plan_start(&mut plan).unwrap();
        TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan).unwrap();
        (task, plan)
    }

    fn orchestrator(
        task: Option<Task>,
        plan: Option<Plan>,
    ) -> (TestOrchestrator, Arc<Mutex<Vec<&'static str>>>) {
        let order = Arc::new(Mutex::new(Vec::new()));
        let tasks = RecordingTaskRepository::new(task, Arc::clone(&order));
        let plans = RecordingPlanRepository::new(plan, Arc::clone(&order));
        (TaskPlanExecutionOrchestrator::new(tasks, plans), order)
    }

    #[test]
    fn execute_plan_persists_running_before_sequential_runtime_success() {
        let (task, mut plan) = ready_pair();
        let first_id = plan.steps[0].id.clone();
        let second = PlanStep::new("verify", "test.verify")
            .unwrap()
            .depends_on(first_id.clone());
        let second_id = second.id.clone();
        plan.add_step(second).unwrap();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            Arc::clone(&order),
            [
                Ok(runtime_success("operation-1", json!({"first": true}))),
                Ok(runtime_success("operation-2", json!({"second": true}))),
            ],
        );

        let result = service.execute_plan(&task.id, &plan.id, &runtime).unwrap();

        assert_eq!(result.plan.status, PlanStatus::Completed);
        assert_eq!(result.task.status, TaskStatus::Verifying);
        assert_eq!(
            result
                .plan
                .steps
                .iter()
                .map(|step| step.status)
                .collect::<Vec<_>>(),
            vec![PlanStepStatus::Completed, PlanStepStatus::Completed]
        );
        assert_eq!(
            runtime
                .calls
                .lock()
                .unwrap()
                .iter()
                .map(|request| request.step_id.clone())
                .collect::<Vec<_>>(),
            vec![first_id, second_id]
        );
        assert_eq!(
            &order.lock().unwrap()[..4],
            ["plan", "task", "plan", "runtime"]
        );
    }

    #[test]
    fn policy_rejection_never_submits_to_runtime() {
        let (mut task, plan) = ready_pair();
        task.transition_to(TaskStatus::Executing).unwrap();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        let runtime =
            RecordingRuntime::new(service.plan_repository().clone(), order, std::iter::empty());

        assert!(matches!(
            service.execute_plan(&task.id, &plan.id, &runtime),
            Err(TaskPlanOrchestrationError::Policy(_))
        ));
        assert!(runtime.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn runtime_denial_fails_step_cancels_remaining_and_synchronizes_task() {
        let (task, mut plan) = ready_pair();
        let first_id = plan.steps[0].id.clone();
        plan.add_step(
            PlanStep::new("dependent", "test.dependent")
                .unwrap()
                .depends_on(first_id),
        )
        .unwrap();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            order,
            [Err(PlanRuntimeExecutionError::PermissionDenied)],
        );

        assert!(matches!(
            service.execute_plan(&task.id, &plan.id, &runtime),
            Err(TaskPlanOrchestrationError::Runtime {
                source: PlanRuntimeExecutionError::PermissionDenied
            })
        ));

        let persisted_plan = service.plan_repository().snapshot().unwrap();
        assert_eq!(persisted_plan.status, PlanStatus::Failed);
        assert_eq!(persisted_plan.steps[0].status, PlanStepStatus::Failed);
        assert_eq!(persisted_plan.steps[1].status, PlanStepStatus::Cancelled);
        assert_eq!(
            service.task_repository().snapshot().unwrap().status,
            TaskStatus::Failed
        );
        assert_eq!(runtime.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn runtime_success_with_no_output_persists_none() {
        let (task, plan) = ready_pair();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            order,
            [Ok(runtime_success_without_output("operation-1"))],
        );

        let result = service.execute_plan(&task.id, &plan.id, &runtime).unwrap();

        assert_eq!(result.plan.steps[0].output, None);
    }

    #[test]
    fn runtime_success_plus_completion_persistence_failure_is_typed_partial_success() {
        let (task, plan) = ready_pair();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .plan_repository()
            .fail_update_at(3, PlanRepositoryError::LockPoisoned);
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            order,
            [Ok(runtime_success("operation-1", json!({"done": true})))],
        );

        let error = service
            .execute_plan(&task.id, &plan.id, &runtime)
            .unwrap_err();

        assert!(matches!(
            error,
            TaskPlanOrchestrationError::ExecutionSucceededButPersistenceFailed {
                operation_id,
                durable_state: DurableTaskPlanState {
                    task_status: TaskStatus::Executing,
                    plan_status: PlanStatus::Executing,
                    step_status: Some(PlanStepStatus::Running),
                    ..
                },
                ..
            } if operation_id == "operation-1"
        ));
    }

    #[test]
    fn runtime_failure_plus_compensation_failure_preserves_both_errors() {
        let (task, plan) = ready_pair();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .plan_repository()
            .fail_update_at(3, PlanRepositoryError::LockPoisoned);
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            order,
            [Err(PlanRuntimeExecutionError::PermissionDenied)],
        );

        let error = service
            .execute_plan(&task.id, &plan.id, &runtime)
            .unwrap_err();

        assert!(matches!(
            error,
            TaskPlanOrchestrationError::ExecutionFailedAndCompensationFailed {
                runtime_error: PlanRuntimeExecutionError::PermissionDenied,
                persistence_error: PlanExecutionError::Repository(
                    PlanRepositoryError::LockPoisoned
                ),
                durable_state: DurableTaskPlanState {
                    step_status: Some(PlanStepStatus::Running),
                    ..
                },
            }
        ));
    }

    #[test]
    fn terminal_plan_plus_task_sync_failure_reports_durable_partial_state() {
        let (task, plan) = ready_pair();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .task_repository()
            .fail_update_at(2, TaskRepositoryError::LockPoisoned);
        let runtime = RecordingRuntime::new(
            service.plan_repository().clone(),
            order,
            [Ok(runtime_success("operation-1", json!({"done": true})))],
        );

        let error = service
            .execute_plan(&task.id, &plan.id, &runtime)
            .unwrap_err();

        assert!(matches!(
            error,
            TaskPlanOrchestrationError::PlanTerminalButTaskSynchronizationFailed {
                runtime_error: None,
                durable_state: DurableTaskPlanState {
                    task_status: TaskStatus::Executing,
                    plan_status: PlanStatus::Completed,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn start_execution_persists_executing_plan_then_executing_task() {
        let (task, plan) = ready_pair();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        let result = service.start_execution(&task.id, &plan.id).unwrap();

        assert_eq!(result.task.status, TaskStatus::Executing);
        assert_eq!(result.plan.status, PlanStatus::Executing);
        assert_eq!(service.task_repository().snapshot(), Some(result.task));
        assert_eq!(service.plan_repository().snapshot(), Some(result.plan));
        assert_eq!(*order.lock().unwrap(), vec!["plan", "task"]);
    }

    #[test]
    fn task_load_failure_is_typed_and_writes_nothing() {
        let (task, plan) = ready_pair();
        let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .task_repository()
            .fail_get(TaskRepositoryError::LockPoisoned);

        assert!(matches!(
            service.start_execution(&task.id, &plan.id),
            Err(TaskPlanOrchestrationError::TaskLoad(
                TaskRepositoryError::LockPoisoned
            ))
        ));
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.plan_repository().writes(), 0);
    }

    #[test]
    fn plan_load_failure_is_typed_and_writes_nothing() {
        let (task, plan) = ready_pair();
        let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .plan_repository()
            .fail_get(PlanRepositoryError::LockPoisoned);

        assert!(matches!(
            service.start_execution(&task.id, &plan.id),
            Err(TaskPlanOrchestrationError::PlanLoad(
                PlanRepositoryError::LockPoisoned
            ))
        ));
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.plan_repository().writes(), 0);
    }

    fn assert_policy_rejection(task: Task, plan: Plan) -> TaskPlanExecutionError {
        let original_task = task.clone();
        let original_plan = plan.clone();
        let task_id = task.id.clone();
        let plan_id = plan.id.clone();
        let (service, _) = orchestrator(Some(task), Some(plan));

        let error = service.start_execution(&task_id, &plan_id).unwrap_err();
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.plan_repository().writes(), 0);
        assert_eq!(service.task_repository().snapshot(), Some(original_task));
        assert_eq!(service.plan_repository().snapshot(), Some(original_plan));

        match error {
            TaskPlanOrchestrationError::Policy(error) => error,
            other => panic!("expected policy error, got {other:?}"),
        }
    }

    #[test]
    fn start_rejects_task_without_active_plan_without_writes() {
        let task = Task::new(TaskType::Do, "execute workflow").unwrap();
        let mut plan = Plan::new(task.id.clone(), 1, "execute workflow").unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        assert!(matches!(
            assert_policy_rejection(task, plan),
            TaskPlanExecutionError::TaskHasNoActivePlan { .. }
        ));
    }

    #[test]
    fn start_rejects_active_plan_mismatch_without_writes() {
        let (mut task, plan) = ready_pair();
        task.activate_plan(PlanId::new());
        assert!(matches!(
            assert_policy_rejection(task, plan),
            TaskPlanExecutionError::ActivePlanMismatch { .. }
        ));
    }

    #[test]
    fn start_rejects_plan_task_mismatch_without_writes() {
        let (mut task, _) = ready_pair();
        let mut plan = Plan::new(TaskId::new(), 1, "other task").unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        task.activate_plan(plan.id.clone());
        assert!(matches!(
            assert_policy_rejection(task, plan),
            TaskPlanExecutionError::PlanTaskMismatch { .. }
        ));
    }

    #[test]
    fn start_rejects_non_ready_task_without_writes() {
        let (mut task, plan) = ready_pair();
        task.transition_to(TaskStatus::Executing).unwrap();
        assert!(matches!(
            assert_policy_rejection(task, plan),
            TaskPlanExecutionError::TaskNotReady { .. }
        ));
    }

    #[test]
    fn start_rejects_non_ready_plan_without_writes() {
        let (task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        assert!(matches!(
            assert_policy_rejection(task, plan),
            TaskPlanExecutionError::PlanNotReady { .. }
        ));
    }

    #[test]
    fn first_plan_persistence_failure_prevents_task_write() {
        let (task, plan) = ready_pair();
        let original_task = task.clone();
        let original_plan = plan.clone();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .plan_repository()
            .fail_update(PlanRepositoryError::LockPoisoned);

        assert!(matches!(
            service.start_execution(&task.id, &plan.id),
            Err(TaskPlanOrchestrationError::PlanPersistence {
                source: PlanRepositoryError::LockPoisoned,
                ..
            })
        ));
        assert_eq!(*order.lock().unwrap(), vec!["plan"]);
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.task_repository().snapshot(), Some(original_task));
        assert_eq!(service.plan_repository().snapshot(), Some(original_plan));
    }

    #[test]
    fn second_task_persistence_failure_reports_partial_persistence() {
        let (task, plan) = ready_pair();
        let original_task = task.clone();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));
        service
            .task_repository()
            .fail_update(TaskRepositoryError::LockPoisoned);

        let error = service.start_execution(&task.id, &plan.id).unwrap_err();
        assert!(matches!(
            error,
            TaskPlanOrchestrationError::PartialPersistence {
                persisted: TaskPlanAggregate::Plan,
                failed: TaskPlanAggregate::Task,
                source: TaskRepositoryError::LockPoisoned,
            }
        ));
        assert!(error.source().is_some());
        assert_eq!(*order.lock().unwrap(), vec!["plan", "task"]);
        assert_eq!(
            service.plan_repository().snapshot().unwrap().status,
            PlanStatus::Executing
        );
        assert_eq!(service.task_repository().snapshot(), Some(original_task));
    }

    #[test]
    fn completed_plan_moves_task_to_verifying_and_writes_only_task() {
        let (task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Completed).unwrap();
        let (service, order) = orchestrator(Some(task.clone()), Some(plan.clone()));

        assert_eq!(
            service
                .synchronize_after_plan_change(&task.id, &plan.id)
                .unwrap(),
            TaskPlanSynchronization::TaskBecameVerifying
        );
        assert_eq!(
            service.task_repository().snapshot().unwrap().status,
            TaskStatus::Verifying
        );
        assert_eq!(service.task_repository().writes(), 1);
        assert_eq!(service.plan_repository().writes(), 0);
        assert_eq!(*order.lock().unwrap(), vec!["task"]);
    }

    #[test]
    fn failed_plan_moves_task_to_failed_and_writes_only_task() {
        let (task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Failed).unwrap();
        let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));

        assert_eq!(
            service
                .synchronize_after_plan_change(&task.id, &plan.id)
                .unwrap(),
            TaskPlanSynchronization::TaskFailed
        );
        assert_eq!(
            service.task_repository().snapshot().unwrap().status,
            TaskStatus::Failed
        );
        assert_eq!(service.task_repository().writes(), 1);
        assert_eq!(service.plan_repository().writes(), 0);
    }

    #[test]
    fn executing_plan_is_unchanged_and_writes_nothing() {
        let (task, plan) = executing_pair();
        let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));

        assert_eq!(
            service
                .synchronize_after_plan_change(&task.id, &plan.id)
                .unwrap(),
            TaskPlanSynchronization::Unchanged
        );
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.plan_repository().writes(), 0);
    }

    #[test]
    fn synchronization_errors_preserve_policy_type_and_write_nothing() {
        let cases = [PlanStatus::Cancelled, PlanStatus::Ready];

        for status in cases {
            let (mut task, mut plan) = executing_pair();
            if status == PlanStatus::Cancelled {
                plan.transition_to(PlanStatus::Cancelled).unwrap();
            } else {
                let (_, ready_plan) = ready_pair();
                plan = ready_plan;
                task.activate_plan(plan.id.clone());
            }
            let original_task = task.clone();
            let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));
            assert!(matches!(
                service.synchronize_after_plan_change(&task.id, &plan.id),
                Err(TaskPlanOrchestrationError::Policy(_))
            ));
            assert_eq!(service.task_repository().writes(), 0);
            assert_eq!(service.plan_repository().writes(), 0);
            assert_eq!(service.task_repository().snapshot(), Some(original_task));
        }
    }

    #[test]
    fn synchronization_rejects_non_executing_task_and_identity_mismatch() {
        let (ready_task, ready_plan) = ready_pair();
        let (service, _) = orchestrator(Some(ready_task.clone()), Some(ready_plan.clone()));
        assert!(matches!(
            service.synchronize_after_plan_change(&ready_task.id, &ready_plan.id),
            Err(TaskPlanOrchestrationError::Policy(
                TaskPlanExecutionError::TaskNotExecuting { .. }
            ))
        ));
        assert_eq!(service.task_repository().writes(), 0);

        let (mut task, plan) = executing_pair();
        task.activate_plan(PlanId::new());
        let (service, _) = orchestrator(Some(task.clone()), Some(plan.clone()));
        assert!(matches!(
            service.synchronize_after_plan_change(&task.id, &plan.id),
            Err(TaskPlanOrchestrationError::Policy(
                TaskPlanExecutionError::ActivePlanMismatch { .. }
            ))
        ));
        assert_eq!(service.task_repository().writes(), 0);
        assert_eq!(service.plan_repository().writes(), 0);
    }
}
