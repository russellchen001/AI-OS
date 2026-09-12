use super::{
    agent_execution::{
        negotiate_agent_compatibility, select_skill_transport, AgentCapabilities, AgentCapability,
        AgentCompatibility, AgentExecutionAdapter, AgentExecutionError, AgentExecutionErrorKind,
        AgentExecutionProgress, AgentExecutionRequest, AgentExecutionResult, AgentId,
        AgentProbeResult,
    },
    agent_skill_transport::{
        AgentSkillTransportAdapter, OpenClawAgentSkillTransport, RuntimeSkillInvocationGateway,
    },
    executor::{OperationEventEmitter, RuntimeExecutionState},
    mano_fallback::{
        ManagedManoCliFallback, ManoFallbackAdapter, ManoFallbackError, ManoFallbackErrorKind,
        ManoFallbackRequest,
    },
    openclaw_execution::{
        OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
        OpenClawExecutionProgress, OpenClawExecutionRequest,
    },
    openclaw_gateway_adapter::OpenClawGatewayExecutionAdapter,
    skill_invocation::{SkillInvocationContext, SkillInvocationGateway},
    skills,
};
use crate::{
    planner::{PlanId, PlanStepId, StepInput, StepOutput},
    task_engine::TaskId,
};
use serde_json::{json, Map, Value};
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;

const CONTROL_PLANE_AGENT_EXECUTE: &str = "agent.execute";

#[derive(Debug, Clone, PartialEq)]
pub struct PlanRuntimeExecutionRequest {
    pub task_id: TaskId,
    pub plan_id: PlanId,
    pub step_id: PlanStepId,
    pub agent_id: Option<String>,
    pub goal: String,
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
    NoViableExecutionPath {
        message: String,
    },
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
            Self::Configuration => formatter.write_str("Runtime configuration is unavailable"),
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
            Self::PermissionDenied => formatter.write_str("Runtime permission was denied."),
            Self::NoViableExecutionPath { message }
            | Self::Admission { message, .. }
            | Self::Runtime { message, .. } => formatter.write_str(message),
        }
    }
}

impl Error for PlanRuntimeExecutionError {}

pub trait PlanRuntimeExecutor: Send + Sync {
    fn execute_step(
        &self,
        request: PlanRuntimeExecutionRequest,
    ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError>;

    fn cancel_execution(&self, execution_id: &str) -> Result<(), PlanRuntimeExecutionError> {
        let _ = execution_id;
        Err(PlanRuntimeExecutionError::InvalidRequest)
    }
}

/// OpenClaw implementation of the generic Agent contract.
///
/// The OpenClaw-specific protocol stays behind this Adapter. Task Engine,
/// Planner and Plan Runtime never depend on an OpenClaw version string.
struct OpenClawAgentExecutionBridge {
    downstream: Arc<dyn OpenClawExecutionAdapter>,
    skill_transport: Option<Arc<dyn AgentSkillTransportAdapter>>,
    skill_gateway: Option<Arc<dyn SkillInvocationGateway>>,
}

impl OpenClawAgentExecutionBridge {
    fn production(runtime: RuntimeExecutionState, emitter: Arc<dyn OperationEventEmitter>) -> Self {
        Self {
            downstream: Arc::new(OpenClawGatewayExecutionAdapter),
            skill_transport: Some(Arc::new(OpenClawAgentSkillTransport::production())),
            skill_gateway: Some(Arc::new(RuntimeSkillInvocationGateway::production(
                runtime, emitter,
            ))),
        }
    }

    #[cfg(test)]
    fn with_downstream(downstream: Arc<dyn OpenClawExecutionAdapter>) -> Self {
        Self {
            downstream,
            skill_transport: None,
            skill_gateway: None,
        }
    }
}

impl AgentExecutionAdapter for OpenClawAgentExecutionBridge {
    fn probe(&self, agent_id: &AgentId) -> Result<AgentProbeResult, AgentExecutionError> {
        if agent_id.as_str() != "openclaw" {
            return Err(AgentExecutionError::new(
                AgentExecutionErrorKind::ExecutionRejected,
                format!(
                    "No Agent Runtime Adapter is registered for {}.",
                    agent_id.as_str()
                ),
                false,
            ));
        }

        Ok(AgentProbeResult {
            agent_id: agent_id.clone(),
            // Do not inspect or gate on an exact OpenClaw version here.
            version: None,
            capabilities: AgentCapabilities::new([
                AgentCapability::TaskExecution,
                AgentCapability::StructuredToolInvocation,
                AgentCapability::NativeSkillTransport,
                AgentCapability::ProgressEvents,
                AgentCapability::DurableSession,
            ]),
        })
    }

