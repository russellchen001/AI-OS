use super::{
    domain::{MediaCapability, MediaProviderSelection},
    provider::{MediaProvider, MediaProviderMetadata},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MediaRegistryError {
    DuplicateProviderIdentity {
        provider_id: String,
        provider_instance_id: Option<String>,
    },
}

/// Replaceable provider registry.
///
/// The production registry is intentionally empty in GM-1B2. A provider is not
/// considered part of AI-OS merely because a future adapter name is known.
#[derive(Default)]
pub(crate) struct MediaProviderRegistry {
    providers: Vec<Box<dyn MediaProvider>>,
}

impl MediaProviderRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register<P>(&mut self, provider: P) -> Result<(), MediaRegistryError>
    where
        P: MediaProvider + 'static,
    {
        let incoming = provider.metadata();

        let duplicate = self.providers.iter().any(|existing| {
            let existing = existing.metadata();

            existing.provider_id == incoming.provider_id
                && existing.provider_instance_id == incoming.provider_instance_id
        });

        if duplicate {
            return Err(MediaRegistryError::DuplicateProviderIdentity {
                provider_id: incoming.provider_id.clone(),
                provider_instance_id: incoming.provider_instance_id.clone(),
            });
        }

        self.providers.push(Box::new(provider));

        Ok(())
    }

    pub(crate) fn providers(&self) -> Vec<&dyn MediaProvider> {
        self.providers
            .iter()
            .map(|provider| provider.as_ref())
            .collect()
    }

    pub(crate) fn metadata(&self) -> Vec<MediaProviderMetadata> {
        self.providers
            .iter()
            .map(|provider| provider.metadata().clone())
            .collect()
    }

    pub(crate) fn supporting(
        &self,
        capability: MediaCapability,
    ) -> Vec<&dyn MediaProvider> {
        self.providers()
            .into_iter()
            .filter(|provider| provider.metadata().supports(capability))
            .collect()
    }

    pub(crate) fn matching_selection(
        &self,
        selection: &MediaProviderSelection,
    ) -> Vec<&dyn MediaProvider> {
        self.providers()
            .into_iter()
            .filter(|provider| {
                let metadata = provider.metadata();

                if metadata.provider_id != selection.provider_id {
                    return false;
                }

                match selection.provider_instance_id.as_ref() {
                    Some(expected_instance) => {
                        metadata.provider_instance_id.as_ref() == Some(expected_instance)
                    }
                    None => true,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                LawfulContentCompatibility, MediaError, MediaProgress, MediaProviderSource,
                MediaRequest, MediaResult,
            },
            provider::LocalMediaReadiness,
        },
        provider_selection::{
            AuthorizationKind, AuthorizationState, ProviderInterfaceKind,
        },
    };

    struct StubProvider {
        metadata: MediaProviderMetadata,
    }

    impl MediaProvider for StubProvider {
        fn metadata(&self) -> &MediaProviderMetadata {
            &self.metadata
        }

        fn execute(
            &self,
            _request: &MediaRequest,
            _report: &mut dyn FnMut(MediaProgress),
        ) -> Result<MediaResult, MediaError> {
            unreachable!("registry tests do not execute a provider")
        }
    }

    fn stub(id: &str, instance: Option<&str>) -> StubProvider {
        StubProvider {
            metadata: MediaProviderMetadata {
                provider_id: id.to_owned(),
                provider_instance_id: instance.map(str::to_owned),
                source: MediaProviderSource::Local,
                interface_kind: ProviderInterfaceKind::NativeStructured,
                authorization_kind: AuthorizationKind::None,
                authorization_state: AuthorizationState::Connected,
                authorization_ref: None,
                available: true,
                local_readiness: Some(LocalMediaReadiness::Ready),
                priority: 10,
                capabilities: vec![MediaCapability::TextToImage],
                lawful_content_compatibility: LawfulContentCompatibility::Unknown,
                recommended_cloud: false,
                supports_progress: true,
                supports_cancellation: true,
                supports_cost_estimation: false,
            },
        }
    }

    #[test]
    fn production_registry_starts_empty() {
        let registry = MediaProviderRegistry::new();

        assert!(registry.providers().is_empty());
        assert!(registry.metadata().is_empty());
    }

    #[test]
    fn duplicate_exact_provider_identity_is_rejected() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(stub("provider-a", Some("primary")))
            .unwrap();

        assert_eq!(
            registry.register(stub("provider-a", Some("primary"))),
            Err(MediaRegistryError::DuplicateProviderIdentity {
                provider_id: "provider-a".to_owned(),
                provider_instance_id: Some("primary".to_owned()),
            })
        );
    }

    #[test]
    fn same_provider_id_can_have_distinct_instances() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(stub("provider-a", Some("primary")))
            .unwrap();

        registry
            .register(stub("provider-a", Some("secondary")))
            .unwrap();

        assert_eq!(registry.providers().len(), 2);
    }

    #[test]
    fn capability_filter_is_typed() {
        let mut registry = MediaProviderRegistry::new();

        registry.register(stub("provider-a", None)).unwrap();

        assert_eq!(
            registry.supporting(MediaCapability::TextToImage).len(),
            1
        );

        assert!(
            registry
                .supporting(MediaCapability::TextToVideo)
                .is_empty()
        );
    }

    #[test]
    fn explicit_instance_selection_matches_exactly() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(stub("provider-a", Some("primary")))
            .unwrap();

        registry
            .register(stub("provider-a", Some("secondary")))
            .unwrap();

        let matches = registry.matching_selection(&MediaProviderSelection {
            provider_id: "provider-a".to_owned(),
            provider_instance_id: Some("secondary".to_owned()),
        });

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].metadata().provider_instance_id.as_deref(),
            Some("secondary")
        );
    }
}
