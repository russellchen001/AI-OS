use super::{
    models::{NormalizedRuntimeError, RuntimeErrorCode},
    scheduler::RuntimeScheduler,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureClassification {
    Recoverable,
    NonRecoverable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryRoute {
    Scheduler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecoveryDecision {
    pub(crate) classification: FailureClassification,
    pub(crate) recovery_possible: bool,
    pub(crate) route: Option<RecoveryRoute>,
}

#[derive(Clone)]
pub(crate) struct RecoveryCoordinator {
    scheduler: RuntimeScheduler,
}

impl RecoveryCoordinator {
    pub(crate) fn new(scheduler: RuntimeScheduler) -> Self {
        Self { scheduler }
    }

    pub(crate) fn evaluate(&self, error: &NormalizedRuntimeError) -> RecoveryDecision {
        let classification = match error.code {
            RuntimeErrorCode::ConnectionUnavailable
            | RuntimeErrorCode::ProbeFailed
            | RuntimeErrorCode::OperationFailed
            | RuntimeErrorCode::OperationTaskFailed
            | RuntimeErrorCode::DependencyUnavailable
            | RuntimeErrorCode::ReadinessTimeout
                if error.retryable =>
            {
                FailureClassification::Recoverable
            }
            RuntimeErrorCode::AuthenticationRequired
            | RuntimeErrorCode::PairingRequired
            | RuntimeErrorCode::ConfigurationUnavailable
            | RuntimeErrorCode::InvalidConfiguration
            | RuntimeErrorCode::UnsupportedPlatform
            | RuntimeErrorCode::RuntimeNotFound
            | RuntimeErrorCode::OperationNotFound
            | RuntimeErrorCode::UnsupportedOperation
            | RuntimeErrorCode::CancellationUnsupported
            | RuntimeErrorCode::CancellationTooLate
            | RuntimeErrorCode::DependencyNotInstalled
            | RuntimeErrorCode::InvalidRuntimeLocation
            | RuntimeErrorCode::ContainerNotFound
            | RuntimeErrorCode::ContainerAmbiguous => FailureClassification::NonRecoverable,
            _ => FailureClassification::Unknown,
        };
        let recovery_possible =
            classification == FailureClassification::Recoverable && self.scheduler.is_available();
        RecoveryDecision {
            classification,
            recovery_possible,
            route: recovery_possible.then_some(RecoveryRoute::Scheduler),
        }
    }

    #[cfg(test)]
    pub(crate) fn shares_scheduler_with(&self, scheduler: &RuntimeScheduler) -> bool {
        self.scheduler.shares_state_with(scheduler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(code: RuntimeErrorCode, retryable: bool) -> NormalizedRuntimeError {
        NormalizedRuntimeError {
            code,
            message: "Safe runtime failure.".to_string(),
            retryable,
        }
    }

    #[test]
    fn retryable_transient_failure_is_recoverable() {
        let scheduler = RuntimeScheduler::default();
        let coordinator = RecoveryCoordinator::new(scheduler.clone());
        let decision = coordinator.evaluate(&error(RuntimeErrorCode::ReadinessTimeout, true));
        assert_eq!(decision.classification, FailureClassification::Recoverable);
        assert!(decision.recovery_possible);
        assert_eq!(decision.route, Some(RecoveryRoute::Scheduler));
        assert!(scheduler.shares_state_with(&coordinator.scheduler));
    }

    #[test]
    fn static_failure_is_non_recoverable_even_if_marked_retryable() {
        let coordinator = RecoveryCoordinator::new(RuntimeScheduler::default());
        let decision = coordinator.evaluate(&error(RuntimeErrorCode::InvalidConfiguration, true));
        assert_eq!(
            decision.classification,
            FailureClassification::NonRecoverable
        );
        assert!(!decision.recovery_possible);
        assert_eq!(decision.route, None);
    }

    #[test]
    fn ambiguous_failure_is_unknown_and_not_eligible() {
        let coordinator = RecoveryCoordinator::new(RuntimeScheduler::default());
        let decision = coordinator.evaluate(&error(RuntimeErrorCode::OperationConflict, true));
        assert_eq!(decision.classification, FailureClassification::Unknown);
        assert!(!decision.recovery_possible);
        assert_eq!(decision.route, None);
    }
}
