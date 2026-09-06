use super::{
    capability_permission::{
        CapabilityPermissionDecision, ConfiguredCapabilityPermissionGate,
        GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
    },
    executor::{
        execute_generative_media_runtime_task, execute_local_model_runtime_task,
        execute_mcp_runtime_task, execute_runtime_task,
        OperationEventEmitter, RuntimeExecutionState, RuntimeTaskExecutionRequest,
        RuntimeTaskExecutionResult,
    },
    models::{NormalizedRuntimeError, RuntimeErrorCode},
    openclaw_execution::OpenClawExecutionAdapter,
    openclaw_gateway_adapter::OpenClawGatewayExecutionAdapter,
    openclaw_permission::{
        OpenClawPermissionGate, PermissionEnforcingOpenClawExecutionAdapter,
    },
    skills,
    trusted_automation::{load_trusted_automation_settings, TrustedAutomationConfigError},
};
use crate::browser::runtime::execute_browser_capability;
use crate::planner::{PlanId, PlanStepId, StepInput, StepOutput};
use serde_json::{Map, Value};
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct PlanRuntimeExecutionRequest {
    pub plan_id: PlanId,
    pub step_id: PlanStepId,
    pub capability: String,
    pub input: StepInput,
    pub user_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanRuntimeExecutionResult {
    pub operation_id: String,
    pub output: Option<StepOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRuntimeExecutionError {
    Configuration,
    InvalidRequest,
    SkillNotFound {
        capability: String,
    },
    UnsupportedExecutor {
        capability: String,
        executor: String,
    },
    PermissionDenied,
    Admission {
        message: String,
        retryable: bool,
    },
    Runtime {
        message: String,
        retryable: bool,
    },
}

impl fmt::Display for PlanRuntimeExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => {
                formatter.write_str("trusted automation configuration is unavailable")
            }
            Self::InvalidRequest => formatter.write_str("Plan runtime request is invalid"),
            Self::SkillNotFound { capability } => {
                write!(
                    formatter,
                    "No registered Skill supports capability: {capability}"
                )
            }
            Self::UnsupportedExecutor {
                capability,
                executor,
            } => write!(
                formatter,
                "Skill capability {capability} requires unsupported executor: {executor}"
            ),
            Self::PermissionDenied => formatter.write_str("OpenClaw action is not permitted."),
            Self::Admission { message, .. } | Self::Runtime { message, .. } => {
                formatter.write_str(message)
            }
        }
    }
}

impl Error for PlanRuntimeExecutionError {}

pub trait PlanRuntimeExecutor: Send + Sync {
    fn execute_step(
        &self,
        request: PlanRuntimeExecutionRequest,
    ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError>;
}

pub(crate) struct RuntimeBackedPlanExecutor {
    runtime: RuntimeExecutionState,
    emitter: Arc<dyn OperationEventEmitter>,
    adapter: Arc<dyn OpenClawExecutionAdapter>,
    permission_gate: Arc<ConfiguredCapabilityPermissionGate>,
}

impl RuntimeBackedPlanExecutor {
    pub(crate) fn from_persisted_settings(
        runtime: RuntimeExecutionState,
        emitter: Arc<dyn OperationEventEmitter>,
    ) -> Result<Self, TrustedAutomationConfigError> {
        let settings = load_trusted_automation_settings()?;
        let gate = Arc::new(ConfiguredCapabilityPermissionGate::new(
            settings.allowed_capabilities(),
        ));
        let openclaw_gate: Arc<dyn OpenClawPermissionGate> = gate.clone();
        let downstream: Arc<dyn OpenClawExecutionAdapter> =
            Arc::new(OpenClawGatewayExecutionAdapter);
        let adapter: Arc<dyn OpenClawExecutionAdapter> = Arc::new(
            PermissionEnforcingOpenClawExecutionAdapter::new(openclaw_gate, downstream),
        );
        Ok(Self {
            runtime,
            emitter,
            adapter,
            permission_gate: gate,
        })
    }

    pub(crate) fn deny_all(
        runtime: RuntimeExecutionState,
        emitter: Arc<dyn OperationEventEmitter>,
    ) -> Self {
        let gate = Arc::new(ConfiguredCapabilityPermissionGate::new(Vec::new()));
        let openclaw_gate: Arc<dyn OpenClawPermissionGate> = gate.clone();
        let downstream: Arc<dyn OpenClawExecutionAdapter> =
            Arc::new(OpenClawGatewayExecutionAdapter);
        let adapter: Arc<dyn OpenClawExecutionAdapter> = Arc::new(
            PermissionEnforcingOpenClawExecutionAdapter::new(openclaw_gate, downstream),
        );
        Self {
            runtime,
            emitter,
            adapter,
            permission_gate: gate,
        }
    }

