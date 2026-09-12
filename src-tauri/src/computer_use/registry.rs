use super::{
    models::{
        ComputerUseCancellationToken, ComputerUseCapability, ComputerUseError,
        ComputerUseErrorKind, ComputerUseProgress, ComputerUseReadiness, ComputerUseRequest,
        ComputerUseResult,
    },
    provider::ComputerUseProvider,
};
use std::{collections::BTreeMap, sync::Arc};

const REQUIRED_CAPABILITIES: [ComputerUseCapability; 7] = [
    ComputerUseCapability::LocalInference,
    ComputerUseCapability::BoundedSteps,
    ComputerUseCapability::Cancellation,
    ComputerUseCapability::ProgressEvents,
    ComputerUseCapability::ScreenCapture,
    ComputerUseCapability::InputControl,
    ComputerUseCapability::ShellDisabled,
];

#[derive(Default)]
pub(crate) struct ComputerUseProviderRegistry {
    providers: BTreeMap<String, Arc<dyn ComputerUseProvider>>,
    selected_provider: Option<String>,
}

impl ComputerUseProviderRegistry {
    pub(crate) fn production() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn with_selected_provider(
        identity: impl Into<String>,
        provider: Arc<dyn ComputerUseProvider>,
    ) -> Self {
        let identity = identity.into();
        Self {
            providers: [(identity.clone(), provider)].into_iter().collect(),
            selected_provider: Some(identity),
        }
    }

    pub(crate) fn execute(
        &self,
        request: &ComputerUseRequest,
        cancellation: &ComputerUseCancellationToken,
        report: &mut dyn FnMut(ComputerUseProgress),
    ) -> Result<ComputerUseResult, ComputerUseError> {
        let selected = self.selected_provider.as_deref().ok_or_else(|| {
            ComputerUseError::new(
                ComputerUseErrorKind::ProviderUnavailable,
                "No Computer Use provider is registered by Runtime policy.",
                false,
            )
        })?;
        let provider = self.providers.get(selected).ok_or_else(|| {
            ComputerUseError::new(
                ComputerUseErrorKind::ProviderUnavailable,
                "The selected Computer Use provider is unavailable.",
                false,
            )
        })?;
        let probe = provider.probe()?;
        if probe.identity != selected || probe.readiness != ComputerUseReadiness::Ready {
            return Err(ComputerUseError::new(
                ComputerUseErrorKind::ProviderUnavailable,
                "The selected Computer Use provider is not ready.",
                false,
            ));
        }
        let missing = probe.capabilities.missing(REQUIRED_CAPABILITIES);
        if !missing.is_empty() {
            return Err(ComputerUseError::new(
                ComputerUseErrorKind::CapabilityUnavailable,
                format!("Computer Use provider lacks required capabilities: {missing:?}."),
                false,
            ));
        }

        let result = provider.execute(request, cancellation, report)?;
        if result.provider_identity != selected {
            return Err(ComputerUseError::new(
                ComputerUseErrorKind::ExecutionFailed,
                "Computer Use provider returned a mismatched identity.",
                false,
            ));
        }
        Ok(result)
    }
}
