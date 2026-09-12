use serde_json::Value;
use std::{collections::BTreeSet, error::Error, fmt};

use super::skill_invocation::SkillInvocationContext;

/// Runtime-owned identity for one selected operational Agent.
///
/// The identifier is intentionally provider/runtime neutral. `openclaw` is the
/// v1 implementation, but callers must not encode OpenClaw-specific behavior
/// into Task Engine, Planner, or the shared Runtime contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentId(String);

impl AgentId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, AgentExecutionContractError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(AgentExecutionContractError::InvalidAgentId);
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One Runtime-owned execution request sent to a selected Agent.
///
/// `allowed_capabilities` is an exposure/admission boundary. It does not mean
/// that the Planner executes those capabilities. The Agent may choose whether
/// and when to request them through the Skill Invocation Gateway.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AgentExecutionRequest {
    pub(crate) execution_id: String,
    pub(crate) task_id: String,
    pub(crate) plan_id: String,
    pub(crate) step_id: String,
    pub(crate) agent_id: AgentId,
    pub(crate) goal: String,
    pub(crate) allowed_capabilities: Vec<String>,
    pub(crate) context: Value,
    pub(crate) skill_invocation_context: SkillInvocationContext,
}

impl AgentExecutionRequest {
    pub(crate) fn new(
        execution_id: impl Into<String>,
        task_id: impl Into<String>,
        plan_id: impl Into<String>,
        step_id: impl Into<String>,
        agent_id: AgentId,
        goal: impl Into<String>,
        allowed_capabilities: Vec<String>,
        context: Value,
        skill_invocation_context: SkillInvocationContext,
    ) -> Result<Self, AgentExecutionContractError> {
        let execution_id =
            required(execution_id.into()).ok_or(AgentExecutionContractError::InvalidExecutionId)?;
        let task_id = required(task_id.into()).ok_or(AgentExecutionContractError::InvalidTaskId)?;
        let plan_id = required(plan_id.into()).ok_or(AgentExecutionContractError::InvalidPlanId)?;
        let step_id = required(step_id.into()).ok_or(AgentExecutionContractError::InvalidStepId)?;
        let goal = required(goal.into()).ok_or(AgentExecutionContractError::InvalidGoal)?;

        let mut normalized_capabilities = Vec::with_capacity(allowed_capabilities.len());
        for capability in allowed_capabilities {
            let capability =
                required(capability).ok_or(AgentExecutionContractError::InvalidCapability)?;
            if !normalized_capabilities.contains(&capability) {
                normalized_capabilities.push(capability);
            }
        }

        if skill_invocation_context.task_id != task_id
            || skill_invocation_context.plan_id != plan_id
            || skill_invocation_context.agent_execution_id != execution_id
            || skill_invocation_context.agent_id != agent_id.as_str()
            || skill_invocation_context.exposed_capabilities != normalized_capabilities
        {
            return Err(AgentExecutionContractError::InvalidSkillInvocationContext);
        }

        Ok(Self {
            execution_id,
            task_id,
            plan_id,
            step_id,
            agent_id,
            goal,
            allowed_capabilities: normalized_capabilities,
            context,
            skill_invocation_context,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AgentExecutionResult {
    pub(crate) session_id: Option<String>,
    pub(crate) output: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentExecutionErrorKind {
    InvalidRequest,
    PermissionDenied,
    AuthenticationRequired,
    PairingRequired,
    ConnectionUnavailable,
    ExecutionRejected,
    NoViableExecutionPath,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentExecutionError {
    pub(crate) kind: AgentExecutionErrorKind,
    pub(crate) message: String,
    pub(crate) retryable: bool,
}

impl AgentExecutionError {
    pub(crate) fn new(
        kind: AgentExecutionErrorKind,
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

impl fmt::Display for AgentExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AgentExecutionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum AgentCapability {
    TaskExecution,
    StructuredToolInvocation,
    McpSkillTransport,
    NativeSkillTransport,
    Cancellation,
    ProgressEvents,
    DurableSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AgentCapabilities {
    supported: BTreeSet<AgentCapability>,
}

impl AgentCapabilities {
    pub(crate) fn new(supported: impl IntoIterator<Item = AgentCapability>) -> Self {
        Self {
            supported: supported.into_iter().collect(),
        }
    }

    pub(crate) fn supports(&self, capability: AgentCapability) -> bool {
        self.supported.contains(&capability)
    }

    pub(crate) fn missing(
        &self,
        required: impl IntoIterator<Item = AgentCapability>,
    ) -> Vec<AgentCapability> {
        required
            .into_iter()
            .filter(|capability| !self.supports(*capability))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentProbeResult {
    pub(crate) agent_id: AgentId,
    /// Diagnostic metadata only. Compatibility MUST NOT be inferred from this
    /// string alone.
    pub(crate) version: Option<String>,
    pub(crate) capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentCompatibility {
    Compatible,
    Degraded {
        missing_optional: Vec<AgentCapability>,
    },
    Incompatible {
        missing_required: Vec<AgentCapability>,
    },
}

pub(crate) fn negotiate_agent_compatibility(
    probe: &AgentProbeResult,
    required: &[AgentCapability],
    optional: &[AgentCapability],
) -> AgentCompatibility {
    let missing_required = probe.capabilities.missing(required.iter().copied());

    if !missing_required.is_empty() {
        return AgentCompatibility::Incompatible { missing_required };
    }

    let missing_optional = probe.capabilities.missing(optional.iter().copied());

    if missing_optional.is_empty() {
        AgentCompatibility::Compatible
    } else {
        AgentCompatibility::Degraded { missing_optional }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSkillTransport {
    Native,
    Mcp,
}

pub(crate) fn select_skill_transport(
    capabilities: &AgentCapabilities,
) -> Option<AgentSkillTransport> {
    if capabilities.supports(AgentCapability::NativeSkillTransport) {
        Some(AgentSkillTransport::Native)
    } else if capabilities.supports(AgentCapability::McpSkillTransport) {
        Some(AgentSkillTransport::Mcp)
    } else {
        None
    }
}

/// Generic Runtime-to-Agent boundary.
///
/// OpenClaw implements this contract in v1. Future operational Agents must be
/// added behind this trait rather than introducing new Task/Planner executors.
pub(crate) trait AgentExecutionAdapter: Send + Sync {
    /// Probe actual Adapter/Agent capabilities.
    ///
    /// Agent versions are diagnostic metadata only. Runtime routing must use
    /// capability negotiation rather than exact-version checks.
    fn probe(&self, agent_id: &AgentId) -> Result<AgentProbeResult, AgentExecutionError> {
        Err(AgentExecutionError::new(
            AgentExecutionErrorKind::ExecutionRejected,
            format!(
                "Agent capability probe is not implemented for {}.",
                agent_id.as_str()
            ),
            false,
        ))
    }

    fn execute(
        &self,
        request: &AgentExecutionRequest,
        report: &mut dyn FnMut(AgentExecutionProgress),
    ) -> Result<AgentExecutionResult, AgentExecutionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentExecutionProgress {
    pub(crate) phase: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentExecutionContractError {
    InvalidAgentId,
    InvalidExecutionId,
    InvalidTaskId,
    InvalidPlanId,
    InvalidStepId,
    InvalidGoal,
    InvalidCapability,
    InvalidSkillInvocationContext,
}

impl fmt::Display for AgentExecutionContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAgentId => "Agent id is invalid.",
            Self::InvalidExecutionId => "Agent execution id is invalid.",
            Self::InvalidTaskId => "Task id is invalid.",
            Self::InvalidPlanId => "Plan id is invalid.",
            Self::InvalidStepId => "Plan step id is invalid.",
            Self::InvalidGoal => "Agent goal is invalid.",
            Self::InvalidCapability => "Allowed capability is invalid.",
            Self::InvalidSkillInvocationContext => {
                "Runtime Skill invocation context does not match Agent execution."
            }
        })
    }
}

impl Error for AgentExecutionContractError {}

fn required(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingAgent {
        requests: Mutex<Vec<AgentExecutionRequest>>,
    }

    impl AgentExecutionAdapter for RecordingAgent {
        fn execute(
            &self,
            request: &AgentExecutionRequest,
            report: &mut dyn FnMut(AgentExecutionProgress),
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            self.requests.lock().unwrap().push(request.clone());
            report(AgentExecutionProgress {
                phase: "executing".to_owned(),
                message: "Agent accepted task.".to_owned(),
            });
            Ok(AgentExecutionResult {
                session_id: Some("session-1".to_owned()),
                output: serde_json::json!({"ok": true}),
            })
        }
    }

    fn request() -> AgentExecutionRequest {
        AgentExecutionRequest::new(
            "execution-1",
            "task-1",
            "plan-1",
            "step-1",
            AgentId::new("openclaw").unwrap(),
            "Create the requested report",
            vec!["document.read".to_owned(), "document.create".to_owned()],
            serde_json::json!({"source": "chat"}),
            SkillInvocationContext::new(
                "task-1",
                "plan-1",
                "execution-1",
                "openclaw",
                vec!["document.read".to_owned(), "document.create".to_owned()],
                true,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn selected_agent_identity_is_part_of_every_agent_execution_request() {
        let request = request();

        assert_eq!(request.agent_id.as_str(), "openclaw");
        assert_eq!(request.task_id, "task-1");
        assert_eq!(request.plan_id, "plan-1");
        assert_eq!(request.step_id, "step-1");
    }

    #[test]
    fn allowed_capabilities_are_constraints_not_terminal_executors() {
        let request = request();

        assert_eq!(
            request.allowed_capabilities,
            vec!["document.read", "document.create"]
        );
        assert_eq!(request.goal, "Create the requested report");
    }

    #[test]
    fn generic_agent_adapter_requires_no_openclaw_specific_request_shape() {
        let adapter = RecordingAgent {
            requests: Mutex::new(Vec::new()),
        };
        let mut progress = Vec::new();

        let result = adapter
            .execute(&request(), &mut |event| progress.push(event))
            .unwrap();

        assert_eq!(result.session_id.as_deref(), Some("session-1"));
        assert_eq!(result.output, serde_json::json!({"ok": true}));
        assert_eq!(adapter.requests.lock().unwrap().len(), 1);
        assert_eq!(progress.len(), 1);
    }

    #[test]
    fn invalid_agent_and_execution_identity_fail_closed() {
        assert_eq!(
            AgentId::new("   ").unwrap_err(),
            AgentExecutionContractError::InvalidAgentId
        );

        assert_eq!(
            AgentExecutionRequest::new(
                " ",
                "task",
                "plan",
                "step",
                AgentId::new("openclaw").unwrap(),
                "goal",
                vec![],
                Value::Null,
                SkillInvocationContext::new(
                    "task",
                    "plan",
                    "execution",
                    "openclaw",
                    Vec::new(),
                    false,
                )
                .unwrap(),
            )
            .unwrap_err(),
            AgentExecutionContractError::InvalidExecutionId
        );
    }

    #[test]
    fn capabilities_are_trimmed_and_deduplicated() {
        let request = AgentExecutionRequest::new(
            "execution",
            "task",
            "plan",
            "step",
            AgentId::new("openclaw").unwrap(),
            "goal",
            vec![
                " browser.search ".to_owned(),
                "browser.search".to_owned(),
                "media.text-to-image".to_owned(),
            ],
            Value::Null,
            SkillInvocationContext::new(
                "task",
                "plan",
                "execution",
                "openclaw",
                vec![
                    "browser.search".to_owned(),
                    "media.text-to-image".to_owned(),
                ],
                false,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            request.allowed_capabilities,
            vec!["browser.search", "media.text-to-image"]
        );
    }

    #[test]
    fn mismatched_runtime_skill_context_fails_closed() {
        let error = AgentExecutionRequest::new(
            "execution",
            "task",
            "plan",
            "step",
            AgentId::new("openclaw").unwrap(),
            "goal",
            vec!["filesystem.scan".to_owned()],
            Value::Null,
            SkillInvocationContext::new(
                "different-task",
                "plan",
                "execution",
                "openclaw",
                vec!["filesystem.scan".to_owned()],
                true,
            )
            .unwrap(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            AgentExecutionContractError::InvalidSkillInvocationContext
        );
    }

    #[test]
    fn unknown_newer_agent_version_is_accepted_when_capabilities_match() {
        let probe = AgentProbeResult {
            agent_id: AgentId::new("future-agent").unwrap(),
            version: Some("9999.42.0".to_owned()),
            capabilities: AgentCapabilities::new([
                AgentCapability::TaskExecution,
                AgentCapability::NativeSkillTransport,
            ]),
        };

        assert_eq!(
            negotiate_agent_compatibility(
                &probe,
                &[AgentCapability::TaskExecution],
                &[AgentCapability::NativeSkillTransport],
            ),
            AgentCompatibility::Compatible
        );
    }

    #[test]
    fn version_change_does_not_change_transport_when_capabilities_are_same() {
        let capabilities = AgentCapabilities::new([
            AgentCapability::TaskExecution,
            AgentCapability::McpSkillTransport,
        ]);

        let old = AgentProbeResult {
            agent_id: AgentId::new("agent").unwrap(),
            version: Some("1.0.0".to_owned()),
            capabilities: capabilities.clone(),
        };
        let new = AgentProbeResult {
            agent_id: AgentId::new("agent").unwrap(),
            version: Some("500.0.0".to_owned()),
            capabilities,
        };

        assert_eq!(
            select_skill_transport(&old.capabilities),
            Some(AgentSkillTransport::Mcp)
        );
        assert_eq!(
            select_skill_transport(&new.capabilities),
            Some(AgentSkillTransport::Mcp)
        );
    }

    #[test]
    fn missing_required_capability_fails_negotiation_cleanly() {
        let probe = AgentProbeResult {
            agent_id: AgentId::new("limited-agent").unwrap(),
            version: Some("1.0".to_owned()),
            capabilities: AgentCapabilities::new([AgentCapability::ProgressEvents]),
        };

        assert_eq!(
            negotiate_agent_compatibility(&probe, &[AgentCapability::TaskExecution], &[],),
            AgentCompatibility::Incompatible {
                missing_required: vec![AgentCapability::TaskExecution],
            }
        );
    }

    #[test]
    fn version_alone_cannot_enable_skill_transport() {
        let probe = AgentProbeResult {
            agent_id: AgentId::new("agent").unwrap(),
            version: Some("2026.8.2".to_owned()),
            capabilities: AgentCapabilities::new([AgentCapability::TaskExecution]),
        };

        assert_eq!(select_skill_transport(&probe.capabilities), None);
    }
}