    #[cfg(test)]
    fn with_adapter(
        runtime: RuntimeExecutionState,
        emitter: Arc<dyn OperationEventEmitter>,
        adapter: Arc<dyn OpenClawExecutionAdapter>,
    ) -> Self {
        Self {
            runtime,
            emitter,
            adapter,
            permission_gate: Arc::new(
                ConfiguredCapabilityPermissionGate::new(Vec::new()),
            ),
        }
    }
}

impl PlanRuntimeExecutor for RuntimeBackedPlanExecutor {
    fn execute_step(
        &self,
        request: PlanRuntimeExecutionRequest,
    ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError> {
        let capability = request.capability.trim().to_owned();
        let skill = skills::resolver::resolve(&capability).ok_or_else(|| {
            PlanRuntimeExecutionError::SkillNotFound {
                capability: capability.clone(),
            }
        })?;

        let executor_kind = skill.executor.kind.clone();
        let handler = skill.executor.handler.clone();

        let attempt_id = Uuid::new_v4().to_string();
        let operation_id = operation_identity(&request.plan_id, &request.step_id, &attempt_id);
        let input = Value::Object(request.input.into_iter().collect::<Map<_, _>>());

        let runtime_request = RuntimeTaskExecutionRequest {
            operation_id,
            plan_id: request.plan_id.as_str().to_owned(),
            step_id: request.step_id.as_str().to_owned(),
            capability: capability.clone(),
            input,
            user_confirmed: request.user_confirmed,
        };

        match executor_kind.as_str() {
            "openclaw" => execute_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
                Arc::clone(&self.adapter),
            ),

            "local" if handler == "ollama" => execute_local_model_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
            ),

            "media" if handler == "generative-media" => {
                match self.permission_gate.authorize_with_policy(
                    &capability,
                    runtime_request.user_confirmed,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                ) {
                    CapabilityPermissionDecision::Allowed => {}
                    CapabilityPermissionDecision::RequiresApproval
                    | CapabilityPermissionDecision::Denied => {
                        return Err(PlanRuntimeExecutionError::PermissionDenied);
                    }
                }

                execute_generative_media_runtime_task(
                    self.runtime.manager(),
                    self.runtime.scheduler(),
                    Arc::clone(&self.emitter),
                    runtime_request,
                    Arc::new(
                        crate::generative_media::registry::MediaProviderRegistry::new(),
                    ),
                )
            }

            "mcp" => execute_mcp_runtime_task(
                self.runtime.manager(),
                self.runtime.scheduler(),
                Arc::clone(&self.emitter),
                runtime_request,
            ),

            _ => {
                return Err(PlanRuntimeExecutionError::UnsupportedExecutor {
                    capability,
                    executor: executor_kind,
                });
            }
        }
        .map(runtime_result)
        .map_err(runtime_error)
    }
}

fn operation_identity(plan_id: &PlanId, step_id: &PlanStepId, attempt_id: &str) -> String {
    format!(
        "plan-step:{}:{}:{}:{}:{}:{}",
        plan_id.as_str().len(),
        plan_id.as_str(),
        step_id.as_str().len(),
        step_id.as_str(),
        attempt_id.len(),
        attempt_id,
    )
}

fn runtime_result(result: RuntimeTaskExecutionResult) -> PlanRuntimeExecutionResult {
    PlanRuntimeExecutionResult {
        operation_id: result.operation_id,
        output: result.output,
    }
}