    fn execute(
        &self,
        request: &AgentExecutionRequest,
        report: &mut dyn FnMut(AgentExecutionProgress),
    ) -> Result<AgentExecutionResult, AgentExecutionError> {
        if request.agent_id.as_str() != "openclaw" {
            return Err(AgentExecutionError::new(
                AgentExecutionErrorKind::ExecutionRejected,
                format!(
                    "No Agent Runtime Adapter is registered for {}.",
                    request.agent_id.as_str()
                ),
                false,
            ));
        }

        if !request.allowed_capabilities.is_empty() {
            let transport = self.skill_transport.as_ref().ok_or_else(|| {
                AgentExecutionError::new(
                    AgentExecutionErrorKind::ExecutionRejected,
                    "Agent Skill transport is unavailable.",
                    false,
                )
            })?;
            let gateway = self.skill_gateway.as_ref().ok_or_else(|| {
                AgentExecutionError::new(
                    AgentExecutionErrorKind::ExecutionRejected,
                    "Skill Invocation Gateway is unavailable.",
                    false,
                )
            })?;

            return transport.execute(
                request,
                &request.skill_invocation_context,
                gateway.as_ref(),
                report,
            );
        }

        let openclaw_request = OpenClawExecutionRequest::new(
            request.execution_id.clone(),
            "sessions.create",
            json!({
                "message": request.goal,
                "agentId": request.agent_id.as_str(),
                "label": "AI-OS Work",
                "idempotencyKey": request.execution_id,
            }),
        )
        .map_err(map_openclaw_error)?;

        let result = self
            .downstream
            .execute(&openclaw_request, &mut |progress| {
                report(map_openclaw_progress(progress))
            })
            .map_err(map_openclaw_error)?;

        let session_id = result
            .output
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);

        Ok(AgentExecutionResult {
            session_id,
            output: result.output,
        })
    }
}

fn map_openclaw_progress(progress: OpenClawExecutionProgress) -> AgentExecutionProgress {
    AgentExecutionProgress {
        phase: progress.phase,
        message: progress.message,
    }
}

fn map_openclaw_error(error: OpenClawExecutionError) -> AgentExecutionError {
    let kind = match error.kind {
        OpenClawExecutionErrorKind::InvalidRequest => AgentExecutionErrorKind::InvalidRequest,
        OpenClawExecutionErrorKind::PermissionRequired
        | OpenClawExecutionErrorKind::PermissionDenied => AgentExecutionErrorKind::PermissionDenied,
        OpenClawExecutionErrorKind::AuthenticationRequired => {
            AgentExecutionErrorKind::AuthenticationRequired
        }
        OpenClawExecutionErrorKind::PairingRequired => AgentExecutionErrorKind::PairingRequired,
        OpenClawExecutionErrorKind::ConnectionUnavailable
        | OpenClawExecutionErrorKind::ProtocolFailure => {
            AgentExecutionErrorKind::ConnectionUnavailable
        }
        OpenClawExecutionErrorKind::ExecutionRejected => AgentExecutionErrorKind::ExecutionRejected,
        OpenClawExecutionErrorKind::ExecutionFailed => AgentExecutionErrorKind::ExecutionFailed,
    };

    AgentExecutionError::new(kind, error.message, error.retryable)
}

pub(crate) struct RuntimeBackedPlanExecutor {
    agent_adapter: Arc<dyn AgentExecutionAdapter>,
    mano_fallback: Arc<dyn ManoFallbackAdapter>,
}

impl RuntimeBackedPlanExecutor {
    pub(crate) fn production(
        runtime: RuntimeExecutionState,
        emitter: Arc<dyn OperationEventEmitter>,
    ) -> Self {
        Self {
            agent_adapter: Arc::new(OpenClawAgentExecutionBridge::production(runtime, emitter)),
            mano_fallback: Arc::new(ManagedManoCliFallback::production()),
        }
    }

