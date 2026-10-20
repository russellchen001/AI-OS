pub(crate) mod models;
pub(crate) mod peekaboo;
pub(crate) mod provider;
pub(crate) mod registry;

use crate::runtime::skill_invocation::{
    SkillBackend, SkillInvocationContext, SkillInvocationError, SkillInvocationErrorKind,
    SkillInvocationRequest, SkillInvocationResult,
};
use models::{
    ComputerUseCancellationToken, ComputerUseError, ComputerUseErrorKind,
    ComputerUseExecutionIdentity, ComputerUseInvocationInput, ComputerUseProgress,
    ComputerUseRequest, ComputerUseResult,
};
use registry::ComputerUseProviderRegistry;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
};

pub(crate) const EXECUTE_CAPABILITY: &str = "computer.use.execute";

#[derive(Default)]
struct ActiveTaskState {
    active: HashMap<String, ComputerUseCancellationToken>,
    pending_cancel: HashSet<String>,
}

static ACTIVE_TASKS: OnceLock<Mutex<ActiveTaskState>> = OnceLock::new();

fn active_tasks() -> &'static Mutex<ActiveTaskState> {
    ACTIVE_TASKS.get_or_init(|| Mutex::new(ActiveTaskState::default()))
}

struct ActiveTaskGuard(String);

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        if let Ok(mut tasks) = active_tasks().lock() {
            tasks.active.remove(&self.0);
            tasks.pending_cancel.remove(&self.0);
        }
    }
}

fn register_active(
    task_id: &str,
    token: &ComputerUseCancellationToken,
) -> Result<ActiveTaskGuard, SkillInvocationError> {
    let mut tasks = active_tasks().lock().map_err(|_| {
        SkillInvocationError::new(
            SkillInvocationErrorKind::ExecutionFailed,
            "Computer Use cancellation state is unavailable",
            false,
        )
    })?;
    if tasks.active.contains_key(task_id) {
        return Err(SkillInvocationError::new(
            SkillInvocationErrorKind::ExecutionFailed,
            "Computer Use is already active for this task",
            false,
        ));
    }
    if tasks.pending_cancel.remove(task_id) {
        token.cancel();
    }
    tasks.active.insert(task_id.to_owned(), token.clone());
    Ok(ActiveTaskGuard(task_id.to_owned()))
}

pub(crate) fn cancel_active_task(task_id: &str) -> bool {
    let Ok(mut tasks) = active_tasks().lock() else {
        return false;
    };
    if let Some(token) = tasks.active.get(task_id) {
        token.cancel();
    } else if tasks.pending_cancel.len() < 128 {
        tasks.pending_cancel.insert(task_id.to_owned());
    } else {
        return false;
    }
    true
}

pub(crate) struct ComputerUseSkillBackend {
    registry: Arc<ComputerUseProviderRegistry>,
}