fn runtime_error(error: NormalizedRuntimeError) -> PlanRuntimeExecutionError {
    match error.code {
        RuntimeErrorCode::PermissionDenied => PlanRuntimeExecutionError::PermissionDenied,
        RuntimeErrorCode::InvalidRequest => PlanRuntimeExecutionError::InvalidRequest,
        RuntimeErrorCode::OperationConflict
        | RuntimeErrorCode::OperationCapacityExceeded
        | RuntimeErrorCode::RuntimeNotFound => PlanRuntimeExecutionError::Admission {
            message: error.message,
            retryable: error.retryable,
        },
        _ => PlanRuntimeExecutionError::Runtime {
            message: error.message,
            retryable: error.retryable,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::openclaw_execution::{
        OpenClawExecutionError, OpenClawExecutionErrorKind, OpenClawExecutionProgress,
        OpenClawExecutionRequest, OpenClawExecutionResult,
    };
    use crate::runtime::openclaw_permission::{
        ConfiguredCapabilityPermissionGate, PermissionEnforcingOpenClawExecutionAdapter,
    };
    use crate::runtime::{executor::OperationEventEmitter, models::RuntimeOperationSnapshot};
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingEmitter;

    impl OperationEventEmitter for RecordingEmitter {
        fn emit(&self, _snapshot: RuntimeOperationSnapshot) -> Result<(), ()> {
            Ok(())
        }
    }

    fn bridge(adapter: Arc<dyn OpenClawExecutionAdapter>) -> RuntimeBackedPlanExecutor {
        RuntimeBackedPlanExecutor::with_adapter(
            RuntimeExecutionState::default(),
            Arc::new(RecordingEmitter),
            adapter,
        )
    }

    struct RecordingAdapter {
        requests: Mutex<Vec<OpenClawExecutionRequest>>,
        outcome: Result<OpenClawExecutionResult, OpenClawExecutionError>,
    }

    impl OpenClawExecutionAdapter for RecordingAdapter {
        fn execute(
            &self,
            request: &OpenClawExecutionRequest,
            _report: &mut dyn FnMut(OpenClawExecutionProgress),
        ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
            self.requests.lock().unwrap().push(request.clone());
            self.outcome.clone()
        }
    }

    fn request(plan: &str, step: &str) -> PlanRuntimeExecutionRequest {
        PlanRuntimeExecutionRequest {
            plan_id: PlanId::from_static(plan),
            step_id: PlanStepId::from_static(step),
            capability: "filesystem.scan".to_owned(),
            input: [("path".to_owned(), json!("/safe"))].into_iter().collect(),
            user_confirmed: false,
        }
    }

    #[test]
    fn unknown_capability_is_rejected_before_runtime_execution() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: Value::Null,
                summary: None,
            }),
        });
        let executor = bridge(adapter.clone());
        let mut unknown = request("plan", "step");
        unknown.capability = "unknown.capability".to_owned();

        assert_eq!(
            executor.execute_step(unknown),
            Err(PlanRuntimeExecutionError::SkillNotFound {
                capability: "unknown.capability".to_owned(),
            })
        );
        assert!(adapter.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn browser_skill_is_not_routed_to_openclaw() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: Value::Null,
                summary: None,
            }),
        });

        let executor = bridge(adapter.clone());

        let mut browser = request("plan", "step");
        browser.capability = "browser.search".to_owned();

        let result = executor.execute_step(browser);

        assert!(
            matches!(
                result,
                Err(PlanRuntimeExecutionError::Runtime { .. })
                    | Err(PlanRuntimeExecutionError::Admission { .. })
            ),
            "browser skill should reach MCP runtime path"
        );

        assert!(adapter.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn operation_identity_is_attempt_safe_and_traceable() {
        let first = operation_identity(
            &PlanId::from_static("plan-a"),
            &PlanStepId::from_static("step-a"),
            "attempt:一",
        );
        let same = operation_identity(
            &PlanId::from_static("plan-a"),
            &PlanStepId::from_static("step-a"),
            "attempt:一",
        );
        assert_eq!(first, same);
        assert!(first.contains("plan-a"));
        assert!(first.contains("step-a"));
        assert_ne!(
            first,
            operation_identity(
                &PlanId::from_static("plan-a"),
                &PlanStepId::from_static("step-b"),
                "attempt:一",
            )
        );
        assert_ne!(
            first,
            operation_identity(
                &PlanId::from_static("plan-b"),
                &PlanStepId::from_static("step-a"),
                "attempt:一",
            )
        );
        assert_ne!(
            first,
            operation_identity(
                &PlanId::from_static("plan-a"),
                &PlanStepId::from_static("step-a"),
                "attempt:二",
            )
        );
    }

    #[test]
    fn plan_step_maps_to_runtime_task_request_and_success() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"files": 2}),
                summary: None,
            }),
        });
        let bridge = bridge(adapter.clone());

        let result = bridge.execute_step(request("plan-a", "step-a")).unwrap();

        let received = adapter.requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action.as_str(), "filesystem.scan");
        assert_eq!(received[0].input, json!({"path": "/safe"}));
        assert_eq!(result.output, Some(json!({"files": 2})));
    }

    #[test]
    fn download_plan_step_uses_the_openclaw_execution_contract() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"kind": "download", "status": "completed"}),
                summary: None,
            }),
        });
        let bridge = bridge(adapter.clone());
        let mut download = request("plan-download", "step-download");
        download.capability = "download.start".to_owned();
        download.input = [
            ("source".to_owned(), json!("https://example.com/file.zip")),
            ("destination".to_owned(), json!("/safe/downloads")),
        ]
        .into_iter()
        .collect();
        download.user_confirmed = true;

        let result = bridge.execute_step(download).unwrap();
        let received = adapter.requests.lock().unwrap();

        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action.as_str(), "download.start");
        assert!(received[0].user_confirmed);
        assert_eq!(
            result.output,
            Some(json!({"kind": "download", "status": "completed"}))
        );
    }

    #[test]
    fn repeated_plan_step_attempts_have_distinct_retained_terminal_operations() {
        let runtime = RuntimeExecutionState::default();
        let manager = runtime.manager();
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"files": 2}),
                summary: None,
            }),
        });
        let bridge =
            RuntimeBackedPlanExecutor::with_adapter(runtime, Arc::new(RecordingEmitter), adapter);

        let first = bridge.execute_step(request("plan:一", "step:一")).unwrap();
        let second = bridge.execute_step(request("plan:一", "step:一")).unwrap();

        assert_ne!(first.operation_id, second.operation_id);
        assert!(manager.get_operation(&first.operation_id).is_ok());
        assert!(manager.get_operation(&second.operation_id).is_ok());
    }

    #[test]
    fn permission_denial_maps_to_typed_bridge_error() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::PermissionDenied,
                "OpenClaw action is not permitted.",
                false,
            )),
        });
        let bridge = bridge(adapter);

        assert_eq!(
            bridge
                .execute_step(request("plan-a", "step-a"))
                .unwrap_err(),
            PlanRuntimeExecutionError::PermissionDenied
        );
    }

    #[test]
    fn concrete_bridge_stack_enforces_configured_gate_before_downstream() {
        let downstream = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"files": 2}),
                summary: None,
            }),
        });
        let gate = Arc::new(ConfiguredCapabilityPermissionGate::new([
            "filesystem.read".to_owned()
        ]));
        let adapter: Arc<dyn OpenClawExecutionAdapter> = Arc::new(
            PermissionEnforcingOpenClawExecutionAdapter::new(gate, downstream.clone()),
        );
        let bridge = bridge(adapter);

        assert_eq!(
            bridge
                .execute_step(request("plan-a", "step-a"))
                .unwrap_err(),
            PlanRuntimeExecutionError::PermissionDenied
        );
        assert!(downstream.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn confirmed_filesystem_scan_crosses_permission_gate_with_original_input() {
        let downstream = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"files": ["a.txt"]}),
                summary: None,
            }),
        });
        let gate = Arc::new(ConfiguredCapabilityPermissionGate::new(Vec::new()));
        let adapter: Arc<dyn OpenClawExecutionAdapter> = Arc::new(
            PermissionEnforcingOpenClawExecutionAdapter::new(gate, downstream.clone()),
        );
        let bridge = bridge(adapter);
        let mut confirmed = request("plan-a", "step-a");
        confirmed.user_confirmed = true;

        let result = bridge.execute_step(confirmed).unwrap();

        assert_eq!(result.output, Some(json!({"files": ["a.txt"]})));
        let received = downstream.requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action.as_str(), "filesystem.scan");
        assert_eq!(received[0].input, json!({"path": "/safe"}));
        assert!(received[0].user_confirmed);
    }
    #[test]
    fn unconfirmed_generative_media_is_rejected_before_runtime_execution() {
        let adapter = Arc::new(RecordingAdapter {
            requests: Mutex::new(Vec::new()),
            outcome: Ok(OpenClawExecutionResult {
                output: json!({"unexpected": true}),
                summary: None,
            }),
        });

        let bridge = bridge(adapter.clone());
        let mut media = request("plan-media", "step-media");

        media.capability = "media.text-to-image".to_owned();
        media.user_confirmed = false;

        assert_eq!(
            bridge.execute_step(media).unwrap_err(),
            PlanRuntimeExecutionError::PermissionDenied
        );

        assert!(
            adapter.requests.lock().unwrap().is_empty(),
            "media denial must not enter OpenClaw execution"
        );
    }

}