    #[cfg(test)]
    fn with_agent_adapter(agent_adapter: Arc<dyn AgentExecutionAdapter>) -> Self {
        Self {
            agent_adapter,
            mano_fallback: Arc::new(RejectingManoFallback),
        }
    }

    #[cfg(test)]
    fn with_adapters(
        agent_adapter: Arc<dyn AgentExecutionAdapter>,
        mano_fallback: Arc<dyn ManoFallbackAdapter>,
    ) -> Self {
        Self {
            agent_adapter,
            mano_fallback,
        }
    }
}

impl PlanRuntimeExecutor for RuntimeBackedPlanExecutor {
    fn execute_step(
        &self,
        request: PlanRuntimeExecutionRequest,
    ) -> Result<PlanRuntimeExecutionResult, PlanRuntimeExecutionError> {
        let capability = request.capability.trim().to_owned();

        if capability.is_empty() {
            return Err(PlanRuntimeExecutionError::InvalidRequest);
        }

        let allowed_capabilities = if capability == CONTROL_PLANE_AGENT_EXECUTE {
            Vec::new()
        } else {
            skills::resolver::resolve(&capability).ok_or_else(|| {
                PlanRuntimeExecutionError::SkillNotFound {
                    capability: capability.clone(),
                }
            })?;

            vec![capability.clone()]
        };

        let raw_agent_id = request
            .agent_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(PlanRuntimeExecutionError::InvalidRequest)?;

        let agent_id =
            AgentId::new(raw_agent_id).map_err(|_| PlanRuntimeExecutionError::InvalidRequest)?;

        let probe = self.agent_adapter.probe(&agent_id).map_err(agent_error)?;

        let mut required = vec![AgentCapability::TaskExecution];
        if !allowed_capabilities.is_empty() {
            required.push(AgentCapability::StructuredToolInvocation);
        }

        match negotiate_agent_compatibility(
            &probe,
            &required,
            &[
                AgentCapability::NativeSkillTransport,
                AgentCapability::McpSkillTransport,
                AgentCapability::Cancellation,
                AgentCapability::ProgressEvents,
                AgentCapability::DurableSession,
            ],
        ) {
            AgentCompatibility::Compatible | AgentCompatibility::Degraded { .. } => {}
            AgentCompatibility::Incompatible { missing_required } => {
                return Err(PlanRuntimeExecutionError::Admission {
                    message: format!(
                        "Selected Agent is missing required capabilities: {missing_required:?}"
                    ),
                    retryable: false,
                });
            }
        }

        if !allowed_capabilities.is_empty() && select_skill_transport(&probe.capabilities).is_none()
        {
            return Err(PlanRuntimeExecutionError::Admission {
                message: "Selected Agent has no negotiated Skill transport.".to_owned(),
                retryable: false,
            });
        }

        let attempt_id = Uuid::new_v4().to_string();
        let operation_id = operation_identity(&request.plan_id, &request.step_id, &attempt_id);
        let skill_input = Value::Object(request.input.clone().into_iter().collect::<Map<_, _>>());

        let context = if capability == CONTROL_PLANE_AGENT_EXECUTE {
            json!({
                "source": "ai-os-plan-runtime"
            })
        } else {
            json!({
                "source": "ai-os-plan-runtime",
                "requestedSkill": {
                    "capability": capability,
                    "input": skill_input,
                }
            })
        };

        let skill_invocation_context = SkillInvocationContext::new(
            request.task_id.as_str(),
            request.plan_id.as_str(),
            operation_id.clone(),
            agent_id.as_str(),
            allowed_capabilities.clone(),
            request.user_confirmed,
        )
        .map_err(|_| PlanRuntimeExecutionError::InvalidRequest)?;

        let agent_request = AgentExecutionRequest::new(
            operation_id.clone(),
            request.task_id.as_str(),
            request.plan_id.as_str(),
            request.step_id.as_str(),
            agent_id,
            request.goal.clone(),
            allowed_capabilities,
            context,
            skill_invocation_context,
        )
        .map_err(|_| PlanRuntimeExecutionError::InvalidRequest)?;

        let result = match self.agent_adapter.execute(&agent_request, &mut |_| {}) {
            Ok(result) => result,
            Err(error) if error.kind == AgentExecutionErrorKind::NoViableExecutionPath => {
                let fallback_request = ManoFallbackRequest {
                    execution_id: operation_id.clone(),
                    task_id: request.task_id.as_str().to_owned(),
                    plan_id: request.plan_id.as_str().to_owned(),
                    step_id: request.step_id.as_str().to_owned(),
                    goal: request.goal,
                    capability,
                    input: skill_input,
                    user_confirmed: request.user_confirmed,
                };
                let fallback = self
                    .mano_fallback
                    .execute(&fallback_request, &mut |_| {})
                    .map_err(mano_error)?;
                AgentExecutionResult {
                    session_id: None,
                    output: fallback.output,
                }
            }
            Err(error) => return Err(agent_error(error)),
        };

        Ok(PlanRuntimeExecutionResult {
            operation_id,
            output: Some(result.output),
        })
    }