impl ComputerUseSkillBackend {
    pub(crate) fn production() -> Self {
        Self {
            registry: Arc::new(ComputerUseProviderRegistry::production()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_registry(registry: ComputerUseProviderRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    fn execute(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
        cancellation: &ComputerUseCancellationToken,
        report: &mut dyn FnMut(ComputerUseProgress),
    ) -> Result<ComputerUseResult, ComputerUseError> {
        if request.capability != EXECUTE_CAPABILITY {
            return Err(ComputerUseError::invalid(
                "Computer Use backend received an unsupported capability.",
            ));
        }
        let input: ComputerUseInvocationInput = serde_json::from_value(request.input.clone())
            .map_err(|_| {
                ComputerUseError::invalid(
                    "Computer Use input does not match the provider-neutral contract.",
                )
            })?;
        let normalized = ComputerUseRequest::validated(
            ComputerUseExecutionIdentity {
                task_id: context.task_id.clone(),
                plan_id: context.plan_id.clone(),
                agent_execution_id: context.agent_execution_id.clone(),
                invocation_id: request.invocation_id.clone(),
            },
            input,
        )?;

        self.registry.execute(&normalized, cancellation, report)
    }
}

impl SkillBackend for ComputerUseSkillBackend {
    fn invoke(
        &self,
        context: &SkillInvocationContext,
        request: &SkillInvocationRequest,
    ) -> Result<SkillInvocationResult, SkillInvocationError> {
        let cancellation = ComputerUseCancellationToken::default();
        let _guard = register_active(&context.task_id, &cancellation)?;
        let result = self
            .execute(context, request, &cancellation, &mut |_| {})
            .map_err(map_skill_error)?;
        let output = serde_json::to_value(&result).map_err(|_| {
            SkillInvocationError::new(
                SkillInvocationErrorKind::ExecutionFailed,
                "Computer Use result could not be normalized.",
                false,
            )
        })?;

        Ok(SkillInvocationResult {
            invocation_id: request.invocation_id.clone(),
            capability: request.capability.clone(),
            backend: "computer-use".to_owned(),
            provider: Some(result.provider_identity),
            output,
        })
    }
}

fn map_skill_error(error: ComputerUseError) -> SkillInvocationError {
    let kind = match error.kind {
        ComputerUseErrorKind::InvalidRequest => SkillInvocationErrorKind::InvalidRequest,
        ComputerUseErrorKind::ProviderUnavailable | ComputerUseErrorKind::CapabilityUnavailable => {
            SkillInvocationErrorKind::BackendUnavailable
        }
        ComputerUseErrorKind::PermissionRequired => SkillInvocationErrorKind::PermissionRequired,
        ComputerUseErrorKind::PermissionDenied => SkillInvocationErrorKind::PermissionDenied,
        ComputerUseErrorKind::Cancelled
        | ComputerUseErrorKind::Timeout
        | ComputerUseErrorKind::ExecutionFailed => SkillInvocationErrorKind::ExecutionFailed,
    };
    SkillInvocationError::new(kind, error.message, error.retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use::{
        models::{
            ComputerUseCapability, ComputerUseCapabilitySet, ComputerUseErrorKind,
            ComputerUseReadiness,
        },
        provider::mock::{MockComputerUseOutcome, MockComputerUseProvider},
    };
    use serde_json::json;

    fn context() -> SkillInvocationContext {
        SkillInvocationContext::new(
            "runtime-task",
            "runtime-plan",
            "runtime-agent-execution",
            "openclaw",
            vec![EXECUTE_CAPABILITY.to_owned()],
            true,
        )
        .unwrap()
    }

    fn input() -> serde_json::Value {
        json!({
            "task": "Interact with the fixture application",
            "goal": "Complete the bounded visual fixture",
            "allowedApplications": ["Fixture App"],
            "maxSteps": 4,
            "maxDurationMs": 2_000,
            "displayScope": "primary"
        })
    }

    fn request(value: serde_json::Value) -> SkillInvocationRequest {
        SkillInvocationRequest::new("computer-use-1", EXECUTE_CAPABILITY, value).unwrap()
    }

    fn backend_with(provider: Arc<MockComputerUseProvider>) -> ComputerUseSkillBackend {
        ComputerUseSkillBackend::with_registry(ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider,
        ))
    }

    fn execute_registry(
        registry: &ComputerUseProviderRegistry,
        cancellation: &ComputerUseCancellationToken,
        report: &mut dyn FnMut(ComputerUseProgress),
    ) -> Result<ComputerUseResult, ComputerUseError> {
        let parsed: ComputerUseInvocationInput = serde_json::from_value(input()).unwrap();
        let normalized = ComputerUseRequest::validated(
            ComputerUseExecutionIdentity {
                task_id: "runtime-task".to_owned(),
                plan_id: "runtime-plan".to_owned(),
                agent_execution_id: "runtime-agent-execution".to_owned(),
                invocation_id: "computer-use-1".to_owned(),
            },
            parsed,
        )
        .unwrap();
        registry.execute(&normalized, cancellation, report)
    }

    #[test]
    fn mp0_agent_cannot_choose_provider() {
        let provider = Arc::new(MockComputerUseProvider::ready("test"));
        let backend = backend_with(provider.clone());
        let mut value = input();
        value["provider"] = json!("mano");

        let error = backend.invoke(&context(), &request(value)).unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::InvalidRequest);
        assert_eq!(provider.request_count(), 0);
    }

    #[test]
    fn mp0_agent_cannot_choose_cloud_or_local_mode() {
        for forbidden in [
            ("mode", json!("cloud")),
            ("mode", json!("local")),
            ("userConfirmed", json!(true)),
            ("shellEnabled", json!(true)),
            ("credentials", json!({"token": "not-admitted"})),
        ] {
            let provider = Arc::new(MockComputerUseProvider::ready("test"));
            let backend = backend_with(provider.clone());
            let mut value = input();
            value[forbidden.0] = forbidden.1;

            let error = backend.invoke(&context(), &request(value)).unwrap_err();

            assert_eq!(error.kind, SkillInvocationErrorKind::InvalidRequest);
            assert_eq!(provider.request_count(), 0);
        }
    }

    #[test]
    fn mp0_version_metadata_does_not_determine_compatibility() {
        for version in ["0.0.1", "9999.42.0"] {
            let provider = Arc::new(MockComputerUseProvider::ready(version));
            let registry = ComputerUseProviderRegistry::with_selected_provider(
                provider.identity.clone(),
                provider,
            );

            assert!(execute_registry(
                &registry,
                &ComputerUseCancellationToken::default(),
                &mut |_| {}
            )
            .is_ok());
        }
    }

    #[test]
    fn mp0_missing_required_capability_rejects_provider() {
        let mut provider = MockComputerUseProvider::ready("expected-looking-version");
        provider.capabilities = ComputerUseCapabilitySet::new([
            ComputerUseCapability::LocalInference,
            ComputerUseCapability::BoundedSteps,
            ComputerUseCapability::Cancellation,
            ComputerUseCapability::ProgressEvents,
            ComputerUseCapability::ScreenCapture,
            ComputerUseCapability::ShellDisabled,
        ]);
        let provider = Arc::new(provider);
        let registry = ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider.clone(),
        );

        let error = execute_registry(
            &registry,
            &ComputerUseCancellationToken::default(),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, ComputerUseErrorKind::CapabilityUnavailable);
        assert_eq!(provider.request_count(), 0);
    }

    #[test]
    fn mp0_progress_contract_is_bounded_and_emitted() {
        let provider = Arc::new(MockComputerUseProvider::ready("test"));
        let registry = ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider.clone(),
        );
        let mut observed = Vec::new();

        execute_registry(
            &registry,
            &ComputerUseCancellationToken::default(),
            &mut |progress| observed.push(progress),
        )
        .unwrap();

        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].step, 1);
        assert!(observed[0].message.len() <= 256);
        assert_eq!(provider.progress(), observed);
    }

    #[test]
    fn mp0_mock_provider_simulates_cancellation() {
        let mut provider = MockComputerUseProvider::ready("test");
        provider.outcome = MockComputerUseOutcome::ObserveCancellation;
        let provider = Arc::new(provider);
        let registry = ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider,
        );
        let cancellation = ComputerUseCancellationToken::default();
        cancellation.cancel();

        let error = execute_registry(&registry, &cancellation, &mut |_| {}).unwrap_err();

        assert_eq!(error.kind, ComputerUseErrorKind::Cancelled);
    }

    #[test]
    fn active_task_cancellation_is_scoped_and_cleans_up() {
        let token = ComputerUseCancellationToken::default();
        let guard = register_active("computer-use-cancel-test", &token).unwrap();
        let other = ComputerUseCancellationToken::default();
        let other_guard = register_active("another-task", &other).unwrap();
        assert!(cancel_active_task("computer-use-cancel-test"));
        assert!(token.is_cancelled());
        assert!(!other.is_cancelled());
        drop(guard);
        drop(other_guard);
        assert!(!active_tasks()
            .lock()
            .unwrap()
            .active
            .contains_key("computer-use-cancel-test"));
    }

    #[test]
    fn cancellation_before_provider_registration_is_honored() {
        assert!(cancel_active_task("computer-use-pending-cancel-test"));
        let token = ComputerUseCancellationToken::default();
        let guard = register_active("computer-use-pending-cancel-test", &token).unwrap();
        assert!(token.is_cancelled());
        drop(guard);
    }

    #[test]
    fn mp0_cancellation_precedes_provider_readiness() {
        let cancellation = ComputerUseCancellationToken::default();
        cancellation.cancel();
        let error = execute_registry(
            &ComputerUseProviderRegistry::default(),
            &cancellation,
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(error.kind, ComputerUseErrorKind::Cancelled);
    }

    #[test]
    fn mp0_provider_error_is_normalized() {
        let mut provider = MockComputerUseProvider::ready("test");
        provider.outcome = MockComputerUseOutcome::Error(ComputerUseError::new(
            ComputerUseErrorKind::Timeout,
            "Normalized timeout.",
            false,
        ));
        let provider = Arc::new(provider);
        let registry = ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider,
        );

        let error = execute_registry(
            &registry,
            &ComputerUseCancellationToken::default(),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, ComputerUseErrorKind::Timeout);
        assert_eq!(error.message, "Normalized timeout.");
    }

    #[test]
    fn mp0_unregistered_provider_is_normalized_failure() {
        let error = execute_registry(
            &ComputerUseProviderRegistry::default(),
            &ComputerUseCancellationToken::default(),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, ComputerUseErrorKind::ProviderUnavailable);
    }

    #[test]
    fn mp0_runtime_context_owns_execution_identity() {
        let provider = Arc::new(MockComputerUseProvider::ready("test"));
        let backend = backend_with(provider.clone());

        backend.invoke(&context(), &request(input())).unwrap();

        let requests = provider.requests();
        assert_eq!(requests[0].execution.task_id, "runtime-task");
        assert_eq!(requests[0].execution.plan_id, "runtime-plan");
        assert_eq!(
            requests[0].execution.agent_execution_id,
            "runtime-agent-execution"
        );
        assert_eq!(requests[0].execution.invocation_id, "computer-use-1");
    }

    #[test]
    fn mp0_request_bounds_stop_provider_execution() {
        let provider = Arc::new(MockComputerUseProvider::ready("test"));
        let backend = backend_with(provider.clone());
        let mut value = input();
        value["maxSteps"] = json!(101);

        let error = backend.invoke(&context(), &request(value)).unwrap_err();

        assert_eq!(error.kind, SkillInvocationErrorKind::InvalidRequest);
        assert_eq!(provider.request_count(), 0);
    }

    #[test]
    fn mp0_result_contract_excludes_sensitive_payloads() {
        let result = ComputerUseResult::validated(
            models::ComputerUseCompletionStatus::Completed,
            "Fixture completed.",
            1,
            models::ComputerUseStopReason::Completed,
            "mock-computer-use",
            2,
            "completed",
        )
        .unwrap();
        let value = serde_json::to_value(result).unwrap();

        for forbidden in [
            "screenshot",
            "password",
            "secret",
            "typedText",
            "transcript",
        ] {
            assert!(value.get(forbidden).is_none());
        }
    }

    #[test]
    fn mp0_not_ready_provider_never_executes() {
        let mut provider = MockComputerUseProvider::ready("test");
        provider.readiness = ComputerUseReadiness::NotReady;
        let provider = Arc::new(provider);
        let registry = ComputerUseProviderRegistry::with_selected_provider(
            provider.identity.clone(),
            provider.clone(),
        );

        let error = execute_registry(
            &registry,
            &ComputerUseCancellationToken::default(),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, ComputerUseErrorKind::ProviderUnavailable);
        assert_eq!(provider.request_count(), 0);
    }

    #[test]
    #[ignore = "Requires an unlocked Mac, Peekaboo.app, oMLX, and the temporary TextEdit fixture"]
    fn live_production_skill_invocation_reaches_peekaboo() {
        let backend = ComputerUseSkillBackend::production();
        let marker = format!("AIOS_RUNTIME_PASS_{}", std::process::id());
        let invocation = request(json!({
            "task": format!("Type {marker} into the open TextEdit document"),
            "goal": format!("The document contains {marker}"),
            "allowedApplications": ["com.apple.TextEdit"],
            "maxSteps": 4,
            "maxDurationMs": 120000
        }));
        let result = backend.invoke(&context(), &invocation).unwrap();
        assert_eq!(result.provider.as_deref(), Some("peekaboo-local"));
        assert_eq!(result.output["status"], "completed");
    }
}
