use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) const MAX_COMPUTER_USE_STEPS: u16 = 100;
pub(crate) const MAX_COMPUTER_USE_DURATION_MS: u64 = 15 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComputerUseInvocationInput {
    pub(crate) task: String,
    pub(crate) goal: String,
    pub(crate) allowed_applications: Vec<String>,
    pub(crate) max_steps: u16,
    pub(crate) max_duration_ms: u64,
    pub(crate) display_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseExecutionIdentity {
    pub(crate) task_id: String,
    pub(crate) plan_id: String,
    pub(crate) agent_execution_id: String,
    pub(crate) invocation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseRequest {
    pub(crate) execution: ComputerUseExecutionIdentity,
    pub(crate) task: String,
    pub(crate) goal: String,
    pub(crate) allowed_applications: Vec<String>,
    pub(crate) max_steps: u16,
    pub(crate) max_duration_ms: u64,
    pub(crate) display_scope: Option<String>,
}

impl ComputerUseRequest {
    pub(crate) fn validated(
        execution: ComputerUseExecutionIdentity,
        input: ComputerUseInvocationInput,
    ) -> Result<Self, ComputerUseError> {
        let task = bounded_text(input.task, "task", 256)?;
        let goal = bounded_text(input.goal, "goal", 2_048)?;
        if input.allowed_applications.is_empty() || input.allowed_applications.len() > 16 {
            return Err(ComputerUseError::invalid(
                "Computer Use requires between 1 and 16 allowed applications.",
            ));
        }
        let mut allowed_applications = Vec::with_capacity(input.allowed_applications.len());
        for application in input.allowed_applications {
            let application = bounded_text(application, "allowed application", 128)?;
            if !allowed_applications.contains(&application) {
                allowed_applications.push(application);
            }
        }
        if input.max_steps == 0 || input.max_steps > MAX_COMPUTER_USE_STEPS {
            return Err(ComputerUseError::invalid(
                "Computer Use max steps must be between 1 and 100.",
            ));
        }
        if input.max_duration_ms == 0 || input.max_duration_ms > MAX_COMPUTER_USE_DURATION_MS {
            return Err(ComputerUseError::invalid(
                "Computer Use max duration must be between 1 ms and 15 minutes.",
            ));
        }
        let display_scope = input
            .display_scope
            .map(|value| bounded_text(value, "display scope", 64))
            .transpose()?;

        Ok(Self {
            execution,
            task,
            goal,
            allowed_applications,
            max_steps: input.max_steps,
            max_duration_ms: input.max_duration_ms,
            display_scope,
        })
    }
}

fn bounded_text(value: String, field: &str, max_chars: usize) -> Result<String, ComputerUseError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(ComputerUseError::invalid(format!(
            "Computer Use {field} is empty or exceeds its bound."
        )));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseCompletionStatus {
    Completed,
    Stopped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseStopReason {
    Completed,
    ProviderStopped,
    Cancelled,
    StepLimit,
    DurationLimit,
    Infeasible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseResultMetadata {
    pub(crate) duration_ms: u64,
    pub(crate) final_phase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseResult {
    pub(crate) status: ComputerUseCompletionStatus,
    pub(crate) summary: String,
    pub(crate) steps_used: u16,
    pub(crate) stop_reason: ComputerUseStopReason,
    pub(crate) provider_identity: String,
    pub(crate) metadata: ComputerUseResultMetadata,
}

impl ComputerUseResult {
    pub(crate) fn validated(
        status: ComputerUseCompletionStatus,
        summary: impl Into<String>,
        steps_used: u16,
        stop_reason: ComputerUseStopReason,
        provider_identity: impl Into<String>,
        duration_ms: u64,
        final_phase: impl Into<String>,
    ) -> Result<Self, ComputerUseError> {
        Ok(Self {
            status,
            summary: bounded_text(summary.into(), "result summary", 1_024)?,
            steps_used,
            stop_reason,
            provider_identity: bounded_text(provider_identity.into(), "provider identity", 128)?,
            metadata: ComputerUseResultMetadata {
                duration_ms,
                final_phase: bounded_text(final_phase.into(), "final phase", 64)?,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseProgressPhase {
    Preparing,
    Observing,
    Acting,
    Verifying,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseProgress {
    pub(crate) phase: ComputerUseProgressPhase,
    pub(crate) step: u16,
    pub(crate) message: String,
}

impl ComputerUseProgress {
    pub(crate) fn new(
        phase: ComputerUseProgressPhase,
        step: u16,
        message: impl Into<String>,
    ) -> Result<Self, ComputerUseError> {
        Ok(Self {
            phase,
            step,
            message: bounded_text(message.into(), "progress message", 256)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseCapability {
    LocalInference,
    BoundedSteps,
    Cancellation,
    ProgressEvents,
    ScreenCapture,
    InputControl,
    ShellDisabled,
    AppScope,
    DisplayScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub(crate) struct ComputerUseCapabilitySet(BTreeSet<ComputerUseCapability>);

impl ComputerUseCapabilitySet {
    pub(crate) fn new(capabilities: impl IntoIterator<Item = ComputerUseCapability>) -> Self {
        Self(capabilities.into_iter().collect())
    }

    pub(crate) fn supports(&self, capability: ComputerUseCapability) -> bool {
        self.0.contains(&capability)
    }

    pub(crate) fn missing(
        &self,
        required: impl IntoIterator<Item = ComputerUseCapability>,
    ) -> Vec<ComputerUseCapability> {
        required
            .into_iter()
            .filter(|capability| !self.supports(*capability))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseReadiness {
    Ready,
    NotReady,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseProviderProbe {
    pub(crate) identity: String,
    pub(crate) diagnostic_version: Option<String>,
    pub(crate) capabilities: ComputerUseCapabilitySet,
    pub(crate) readiness: ComputerUseReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerUseErrorKind {
    InvalidRequest,
    ProviderUnavailable,
    CapabilityUnavailable,
    PermissionRequired,
    PermissionDenied,
    Cancelled,
    Timeout,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseError {
    pub(crate) kind: ComputerUseErrorKind,
    pub(crate) message: String,
    pub(crate) retryable: bool,
}

impl ComputerUseError {
    pub(crate) fn new(
        kind: ComputerUseErrorKind,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        let message = message.into();
        let message = message.trim();
        Self {
            kind,
            message: message.chars().take(512).collect(),
            retryable,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ComputerUseErrorKind::InvalidRequest, message, false)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ComputerUseCancellationToken(Arc<AtomicBool>);

impl ComputerUseCancellationToken {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