    fn cancel_execution(&self, execution_id: &str) -> Result<(), PlanRuntimeExecutionError> {
        self.mano_fallback.cancel(execution_id).map_err(mano_error)
    }
}

fn operation_identity(plan_id: &PlanId, step_id: &PlanStepId, attempt_id: &str) -> String {
    format!(
        "agent-execution:{}:{}:{}:{}:{}:{}",
        plan_id.as_str().len(),
        plan_id.as_str(),
        step_id.as_str().len(),
        step_id.as_str(),
        attempt_id.len(),
        attempt_id,
    )
}

fn agent_error(error: AgentExecutionError) -> PlanRuntimeExecutionError {
    match error.kind {
        AgentExecutionErrorKind::InvalidRequest => PlanRuntimeExecutionError::InvalidRequest,
        AgentExecutionErrorKind::PermissionDenied => PlanRuntimeExecutionError::PermissionDenied,
        AgentExecutionErrorKind::NoViableExecutionPath => {
            PlanRuntimeExecutionError::NoViableExecutionPath {
                message: error.message,
            }
        }
        AgentExecutionErrorKind::AuthenticationRequired
        | AgentExecutionErrorKind::PairingRequired
        | AgentExecutionErrorKind::ConnectionUnavailable
        | AgentExecutionErrorKind::ExecutionRejected
        | AgentExecutionErrorKind::ExecutionFailed => PlanRuntimeExecutionError::Runtime {
            message: error.message,
            retryable: error.retryable,
        },
    }
}

fn mano_error(error: ManoFallbackError) -> PlanRuntimeExecutionError {
    match error.kind {
        ManoFallbackErrorKind::InvalidRequest => PlanRuntimeExecutionError::InvalidRequest,
        ManoFallbackErrorKind::PermissionDenied => PlanRuntimeExecutionError::PermissionDenied,
        ManoFallbackErrorKind::Unsupported
        | ManoFallbackErrorKind::Unavailable
        | ManoFallbackErrorKind::AlreadyRunning
        | ManoFallbackErrorKind::Cancelled
        | ManoFallbackErrorKind::TimedOut
        | ManoFallbackErrorKind::ExecutionFailed => PlanRuntimeExecutionError::Runtime {
            message: error.message,
            retryable: error.retryable,
        },
    }
}

#[cfg(test)]
struct RejectingManoFallback;

#[cfg(test)]
impl ManoFallbackAdapter for RejectingManoFallback {
    fn probe(&self) -> Result<super::mano_fallback::ManoProbeResult, ManoFallbackError> {
        Err(ManoFallbackError {
            kind: ManoFallbackErrorKind::Unavailable,
            message: "Mano fallback was not configured for this test.".to_owned(),
            retryable: false,
        })
    }

    fn execute(
        &self,
        _request: &ManoFallbackRequest,
        _report: &mut dyn FnMut(super::mano_fallback::ManoFallbackProgress),
    ) -> Result<super::mano_fallback::ManoFallbackResult, ManoFallbackError> {
        Err(ManoFallbackError {
            kind: ManoFallbackErrorKind::Unavailable,
            message: "Mano fallback was not configured for this test.".to_owned(),
            retryable: false,
        })
    }

