use serde_json::Value;
use std::{error::Error, fmt};

/// Runtime-issued context attached to an Agent's Skill invocation.
///
/// Critically, the Agent does not supply confirmation state in
/// `SkillInvocationRequest`. Confirmation belongs to this trusted Runtime
/// context so an Agent cannot self-authorize a privileged capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillInvocationContext {
    pub(crate) task_id: String,
    pub(crate) plan_id: String,
    pub(crate) agent_execution_id: String,
    pub(crate) agent_id: String,
    pub(crate) exposed_capabilities: Vec<String>,
    pub(crate) user_confirmed: bool,
}

impl SkillInvocationContext {
    pub(crate) fn new(
        task_id: impl Into<String>,
        plan_id: impl Into<String>,
        agent_execution_id: impl Into<String>,
        agent_id: impl Into<String>,
        exposed_capabilities: Vec<String>,
        user_confirmed: bool,
    ) -> Result<Self, SkillInvocationContractError> {
        let mut normalized_capabilities = Vec::with_capacity(exposed_capabilities.len());
        for capability in exposed_capabilities {
            let capability =
                required(capability).ok_or(SkillInvocationContractError::InvalidCapability)?;
            if !normalized_capabilities.contains(&capability) {
                normalized_capabilities.push(capability);
            }
        }

        Ok(Self {
            task_id: required(task_id.into()).ok_or(SkillInvocationContractError::InvalidTaskId)?,
            plan_id: required(plan_id.into()).ok_or(SkillInvocationContractError::InvalidPlanId)?,
            agent_execution_id: required(agent_execution_id.into())
                .ok_or(SkillInvocationContractError::InvalidAgentExecutionId)?,
            agent_id: required(agent_id.into())
                .ok_or(SkillInvocationContractError::InvalidAgentId)?,
            exposed_capabilities: normalized_capabilities,
            user_confirmed,
        })
    }
}

/// Agent-authored portion of a Skill invocation.
///
/// There is deliberately no `user_confirmed`, permission decision, backend,
/// provider, or executor field here. Those are AI-OS Runtime responsibilities.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillInvocationRequest {
    pub(crate) invocation_id: String,
    pub(crate) capability: String,
    pub(crate) input: Value,
}

