use super::models::{
    ComputerUseCancellationToken, ComputerUseError, ComputerUseProgress, ComputerUseProviderProbe,
    ComputerUseRequest, ComputerUseResult,
};

pub(crate) trait ComputerUseProvider: Send + Sync {
    fn probe(&self) -> Result<ComputerUseProviderProbe, ComputerUseError>;

    fn execute(
        &self,
        request: &ComputerUseRequest,
        cancellation: &ComputerUseCancellationToken,
        report: &mut dyn FnMut(ComputerUseProgress),
    ) -> Result<ComputerUseResult, ComputerUseError>;
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use crate::computer_use::models::{
        ComputerUseCapability, ComputerUseCapabilitySet, ComputerUseCompletionStatus,
        ComputerUseErrorKind, ComputerUseProgressPhase, ComputerUseReadiness,
        ComputerUseStopReason,
    };
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    pub(crate) enum MockComputerUseOutcome {
        Success,
        Error(ComputerUseError),
        ObserveCancellation,
    }

    pub(crate) struct MockComputerUseProvider {
        pub(crate) identity: String,
        pub(crate) diagnostic_version: Option<String>,
        pub(crate) capabilities: ComputerUseCapabilitySet,
        pub(crate) readiness: ComputerUseReadiness,
        pub(crate) outcome: MockComputerUseOutcome,
        requests: Mutex<Vec<ComputerUseRequest>>,
        progress: Mutex<Vec<ComputerUseProgress>>,
    }

    impl MockComputerUseProvider {
        pub(crate) fn ready(version: &str) -> Self {
            Self {
                identity: "mock-computer-use".to_owned(),
                diagnostic_version: Some(version.to_owned()),
                capabilities: ComputerUseCapabilitySet::new([
                    ComputerUseCapability::LocalInference,
                    ComputerUseCapability::BoundedSteps,
                    ComputerUseCapability::Cancellation,
                    ComputerUseCapability::ProgressEvents,
                    ComputerUseCapability::ScreenCapture,
                    ComputerUseCapability::InputControl,
                    ComputerUseCapability::ShellDisabled,
                    ComputerUseCapability::AppScope,
                ]),
                readiness: ComputerUseReadiness::Ready,
                outcome: MockComputerUseOutcome::Success,
                requests: Mutex::new(Vec::new()),
                progress: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }

        pub(crate) fn requests(&self) -> Vec<ComputerUseRequest> {
            self.requests.lock().unwrap().clone()
        }

        pub(crate) fn progress(&self) -> Vec<ComputerUseProgress> {
            self.progress.lock().unwrap().clone()
        }
    }

    impl ComputerUseProvider for MockComputerUseProvider {
        fn probe(&self) -> Result<ComputerUseProviderProbe, ComputerUseError> {
            Ok(ComputerUseProviderProbe {
                identity: self.identity.clone(),
                diagnostic_version: self.diagnostic_version.clone(),
                capabilities: self.capabilities.clone(),
                readiness: self.readiness,
            })
        }

        fn execute(
            &self,
            request: &ComputerUseRequest,
            cancellation: &ComputerUseCancellationToken,
            report: &mut dyn FnMut(ComputerUseProgress),
        ) -> Result<ComputerUseResult, ComputerUseError> {
            self.requests.lock().unwrap().push(request.clone());
            let progress = ComputerUseProgress::new(
                ComputerUseProgressPhase::Observing,
                1,
                "Mock provider observed the bounded fixture.",
            )?;
            self.progress.lock().unwrap().push(progress.clone());
            report(progress);

            match &self.outcome {
                MockComputerUseOutcome::Success => ComputerUseResult::validated(
                    ComputerUseCompletionStatus::Completed,
                    "Mock Computer Use task completed.",
                    1,
                    ComputerUseStopReason::Completed,
                    &self.identity,
                    1,
                    "completed",
                ),
                MockComputerUseOutcome::Error(error) => Err(error.clone()),
                MockComputerUseOutcome::ObserveCancellation if cancellation.is_cancelled() => {
                    Err(ComputerUseError::new(
                        ComputerUseErrorKind::Cancelled,
                        "Computer Use operation was cancelled.",
                        false,
                    ))
                }
                MockComputerUseOutcome::ObserveCancellation => Err(ComputerUseError::new(
                    ComputerUseErrorKind::ExecutionFailed,
                    "Cancellation was not requested.",
                    false,
                )),
            }
        }
    }
}