    fn cancel(&self, _execution_id: &str) -> Result<(), ManoFallbackError> {
        Err(ManoFallbackError {
            kind: ManoFallbackErrorKind::InvalidRequest,
            message: "No Mano fallback execution is active.".to_owned(),
            retryable: false,
        })
    }
}

#[cfg(test)]
mod mano_fallback_error_contract_tests {
    use super::*;

    fn agent_error_of(kind: AgentExecutionErrorKind) -> PlanRuntimeExecutionError {
        agent_error(AgentExecutionError::new(kind, "test failure", false))
    }

    #[test]
    fn only_no_viable_execution_path_keeps_a_dedicated_plan_runtime_signal() {
        assert!(matches!(
            agent_error_of(AgentExecutionErrorKind::NoViableExecutionPath),
            PlanRuntimeExecutionError::NoViableExecutionPath { .. }
        ));

        for kind in [
            AgentExecutionErrorKind::AuthenticationRequired,
            AgentExecutionErrorKind::PairingRequired,
            AgentExecutionErrorKind::ConnectionUnavailable,
            AgentExecutionErrorKind::ExecutionRejected,
            AgentExecutionErrorKind::ExecutionFailed,
        ] {
            assert!(matches!(
                agent_error_of(kind),
                PlanRuntimeExecutionError::Runtime { .. }
            ));
        }
    }

