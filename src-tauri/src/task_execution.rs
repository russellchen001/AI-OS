use crate::{
    planner::InMemoryPlanRepository,
    runtime::{
        executor::{OperationEventEmitter, RuntimeExecutionState},
        plan_runtime_bridge::{PlanRuntimeExecutor, RuntimeBackedPlanExecutor},
    },
    task_engine::{
        InMemoryTaskEventBus, InMemoryTaskRepository, TaskId, TaskLifecycleManager, TaskRepository,
        TaskRepositoryError,
    },
    task_plan_orchestration::{
        TaskPlanExecutionOrchestrator, TaskPlanExecutionStart, TaskPlanOrchestrationError,
    },
};
use std::{error::Error, fmt, sync::Arc};

/// Process-local P11 composition. Task and Plan state is intentionally lost when
/// the application exits; persistent repositories are deferred beyond P11.
pub(crate) struct TaskExecutionState {
    tasks: Arc<InMemoryTaskRepository>,
    plans: Arc<InMemoryPlanRepository>,
    #[allow(dead_code)]
    lifecycle: TaskLifecycleManager<Arc<InMemoryTaskRepository>, Arc<InMemoryTaskEventBus>>,
    service: TaskExecutionService,
}

impl TaskExecutionState {
    pub(crate) fn service(&self) -> &TaskExecutionService {
        &self.service
    }

    #[cfg(test)]
    fn task_repository(&self) -> Arc<InMemoryTaskRepository> {
        Arc::clone(&self.tasks)
    }

    #[cfg(test)]
    fn plan_repository(&self) -> Arc<InMemoryPlanRepository> {
        Arc::clone(&self.plans)
    }
}

pub(crate) struct TaskExecutionService {
    tasks: Arc<InMemoryTaskRepository>,
    plans: Arc<InMemoryPlanRepository>,
    runtime: Arc<dyn PlanRuntimeExecutor>,
}

impl TaskExecutionService {
    pub(crate) fn execute_task(
        &self,
        task_id: &TaskId,
    ) -> Result<TaskPlanExecutionStart, TaskExecutionServiceError> {
        let task = self
            .tasks
            .get(task_id)?
            .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))?;
        let plan_id = task
            .active_plan_id
            .clone()
            .ok_or_else(|| TaskExecutionServiceError::NoActivePlan(task_id.clone()))?;
        TaskPlanExecutionOrchestrator::new(Arc::clone(&self.tasks), Arc::clone(&self.plans))
            .execute_plan(task_id, &plan_id, self.runtime.as_ref())
            .map_err(TaskExecutionServiceError::Orchestration)
    }
}

#[derive(Debug)]
pub(crate) enum TaskExecutionServiceError {
    TaskRepository(TaskRepositoryError),
    NoActivePlan(TaskId),
    Orchestration(TaskPlanOrchestrationError),
}

impl fmt::Display for TaskExecutionServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskRepository(error) => write!(formatter, "failed to load Task: {error}"),
            Self::NoActivePlan(task_id) => write!(formatter, "Task {task_id} has no active Plan"),
            Self::Orchestration(error) => write!(formatter, "Task execution failed: {error}"),
        }
    }
}

impl Error for TaskExecutionServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TaskRepository(error) => Some(error),
            Self::NoActivePlan(_) => None,
            Self::Orchestration(error) => Some(error),
        }
    }
}

impl From<TaskRepositoryError> for TaskExecutionServiceError {
    fn from(error: TaskRepositoryError) -> Self {
        Self::TaskRepository(error)
    }
}

pub(crate) fn build_task_execution_state(
    runtime: RuntimeExecutionState,
    emitter: Arc<dyn OperationEventEmitter>,
) -> TaskExecutionState {
    let tasks = Arc::new(InMemoryTaskRepository::new());
    let plans = Arc::new(InMemoryPlanRepository::new());
    let lifecycle =
        TaskLifecycleManager::new(Arc::clone(&tasks), Arc::new(InMemoryTaskEventBus::new()));
    let runtime_executor =
        RuntimeBackedPlanExecutor::from_persisted_settings(runtime.clone(), Arc::clone(&emitter))
            .unwrap_or_else(|_| RuntimeBackedPlanExecutor::deny_all(runtime, emitter));
    let service = TaskExecutionService {
        tasks: Arc::clone(&tasks),
        plans: Arc::clone(&plans),
        runtime: Arc::new(runtime_executor),
    };
    TaskExecutionState {
        tasks,
        plans,
        lifecycle,
        service,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::models::RuntimeOperationSnapshot;
    use crate::{
        planner::{Plan, PlanRepository, PlanStatus, PlanStep},
        runtime::plan_runtime_bridge::{
            PlanRuntimeExecutionError, PlanRuntimeExecutionRequest, PlanRuntimeExecutionResult,
        },
        task_engine::{Task, TaskStatus, TaskType},
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingEmitter(Mutex<Vec<RuntimeOperationSnapshot>>);

    impl OperationEventEmitter for RecordingEmitter {
        fn emit(&self, snapshot: RuntimeOperationSnapshot) -> Result<(), ()> {
            self.0.lock().unwrap().push(snapshot);
            Ok(())
        }
    }

    #[test]
    fn production_builder_constructs_shared_process_local_repositories_and_service() {
        let state = build_task_execution_state(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter::default()),
        );

        assert!(Arc::ptr_eq(&state.tasks, &state.task_repository()));
        assert!(Arc::ptr_eq(&state.plans, &state.plan_repository()));
        let _ = state.service();
    }

    struct SuccessfulRuntime;

    impl PlanRuntimeExecutor for SuccessfulRuntime {
        fn execute_step(
            &self,
            _request: PlanRuntimeExecutionRequest,
        ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError> {
            Ok(PlanRuntimeExecutionResult {
                operation_id: "test-attempt".to_owned(),
                output: Some(json!({"done": true})),
            })
        }
    }

    #[test]
    fn service_resolves_active_plan_and_reaches_orchestrator() {
        let tasks = Arc::new(InMemoryTaskRepository::new());
        let plans = Arc::new(InMemoryPlanRepository::new());
        let mut task = Task::new(TaskType::Do, "execute").unwrap();
        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();
        let mut plan = Plan::new(task.id.clone(), 1, "execute").unwrap();
        plan.add_step(PlanStep::new("run", "test.run").unwrap())
            .unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        task.activate_plan(plan.id.clone());
        task.transition_to(TaskStatus::Ready).unwrap();
        tasks.create(task.clone()).unwrap();
        plans.create(plan).unwrap();
        let service = TaskExecutionService {
            tasks,
            plans,
            runtime: Arc::new(SuccessfulRuntime),
        };

        let result = service.execute_task(&task.id).unwrap();

        assert_eq!(result.plan.status, PlanStatus::Completed);
        assert_eq!(result.task.status, TaskStatus::Verifying);
    }
}