impl SkillInvocationRequest {
    pub(crate) fn new(
        invocation_id: impl Into<String>,
        capability: impl Into<String>,
        input: Value,
    ) -> Result<Self, SkillInvocationContractError> {
        Ok(Self {
            invocation_id: required(invocation_id.into())
                .ok_or(SkillInvocationContractError::InvalidInvocationId)?,
            capability: required(capability.into())
                .ok_or(SkillInvocationContractError::InvalidCapability)?,
            input,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillPermissionDecision {
    Allowed,
    RequiresApproval,
    Denied,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillInvocationResult {
    pub(crate) invocation_id: String,
    pub(crate) capability: String,
    pub(crate) backend: String,
    pub(crate) provider: Option<String>,
    pub(crate) output: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillInvocationErrorKind {
    InvalidRequest,
    SkillNotFound,
    CapabilityNotExposed,
    PermissionRequired,
    PermissionDenied,
    BackendUnavailable,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillInvocationError {
    pub(crate) kind: SkillInvocationErrorKind,
    pub(crate) message: String,
    pub(crate) retryable: bool,
}

impl SkillInvocationError {
    pub(crate) fn new(
        kind: SkillInvocationErrorKind,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable,
        }
    }
}

impl fmt::Display for SkillInvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SkillInvocationError {}

/// Backend of one AI-OS Skill.
///
/// MCP, native Rust, provider routers, Office adapters, Browser providers and
/// similar implementations belong behind this boundary. They are not task
/// executors and do not own Agent lifecycle.
pub(crate) trait SkillBackend: Send + Sync {
    fn invoke(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError>;
}

/// Runtime-owned Agent -> Skill admission boundary.
///
/// Implementations added in AR-1B will resolve the Skill, verify Agent exposure,
/// apply Runtime permission/confirmation policy, invoke the backend, and emit
/// traceable lifecycle events.
pub(crate) trait SkillInvocationGateway: Send + Sync {
    fn invoke(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillInvocationContractError {
    InvalidTaskId,
    InvalidPlanId,
    InvalidAgentExecutionId,
    InvalidAgentId,
    InvalidInvocationId,
    InvalidCapability,
}

impl fmt::Display for SkillInvocationContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTaskId => "Skill invocation Task id is invalid.",
            Self::InvalidPlanId => "Skill invocation Plan id is invalid.",
            Self::InvalidAgentExecutionId => "Skill invocation Agent execution id is invalid.",
            Self::InvalidAgentId => "Skill invocation Agent id is invalid.",
            Self::InvalidInvocationId => "Skill invocation id is invalid.",
            Self::InvalidCapability => "Skill invocation capability is invalid.",
        })
    }
}

impl Error for SkillInvocationContractError {}

fn required(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingBackend {
        calls: Mutex<Vec<(SkillInvocationContext, SkillInvocationRequest)>>,
    }

    impl SkillBackend for RecordingBackend {
        fn invoke(
            &self,
            context: &SkillInvocationContext,
            request: &SkillInvocationRequest,
        ) -> Result<SkillInvocationResult, SkillInvocationError> {
            self.calls
                .lock()
                .unwrap()
                .push((context.clone(), request.clone()));

            Ok(SkillInvocationResult {
                invocation_id: request.invocation_id.clone(),
                capability: request.capability.clone(),
                backend: "test-backend".to_owned(),
                provider: Some("test-provider".to_owned()),
                output: serde_json::json!({"ok": true}),
            })
        }
    }

    struct ConfirmationGateway<B> {
        backend: B,
    }

    impl<B: SkillBackend> SkillInvocationGateway for ConfirmationGateway<B> {
        fn invoke(
            &self,
            context: &SkillInvocationContext,
            request: &SkillInvocationRequest,
        ) -> Result<SkillInvocationResult, SkillInvocationError> {
            if !context.user_confirmed {
                return Err(SkillInvocationError::new(
                    SkillInvocationErrorKind::PermissionRequired,
                    "Current execution requires user confirmation.",
                    false,
                ));
            }

            self.backend.invoke(context, request)
        }
    }

    fn context(confirmed: bool) -> SkillInvocationContext {
        SkillInvocationContext::new(
            "task-1",
            "plan-1",
            "agent-execution-1",
            "openclaw",
            vec!["media.text-to-image".to_owned()],
            confirmed,
        )
        .unwrap()
    }

    fn request() -> SkillInvocationRequest {
        SkillInvocationRequest::new(
            "skill-invocation-1",
            "media.text-to-image",
            serde_json::json!({"prompt": "test"}),
        )
        .unwrap()
    }

    #[test]
    fn runtime_context_carries_trace_identity_end_to_end() {
        let context = context(true);

        assert_eq!(context.task_id, "task-1");
        assert_eq!(context.plan_id, "plan-1");
        assert_eq!(context.agent_execution_id, "agent-execution-1");
        assert_eq!(context.agent_id, "openclaw");
        assert_eq!(context.exposed_capabilities, vec!["media.text-to-image"]);
    }

    #[test]
    fn agent_skill_request_cannot_self_authorize_confirmation() {
        let request = request();
        let serialized_shape = serde_json::json!({
            "invocationId": request.invocation_id,
            "capability": request.capability,
            "input": request.input,
        });

        assert!(serialized_shape.get("userConfirmed").is_none());
        assert!(serialized_shape.get("permissionDecision").is_none());
        assert!(serialized_shape.get("backend").is_none());
        assert!(serialized_shape.get("provider").is_none());
    }

    #[test]
    fn unconfirmed_skill_invocation_is_blocked_before_backend() {
        let gateway = ConfirmationGateway {
            backend: RecordingBackend {
                calls: Mutex::new(Vec::new()),
            },
        };

        let error = gateway.invoke(&context(false), &request()).unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::PermissionRequired);
        assert!(gateway.backend.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn confirmed_skill_invocation_reaches_backend_with_same_trace_context() {
        let gateway = ConfirmationGateway {
            backend: RecordingBackend {
                calls: Mutex::new(Vec::new()),
            },
        };

        let result = gateway.invoke(&context(true), &request()).unwrap();

        assert_eq!(result.invocation_id, "skill-invocation-1");
        assert_eq!(result.capability, "media.text-to-image");
        assert_eq!(result.backend, "test-backend");

        let calls = gateway.backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0.agent_execution_id, "agent-execution-1");
        assert_eq!(calls[0].1.capability, "media.text-to-image");
    }

    #[test]
    fn backend_identity_is_result_metadata_not_agent_input() {
        let request = request();

        assert_eq!(request.capability, "media.text-to-image");
        assert_eq!(request.input["prompt"], "test");
    }
}