    #[test]
    fn permission_and_invalid_request_never_become_mano_fallback_signal() {
        assert_eq!(
            agent_error_of(AgentExecutionErrorKind::PermissionDenied),
            PlanRuntimeExecutionError::PermissionDenied
        );

        assert_eq!(
            agent_error_of(AgentExecutionErrorKind::InvalidRequest),
            PlanRuntimeExecutionError::InvalidRequest
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    struct NoopEmitter;

    impl OperationEventEmitter for NoopEmitter {
        fn emit(
            &self,
            _snapshot: super::super::models::RuntimeOperationSnapshot,
        ) -> Result<(), ()> {
            Ok(())
        }
    }

    struct RecordingAgent {
        probe: AgentProbeResult,
        requests: Mutex<Vec<AgentExecutionRequest>>,
    }

    impl RecordingAgent {
        fn compatible(agent_id: &str) -> Self {
            Self {
                probe: AgentProbeResult {
                    agent_id: AgentId::new(agent_id).unwrap(),
                    version: Some("future-version-is-metadata-only".to_owned()),
                    capabilities: AgentCapabilities::new([
                        AgentCapability::TaskExecution,
                        AgentCapability::StructuredToolInvocation,
                        AgentCapability::NativeSkillTransport,
                        AgentCapability::ProgressEvents,
                    ]),
                },
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl AgentExecutionAdapter for RecordingAgent {
        fn probe(&self, _agent_id: &AgentId) -> Result<AgentProbeResult, AgentExecutionError> {
            Ok(self.probe.clone())
        }

        fn execute(
            &self,
            request: &AgentExecutionRequest,
            _report: &mut dyn FnMut(AgentExecutionProgress),
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            self.requests.lock().unwrap().push(request.clone());
            Ok(AgentExecutionResult {
                session_id: Some("session-test".to_owned()),
                output: json!({"agentOwned": true}),
            })
        }
    }

    struct FailingAgent {
        probe: AgentProbeResult,
        kind: AgentExecutionErrorKind,
    }

    impl FailingAgent {
        fn new(kind: AgentExecutionErrorKind) -> Self {
            Self {
                probe: RecordingAgent::compatible("openclaw").probe,
                kind,
            }
        }
    }

    impl AgentExecutionAdapter for FailingAgent {
        fn probe(&self, _agent_id: &AgentId) -> Result<AgentProbeResult, AgentExecutionError> {
            Ok(self.probe.clone())
        }

        fn execute(
            &self,
            _request: &AgentExecutionRequest,
            _report: &mut dyn FnMut(AgentExecutionProgress),
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            Err(AgentExecutionError::new(
                self.kind.clone(),
                "selected Agent reported no execution path",
                false,
            ))
        }
    }

    struct RecordingMano {
        calls: AtomicUsize,
        requests: Mutex<Vec<ManoFallbackRequest>>,
        cancellations: Mutex<Vec<String>>,
        error: Option<ManoFallbackError>,
    }

    impl RecordingMano {
        fn successful() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                requests: Mutex::new(Vec::new()),
                cancellations: Mutex::new(Vec::new()),
                error: None,
            }
        }
    }

    impl ManoFallbackAdapter for RecordingMano {
        fn probe(&self) -> Result<super::super::mano_fallback::ManoProbeResult, ManoFallbackError> {
            Ok(super::super::mano_fallback::ManoProbeResult {
                mode: super::super::mano_fallback::ManoMode::Local,
                concurrency: 1,
                primary_display_only: true,
            })
        }

        fn execute(
            &self,
            request: &ManoFallbackRequest,
            _report: &mut dyn FnMut(super::super::mano_fallback::ManoFallbackProgress),
        ) -> Result<super::super::mano_fallback::ManoFallbackResult, ManoFallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.requests.lock().unwrap().push(request.clone());
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            Ok(super::super::mano_fallback::ManoFallbackResult {
                output: json!({"executor": "mano-cua", "status": "completed"}),
            })
        }

        fn cancel(&self, execution_id: &str) -> Result<(), ManoFallbackError> {
            self.cancellations
                .lock()
                .unwrap()
                .push(execution_id.to_owned());
            Ok(())
        }
    }

    fn request(capability: &str) -> PlanRuntimeExecutionRequest {
        PlanRuntimeExecutionRequest {
            task_id: TaskId::new(),
            plan_id: PlanId::from_static("plan-a"),
            step_id: PlanStepId::from_static("step-a"),
            agent_id: Some("openclaw".to_owned()),
            goal: "Complete the user's requested work".to_owned(),
            capability: capability.to_owned(),
            input: [("path".to_owned(), json!("/safe"))].into_iter().collect(),
            user_confirmed: false,
        }
    }

    #[test]
    fn mp1_only_no_viable_execution_path_invokes_mano_exactly_once() {
        let mano = Arc::new(RecordingMano::successful());
        let executor = RuntimeBackedPlanExecutor::with_adapters(
            Arc::new(FailingAgent::new(
                AgentExecutionErrorKind::NoViableExecutionPath,
            )),
            mano.clone(),
        );
        let mut fallback_request = request("filesystem.scan");
        fallback_request.user_confirmed = true;
        let task_id = fallback_request.task_id.as_str().to_owned();
        let plan_id = fallback_request.plan_id.as_str().to_owned();
        let step_id = fallback_request.step_id.as_str().to_owned();

        let result = executor.execute_step(fallback_request).unwrap();

        assert_eq!(mano.calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.output.unwrap()["executor"], "mano-cua");
        let received = mano.requests.lock().unwrap();
        assert_eq!(received[0].task_id, task_id);
        assert_eq!(received[0].plan_id, plan_id);
        assert_eq!(received[0].step_id, step_id);
        assert_eq!(received[0].capability, "filesystem.scan");
        assert!(received[0].user_confirmed);
    }

    #[test]
    fn mp1_every_other_agent_outcome_bypasses_mano() {
        for kind in [
            AgentExecutionErrorKind::InvalidRequest,
            AgentExecutionErrorKind::PermissionDenied,
            AgentExecutionErrorKind::AuthenticationRequired,
            AgentExecutionErrorKind::PairingRequired,
            AgentExecutionErrorKind::ConnectionUnavailable,
            AgentExecutionErrorKind::ExecutionRejected,
            AgentExecutionErrorKind::ExecutionFailed,
        ] {
            let mano = Arc::new(RecordingMano::successful());
            let executor = RuntimeBackedPlanExecutor::with_adapters(
                Arc::new(FailingAgent::new(kind)),
                mano.clone(),
            );

            assert!(executor.execute_step(request("filesystem.scan")).is_err());
            assert_eq!(mano.calls.load(Ordering::SeqCst), 0);
        }

        let mano = Arc::new(RecordingMano::successful());
        let executor = RuntimeBackedPlanExecutor::with_adapters(
            Arc::new(RecordingAgent::compatible("openclaw")),
            mano.clone(),
        );
        executor.execute_step(request("filesystem.scan")).unwrap();
        assert_eq!(mano.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn mp1_mano_unavailable_is_a_normalized_terminal_runtime_error() {
        let mano = Arc::new(RecordingMano {
            calls: AtomicUsize::new(0),
            requests: Mutex::new(Vec::new()),
            cancellations: Mutex::new(Vec::new()),
            error: Some(ManoFallbackError {
                kind: ManoFallbackErrorKind::Unavailable,
                message: "Mano-CUA is unavailable or not ready.".to_owned(),
                retryable: true,
            }),
        });
        let executor = RuntimeBackedPlanExecutor::with_adapters(
            Arc::new(FailingAgent::new(
                AgentExecutionErrorKind::NoViableExecutionPath,
            )),
            mano.clone(),
        );
        let mut fallback_request = request("filesystem.scan");
        fallback_request.user_confirmed = true;

        assert!(matches!(
            executor.execute_step(fallback_request),
            Err(PlanRuntimeExecutionError::Runtime {
                retryable: true,
                ..
            })
        ));
        assert_eq!(mano.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn mp1_plan_runtime_routes_cancellation_to_mano() {
        let mano = Arc::new(RecordingMano::successful());
        let executor = RuntimeBackedPlanExecutor::with_adapters(
            Arc::new(RecordingAgent::compatible("openclaw")),
            mano.clone(),
        );

        executor.cancel_execution("execution-1").unwrap();

        assert_eq!(
            mano.cancellations.lock().unwrap().as_slice(),
            ["execution-1"]
        );
    }

    #[test]
    #[ignore = "requires explicit Mano Cloud authorization and macOS GUI permissions"]
    fn mp5_real_openclaw_to_mano_cloud_textedit_completes_and_is_verified() {
        assert_eq!(env::var("AI_OS_MANO_MODE").as_deref(), Ok("cloud"));
        assert_eq!(
            env::var("AI_OS_MANO_CLOUD_AUTHORIZED").as_deref(),
            Ok("true")
        );

        let output_path = PathBuf::from(
            env::var("AI_OS_MANO_CLOUD_E2E_OUTPUT_PATH")
                .expect("the verifier must provide a dedicated output path"),
        );
        let output_directory = output_path
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .expect("the verifier output must have a dedicated directory");
        assert!(output_directory.starts_with("ai-os-mano-cloud-e2e."));
        assert_eq!(
            output_path.file_name().and_then(|name| name.to_str()),
            Some("AI_OS_MANO_CLOUD_E2E.txt")
        );
        assert_eq!(fs::metadata(&output_path).unwrap().len(), 0);

        let marker = "AI_OS_MANO_CLOUD_E2E_20260913";
        let executor = RuntimeBackedPlanExecutor::production(
            RuntimeExecutionState::default(),
            Arc::new(NoopEmitter),
        );
        let mut cloud_request = request("filesystem.scan");
        cloud_request.user_confirmed = true;
        cloud_request.goal = format!(
            "Use GUI actions only. The frontmost app is TextEdit with the dedicated empty local test file {} already open. Click inside the empty document, type exactly {marker}, press Command-S, confirm the document is saved, then finish. Do not use Terminal, shell, a browser, accounts, messages, Save As, or any other document.",
            output_path.display()
        );

        let result = executor.execute_step(cloud_request).unwrap();

        assert_eq!(
            result.output,
            Some(json!({
                "executor": "mano-cua",
                "mode": "cloud",
                "status": "completed"
            }))
        );
        assert_eq!(fs::read_to_string(&output_path).unwrap(), marker);
    }

    #[test]
    fn registered_skill_enters_selected_agent_instead_of_backend_dispatch() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        let result = executor.execute_step(request("filesystem.scan")).unwrap();

        assert_eq!(result.output, Some(json!({"agentOwned": true})));

        let received = agent.requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].agent_id.as_str(), "openclaw");
        assert_eq!(received[0].allowed_capabilities, vec!["filesystem.scan"]);
        assert_eq!(
            received[0].context["requestedSkill"]["capability"],
            "filesystem.scan"
        );
    }

    #[test]
    fn browser_skill_no_longer_dispatches_directly_to_mcp() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        executor.execute_step(request("browser.search")).unwrap();

        let received = agent.requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].allowed_capabilities, vec!["browser.search"]);
    }

