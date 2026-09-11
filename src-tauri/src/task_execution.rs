use crate::{
    planner::{InMemoryPlanRepository, PlanRepository, PlanStep, PlannerService},
    runtime::{
        executor::{OperationEventEmitter, RuntimeExecutionState},
        plan_runtime_bridge::{PlanRuntimeExecutor, RuntimeBackedPlanExecutor},
    },
    task_engine::{
        InMemoryTaskEventBus, InMemoryTaskRepository, Task, TaskId, TaskLifecycleManager,
        TaskRepository, TaskRepositoryError, TaskStatus, TaskType,
    },
    task_plan_orchestration::{
        TaskPlanExecutionOrchestrator, TaskPlanExecutionStart, TaskPlanOrchestrationError,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt, sync::Arc};
use tauri::State;

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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitChatTaskRequest {
    prompt: String,
    task_type: TaskType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitChatTaskResponse {
    task_id: String,
    status: TaskStatus,
    task_type: TaskType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatTaskLifecycleInput {
    task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompleteChatTaskInput {
    task_id: String,
    provider_id: String,
    model_id: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FailChatTaskInput {
    task_id: String,
    reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecuteWorkTaskInput {
    task_id: String,
    agent_id: String,
    capability: Option<String>,
    input: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    user_confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecuteWorkTaskResponse {
    task_id: String,
    plan_id: String,
    agent_id: String,
    status: TaskStatus,
    output: Option<serde_json::Value>,
}

fn build_work_task_step(
    capability: Option<&str>,
    input: Option<&HashMap<String, serde_json::Value>>,
    user_confirmed: bool,
) -> Result<PlanStep, String> {
    if let Some(capability) = capability.map(str::trim).filter(|value| !value.is_empty()) {
        let mut step = PlanStep::new("Execute Core Skill", capability)
            .map_err(|error| error.to_string())?
            .with_description("Expose the requested AI-OS Skill capability to the selected Agent.")
            .with_user_confirmation(user_confirmed);

        if let Some(input) = input {
            step.input.extend(input.clone());
        }

        return Ok(step);
    }

    PlanStep::new("Execute Work Task", "agent.execute")
        .map_err(|error| error.to_string())
        .map(|step| {
            step.with_description(
                "Execute the Plan through the selected Agent without inventing a Skill.",
            )
        })
}

fn resolve_chat_task_id(
    state: &TaskExecutionState,
    task_id: &str,
) -> Result<crate::task_engine::domain::TaskId, String> {
    state
        .lifecycle
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|task| task.id.as_str() == task_id)
        .map(|task| task.id)
        .ok_or_else(|| "chat Task was not found".to_owned())
}

fn transition_chat_task(
    state: &TaskExecutionState,
    task_id: &str,
    statuses: &[TaskStatus],
) -> Result<SubmitChatTaskResponse, String> {
    let task_id = resolve_chat_task_id(state, task_id)?;
    let mut task = None;
    for status in statuses {
        task = Some(
            state
                .lifecycle
                .transition(&task_id, *status)
                .map_err(|error| error.to_string())?,
        );
    }
    let task = task.ok_or_else(|| "chat Task transition is empty".to_owned())?;
    Ok(SubmitChatTaskResponse {
        task_id: task.id.to_string(),
        status: task.status,
        task_type: task.task_type,
    })
}

fn store_chat_task_result(
    state: &TaskExecutionState,
    task_id: &str,
    result: serde_json::Value,
) -> Result<(), String> {
    let task_id = resolve_chat_task_id(state, task_id)?;
    let mut task = state
        .lifecycle
        .get(&task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "chat Task was not found".to_owned())?;
    task.result = Some(result);
    state.tasks.update(task).map_err(|error| error.to_string())
}

fn submit_chat_task_inner(
    state: &TaskExecutionState,
    request: SubmitChatTaskRequest,
) -> Result<SubmitChatTaskResponse, String> {
    let task = Task::new(request.task_type, request.prompt).map_err(|error| error.to_string())?;
    let task_id = state
        .lifecycle
        .create(task)
        .map_err(|error| error.to_string())?;

    state
        .lifecycle
        .transition(&task_id, TaskStatus::Understanding)
        .map_err(|error| error.to_string())?;

    let next = match request.task_type {
        TaskType::Ask => TaskStatus::Ready,
        TaskType::Do => TaskStatus::Planning,
    };
    let task = state
        .lifecycle
        .transition(&task_id, next)
        .map_err(|error| error.to_string())?;

    Ok(SubmitChatTaskResponse {
        task_id: task.id.to_string(),
        status: task.status,
        task_type: task.task_type,
    })
}

#[tauri::command]
pub(crate) fn submit_chat_task(
    request: SubmitChatTaskRequest,
    state: State<'_, TaskExecutionState>,
) -> Result<SubmitChatTaskResponse, String> {
    submit_chat_task_inner(&state, request)
}

#[tauri::command]
pub(crate) fn start_chat_task_execution(
    state: tauri::State<'_, TaskExecutionState>,
    input: ChatTaskLifecycleInput,
) -> Result<SubmitChatTaskResponse, String> {
    transition_chat_task(&state, input.task_id.trim(), &[TaskStatus::Executing])
}

#[tauri::command]
pub(crate) fn complete_chat_task_execution(
    state: tauri::State<'_, TaskExecutionState>,
    input: CompleteChatTaskInput,
) -> Result<SubmitChatTaskResponse, String> {
    transition_chat_task(&state, input.task_id.trim(), &[TaskStatus::Verifying])?;
    store_chat_task_result(
        &state,
        input.task_id.trim(),
        serde_json::json!({
            "kind": "ai-center-answer",
            "providerId": input.provider_id,
            "modelId": input.model_id,
            "text": input.text,
        }),
    )?;
    transition_chat_task(&state, input.task_id.trim(), &[TaskStatus::Completed])
}

#[tauri::command]
pub(crate) fn fail_chat_task_execution(
    state: tauri::State<'_, TaskExecutionState>,
    input: FailChatTaskInput,
) -> Result<SubmitChatTaskResponse, String> {
    let response = transition_chat_task(&state, input.task_id.trim(), &[TaskStatus::Failed])?;
    store_chat_task_result(
        &state,
        input.task_id.trim(),
        serde_json::json!({
            "kind": "ai-center-error",
            "reason": input.reason,
        }),
    )?;
    Ok(response)
}

#[tauri::command]
pub(crate) fn execute_chat_work_task(
    state: tauri::State<'_, TaskExecutionState>,
    input: ExecuteWorkTaskInput,
) -> Result<ExecuteWorkTaskResponse, String> {
    execute_chat_work_task_inner(&state, input)
}

fn execute_chat_work_task_inner(
    state: &TaskExecutionState,
    input: ExecuteWorkTaskInput,
) -> Result<ExecuteWorkTaskResponse, String> {
    let agent_id = input.agent_id.trim();
    if agent_id.is_empty() {
        return Err("The selected Agent id is empty".to_owned());
    }
    let task_id = resolve_chat_task_id(&state, input.task_id.trim())?;
    let task = state
        .tasks
        .get(&task_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "chat Task was not found".to_owned())?;
    if task.task_type != TaskType::Do || task.status != TaskStatus::Planning {
        return Err("Only a planning Work task can be sent to an Agent".to_owned());
    }

    let planner = PlannerService::new(Arc::clone(&state.tasks), Arc::clone(&state.plans));
    let mut plan = planner
        .create_plan(&task_id, task.intent.clone())
        .map_err(|error| error.to_string())?;
    plan.agent_id = Some(agent_id.to_owned());

    let step = build_work_task_step(
        input.capability.as_deref(),
        input.input.as_ref(),
        input.user_confirmed,
    )?;
    plan.add_step(step).map_err(|error| error.to_string())?;
    state
        .plans
        .update(plan.clone())
        .map_err(|error| error.to_string())?;
    let plan = planner
        .validate_plan(&plan.id)
        .map_err(|error| error.to_string())?;
    planner
        .activate_plan(&task_id, &plan.id)
        .map_err(|error| error.to_string())?;
    state
        .lifecycle
        .transition(&task_id, TaskStatus::Ready)
        .map_err(|error| error.to_string())?;
    let execution = state
        .service()
        .execute_task(&task_id)
        .map_err(|error| error.to_string())?;
    let output = execution
        .plan
        .steps
        .last()
        .and_then(|step| step.output.clone());

    Ok(ExecuteWorkTaskResponse {
        task_id: execution.task.id.to_string(),
        plan_id: execution.plan.id.to_string(),
        agent_id: agent_id.to_owned(),
        status: execution.task.status,
        output,
    })
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
    let runtime_executor = RuntimeBackedPlanExecutor::production(runtime, Arc::clone(&emitter));
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

    #[test]
    fn submit_chat_task_creates_ask_task_ready_for_ai_center() {
        let state = build_task_execution_state(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter::default()),
        );

        let response = submit_chat_task_inner(
            &state,
            SubmitChatTaskRequest {
                prompt: "Explain this file".to_owned(),
                task_type: TaskType::Ask,
            },
        )
        .unwrap();

        assert_eq!(response.status, TaskStatus::Ready);
        assert_eq!(response.task_type, TaskType::Ask);
        let stored = state.task_repository().list().unwrap();
        assert!(stored
            .iter()
            .any(|task| task.id.to_string() == response.task_id));
    }

    #[test]
    fn ask_task_reaches_completed_after_ai_center_answer() {
        let state = build_task_execution_state(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter::default()),
        );
        let submitted = submit_chat_task_inner(
            &state,
            SubmitChatTaskRequest {
                prompt: "Explain this file".to_owned(),
                task_type: TaskType::Ask,
            },
        )
        .unwrap();

        let executing =
            transition_chat_task(&state, &submitted.task_id, &[TaskStatus::Executing]).unwrap();
        assert_eq!(executing.status, TaskStatus::Executing);
        store_chat_task_result(
            &state,
            &submitted.task_id,
            serde_json::json!({
                "kind": "ai-center-answer",
                "providerId": "openai",
                "modelId": "test-model",
                "text": "answer"
            }),
        )
        .unwrap();

        let completed = transition_chat_task(
            &state,
            &submitted.task_id,
            &[TaskStatus::Verifying, TaskStatus::Completed],
        )
        .unwrap();
        assert_eq!(completed.status, TaskStatus::Completed);
        let stored = state
            .task_repository()
            .list()
            .unwrap()
            .into_iter()
            .find(|task| task.id.to_string() == submitted.task_id)
            .unwrap();
        assert_eq!(
            stored.result.unwrap()["text"],
            serde_json::Value::String("answer".to_owned())
        );
    }

    #[test]
    fn ask_task_failure_is_terminal_when_ai_center_fails() {
        let state = build_task_execution_state(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter::default()),
        );
        let submitted = submit_chat_task_inner(
            &state,
            SubmitChatTaskRequest {
                prompt: "Explain this file".to_owned(),
                task_type: TaskType::Ask,
            },
        )
        .unwrap();
        transition_chat_task(&state, &submitted.task_id, &[TaskStatus::Executing]).unwrap();

        let failed =
            transition_chat_task(&state, &submitted.task_id, &[TaskStatus::Failed]).unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
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

    #[derive(Default)]
    struct CapturingRuntime(Mutex<Vec<PlanRuntimeExecutionRequest>>);

    impl PlanRuntimeExecutor for CapturingRuntime {
        fn execute_step(
            &self,
            request: PlanRuntimeExecutionRequest,
        ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError> {
            self.0.lock().unwrap().push(request);
            Ok(PlanRuntimeExecutionResult {
                operation_id: "captured-attempt".to_owned(),
                output: Some(json!({"done": true})),
            })
        }
    }

    #[test]
    fn explicit_core_skill_capability_and_input_reach_plan_runtime_request() {
        let tasks = Arc::new(InMemoryTaskRepository::new());
        let plans = Arc::new(InMemoryPlanRepository::new());
        let runtime = Arc::new(CapturingRuntime::default());
        let lifecycle =
            TaskLifecycleManager::new(Arc::clone(&tasks), Arc::new(InMemoryTaskEventBus::new()));
        let state = TaskExecutionState {
            tasks: Arc::clone(&tasks),
            plans: Arc::clone(&plans),
            lifecycle,
            service: TaskExecutionService {
                tasks,
                plans,
                runtime: runtime.clone(),
            },
        };
        let submitted = submit_chat_task_inner(
            &state,
            SubmitChatTaskRequest {
                prompt: "scan the requested folder".to_owned(),
                task_type: TaskType::Do,
            },
        )
        .unwrap();
        let response = execute_chat_work_task_inner(
            &state,
            ExecuteWorkTaskInput {
                task_id: submitted.task_id,
                agent_id: "openclaw".to_owned(),
                capability: Some(" filesystem.scan ".to_owned()),
                input: Some(HashMap::from([(
                    "path".to_owned(),
                    json!("/Users/example/Documents"),
                )])),
                user_confirmed: true,
            },
        )
        .unwrap();

        let requests = runtime.0.lock().unwrap();
        assert_eq!(response.status, TaskStatus::Verifying);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].capability, "filesystem.scan");
        assert_eq!(
            requests[0].input.get("path"),
            Some(&json!("/Users/example/Documents"))
        );
        assert_eq!(requests[0].agent_id.as_deref(), Some("openclaw"));
        assert_eq!(requests[0].task_id.as_str(), response.task_id);
        assert!(requests[0].user_confirmed);
        assert!(!requests[0].input.contains_key("agentId"));
    }

    #[test]
    fn missing_core_skill_capability_plans_generic_agent_execution() {
        let step = build_work_task_step(Some("  "), None, true).unwrap();

        assert_eq!(step.capability, "agent.execute");
        assert!(!step.input.contains_key("agentId"));
        assert!(!step.input.contains_key("message"));
        assert!(!step.input.contains_key("idempotencyKey"));
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

    #[test]
    #[ignore = "requires an active OpenClaw gateway and P15 download fixture"]
    fn real_download_runs_through_task_plan_runtime_and_openclaw() {
        let source = std::env::var("AI_OS_DOWNLOAD_E2E_SOURCE").expect("source missing");
        let destination =
            std::env::var("AI_OS_DOWNLOAD_E2E_DESTINATION").expect("destination missing");
        let state = build_task_execution_state(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter::default()),
        );
        let submitted = submit_chat_task_inner(
            &state,
            SubmitChatTaskRequest {
                prompt: format!("Download {source}"),
                task_type: TaskType::Do,
            },
        )
        .unwrap();

        let response = execute_chat_work_task_inner(
            &state,
            ExecuteWorkTaskInput {
                task_id: submitted.task_id,
                agent_id: "openclaw".to_owned(),
                capability: Some("download.start".to_owned()),
                input: Some(HashMap::from([
                    ("source".to_owned(), json!(source)),
                    ("destination".to_owned(), json!(destination)),
                ])),
                user_confirmed: true,
            },
        )
        .unwrap();

        assert_eq!(response.status, TaskStatus::Verifying);
        assert_eq!(response.output.as_ref().unwrap()["kind"], "download");
    }
}