    #[test]
    fn mp0_planner_dispatches_computer_use_to_selected_agent() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());
        let mut request = request("computer.use.execute");
        request.input = [
            (
                "task".to_owned(),
                json!("Interact with the fixture application"),
            ),
            ("goal".to_owned(), json!("Complete the visual fixture")),
            ("allowedApplications".to_owned(), json!(["Fixture App"])),
            ("maxSteps".to_owned(), json!(4)),
            ("maxDurationMs".to_owned(), json!(2_000)),
        ]
        .into_iter()
        .collect();

        executor.execute_step(request).unwrap();

        let received = agent.requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].agent_id.as_str(), "openclaw");
        assert_eq!(
            received[0].allowed_capabilities,
            vec!["computer.use.execute"]
        );
        assert_eq!(
            received[0].context["requestedSkill"]["capability"],
            "computer.use.execute"
        );
    }

    #[test]
    fn local_model_skill_no_longer_dispatches_directly_to_ollama() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        executor.execute_step(request("models.list")).unwrap();

        let received = agent.requests.lock().unwrap();
        assert_eq!(received[0].allowed_capabilities, vec!["models.list"]);
    }

    #[test]
    fn media_skill_no_longer_dispatches_directly_to_media_router() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        executor
            .execute_step(request("media.text-to-image"))
            .unwrap();

        let received = agent.requests.lock().unwrap();
        assert_eq!(
            received[0].allowed_capabilities,
            vec!["media.text-to-image"]
        );
    }

    #[test]
    fn unknown_skill_is_rejected_before_agent_execution() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        assert_eq!(
            executor
                .execute_step(request("unknown.capability"))
                .unwrap_err(),
            PlanRuntimeExecutionError::SkillNotFound {
                capability: "unknown.capability".to_owned()
            }
        );

        assert!(agent.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn control_plane_agent_execute_requires_no_fake_skill() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());

        executor
            .execute_step(request(CONTROL_PLANE_AGENT_EXECUTE))
            .unwrap();

        let received = agent.requests.lock().unwrap();
        assert!(received[0].allowed_capabilities.is_empty());
        assert!(received[0].context.get("requestedSkill").is_none());
    }

    #[test]
    fn missing_selected_agent_fails_closed() {
        let agent = Arc::new(RecordingAgent::compatible("openclaw"));
        let executor = RuntimeBackedPlanExecutor::with_agent_adapter(agent.clone());
        let mut missing = request("filesystem.scan");
        missing.agent_id = None;

        assert_eq!(
            executor.execute_step(missing).unwrap_err(),
            PlanRuntimeExecutionError::InvalidRequest
        );

        assert!(agent.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn openclaw_bridge_calls_agent_session_not_requested_skill_action() {
        struct RecordingOpenClaw {
            calls: Mutex<Vec<OpenClawExecutionRequest>>,
        }

        impl OpenClawExecutionAdapter for RecordingOpenClaw {
            fn execute(
                &self,
                request: &OpenClawExecutionRequest,
                _report: &mut dyn FnMut(OpenClawExecutionProgress),
            ) -> Result<
                crate::runtime::openclaw_execution::OpenClawExecutionResult,
                OpenClawExecutionError,
            > {
                self.calls.lock().unwrap().push(request.clone());
                Ok(
                    crate::runtime::openclaw_execution::OpenClawExecutionResult {
                        output: json!({"sessionId": "session-1"}),
                        summary: None,
                    },
                )
            }
        }

        let downstream = Arc::new(RecordingOpenClaw {
            calls: Mutex::new(Vec::new()),
        });
        let bridge = OpenClawAgentExecutionBridge::with_downstream(downstream.clone());

        let agent_request = AgentExecutionRequest::new(
            "execution-1",
            "task-1",
            "plan-1",
            "step-1",
            AgentId::new("openclaw").unwrap(),
            "Do the work",
            Vec::new(),
            json!({}),
            SkillInvocationContext::new(
                "task-1",
                "plan-1",
                "execution-1",
                "openclaw",
                Vec::new(),
                false,
            )
            .unwrap(),
        )
        .unwrap();

        bridge.execute(&agent_request, &mut |_| {}).unwrap();

        let calls = downstream.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].action.as_str(), "sessions.create");
        assert_ne!(calls[0].action.as_str(), "browser.search");
    }
}
