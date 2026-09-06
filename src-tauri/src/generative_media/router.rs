use super::{
    domain::{
        MediaCapability, MediaExecutionTarget, MediaProviderSource, MediaRequest, MediaRouteMode,
    },
    provider::{LocalMediaReadiness, MediaProvider},
    registry::MediaProviderRegistry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaRoute {
    pub provider_id: String,
    pub provider_instance_id: Option<String>,
    pub source: MediaProviderSource,
    pub capability: MediaCapability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MediaRouteError {
    ManualProviderRequired,
    ProviderNotFound {
        provider_id: String,
        provider_instance_id: Option<String>,
    },
    UnsupportedCapability {
        provider_id: String,
        capability: MediaCapability,
    },
    ProviderUnavailable {
        provider_id: String,
        provider_instance_id: Option<String>,
    },
    AuthorizationRequired {
        provider_id: String,
        provider_instance_id: Option<String>,
    },
    LocalSetupRequired {
        provider_id: String,
        readiness: LocalMediaReadiness,
    },
    NoCompatibleProvider,
}

pub(crate) struct MediaRouter<'a> {
    registry: &'a MediaProviderRegistry,
}

impl<'a> MediaRouter<'a> {
    pub(crate) fn new(registry: &'a MediaProviderRegistry) -> Self {
        Self { registry }
    }

    pub(crate) fn resolve(
        &self,
        request: &MediaRequest,
    ) -> Result<MediaRoute, MediaRouteError> {
        match request.route_mode {
            MediaRouteMode::Manual => self.resolve_manual(request),
            MediaRouteMode::Auto => match request.execution_target {
                MediaExecutionTarget::LocalFirst => {
                    self.resolve_local_first(request.capability)
                }
                MediaExecutionTarget::Local => {
                    self.resolve_local(request.capability)
                }
                MediaExecutionTarget::Cloud => {
                    self.resolve_cloud(request.capability)
                }
            },
        }
    }

    fn resolve_manual(
        &self,
        request: &MediaRequest,
    ) -> Result<MediaRoute, MediaRouteError> {
        let selection = request
            .manual_provider
            .as_ref()
            .ok_or(MediaRouteError::ManualProviderRequired)?;

        let matching = self.registry.matching_selection(selection);

        if matching.is_empty() {
            return Err(MediaRouteError::ProviderNotFound {
                provider_id: selection.provider_id.clone(),
                provider_instance_id: selection.provider_instance_id.clone(),
            });
        }

        let mut capable = matching
            .into_iter()
            .filter(|provider| provider.metadata().supports(request.capability))
            .collect::<Vec<_>>();

        if capable.is_empty() {
            return Err(MediaRouteError::UnsupportedCapability {
                provider_id: selection.provider_id.clone(),
                capability: request.capability,
            });
        }

        sort_manual_candidates(&mut capable);

        self.admit_provider(capable[0], request.capability)
    }

    /// Local First is deliberately not Local-then-Cloud.
    ///
    /// If local generation cannot currently execute, the caller must surface
    /// setup/repair or the explicit Use Cloud choice. The router never turns a
    /// free/local request into paid cloud execution silently.
    fn resolve_local_first(
        &self,
        capability: MediaCapability,
    ) -> Result<MediaRoute, MediaRouteError> {
        self.resolve_local(capability)
    }

    fn resolve_local(
        &self,
        capability: MediaCapability,
    ) -> Result<MediaRoute, MediaRouteError> {
        let mut candidates = self
            .registry
            .supporting(capability)
            .into_iter()
            .filter(|provider| {
                provider.metadata().source == MediaProviderSource::Local
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return Err(MediaRouteError::NoCompatibleProvider);
        }

        sort_local_candidates(&mut candidates);

        self.admit_provider(candidates[0], capability)
    }

    fn resolve_cloud(
        &self,
        capability: MediaCapability,
    ) -> Result<MediaRoute, MediaRouteError> {
        let mut candidates = self
            .registry
            .supporting(capability)
            .into_iter()
            .filter(|provider| {
                provider.metadata().source == MediaProviderSource::Cloud
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return Err(MediaRouteError::NoCompatibleProvider);
        }

        sort_cloud_candidates(&mut candidates);

        self.admit_provider(candidates[0], capability)
    }

    fn admit_provider(
        &self,
        provider: &dyn MediaProvider,
        capability: MediaCapability,
    ) -> Result<MediaRoute, MediaRouteError> {
        let metadata = provider.metadata();

        if !metadata.available {
            return Err(MediaRouteError::ProviderUnavailable {
                provider_id: metadata.provider_id.clone(),
                provider_instance_id: metadata.provider_instance_id.clone(),
            });
        }

        if metadata.source == MediaProviderSource::Local {
            let readiness = metadata
                .local_readiness
                .unwrap_or(LocalMediaReadiness::InstalledBroken);

            if readiness != LocalMediaReadiness::Ready {
                return Err(MediaRouteError::LocalSetupRequired {
                    provider_id: metadata.provider_id.clone(),
                    readiness,
                });
            }
        }

        if !metadata.is_authorized() {
            return Err(MediaRouteError::AuthorizationRequired {
                provider_id: metadata.provider_id.clone(),
                provider_instance_id: metadata.provider_instance_id.clone(),
            });
        }

        Ok(MediaRoute {
            provider_id: metadata.provider_id.clone(),
            provider_instance_id: metadata.provider_instance_id.clone(),
            source: metadata.source,
            capability,
        })
    }
}

fn sort_manual_candidates(candidates: &mut Vec<&dyn MediaProvider>) {
    candidates.sort_by_key(|provider| {
        let metadata = provider.metadata();

        (
            !metadata.available,
            local_not_ready(*provider),
            !metadata.is_authorized(),
            metadata.priority,
        )
    });
}

fn sort_local_candidates(candidates: &mut Vec<&dyn MediaProvider>) {
    candidates.sort_by_key(|provider| {
        let metadata = provider.metadata();

        (
            !metadata.available,
            local_not_ready(*provider),
            !metadata.is_authorized(),
            metadata.priority,
        )
    });
}

fn sort_cloud_candidates(candidates: &mut Vec<&dyn MediaProvider>) {
    candidates.sort_by_key(|provider| {
        let metadata = provider.metadata();

        (
            !metadata.available,
            !metadata.recommended_cloud,
            metadata.priority,
        )
    });
}

fn local_not_ready(provider: &dyn MediaProvider) -> bool {
    let metadata = provider.metadata();

    metadata.source == MediaProviderSource::Local
        && metadata.local_readiness != Some(LocalMediaReadiness::Ready)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                CreativeIntent, LawfulContentCompatibility, MediaError, MediaKind, MediaProgress,
                MediaProviderSelection, MediaResult,
            },
            provider::MediaProviderMetadata,
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
            unreachable!("router tests resolve metadata only")
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn provider(
        id: &str,
        instance: Option<&str>,
        source: MediaProviderSource,
        readiness: Option<LocalMediaReadiness>,
        authorized: bool,
        available: bool,
        recommended_cloud: bool,
        priority: u16,
        compatibility: LawfulContentCompatibility,
        capabilities: Vec<MediaCapability>,
    ) -> StubProvider {
        StubProvider {
            metadata: MediaProviderMetadata {
                provider_id: id.to_owned(),
                provider_instance_id: instance.map(str::to_owned),
                source,
                interface_kind: match source {
                    MediaProviderSource::Local => {
                        ProviderInterfaceKind::NativeStructured
                    }
                    MediaProviderSource::Cloud => {
                        ProviderInterfaceKind::OfficialApi
                    }
                },
                authorization_kind: match source {
                    MediaProviderSource::Local => AuthorizationKind::None,
                    MediaProviderSource::Cloud => AuthorizationKind::ApiKey,
                },
                authorization_state: if authorized {
                    AuthorizationState::Connected
                } else {
                    AuthorizationState::AuthorizationRequired
                },
                authorization_ref: None,
                available,
                local_readiness: readiness,
                priority,
                capabilities,
                lawful_content_compatibility: compatibility,
                recommended_cloud,
                supports_progress: true,
                supports_cancellation: true,
                supports_cost_estimation: source == MediaProviderSource::Cloud,
            },
        }
    }

    fn request(
        capability: MediaCapability,
        route_mode: MediaRouteMode,
        execution_target: MediaExecutionTarget,
        manual_provider: Option<MediaProviderSelection>,
    ) -> MediaRequest {
        MediaRequest {
            capability,
            intent: CreativeIntent {
                request: "Generate fixture media".to_owned(),
                media_kind: MediaKind::Image,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
            },
            route_mode,
            execution_target,
            manual_provider,
        }
    }

    fn local_and_cloud_registry(
        readiness: LocalMediaReadiness,
    ) -> MediaProviderRegistry {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "local-fixture",
                None,
                MediaProviderSource::Local,
                Some(readiness),
                true,
                true,
                false,
                10,
                LawfulContentCompatibility::Unknown,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "cloud-recommended",
                Some("cloud-primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                true,
                10,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
    }

    #[test]
    fn local_first_routes_to_ready_local_provider() {
        let registry =
            local_and_cloud_registry(LocalMediaReadiness::Ready);
        let router = MediaRouter::new(&registry);

        let route = router
            .resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::LocalFirst,
                None,
            ))
            .unwrap();

        assert_eq!(route.provider_id, "local-fixture");
        assert_eq!(route.source, MediaProviderSource::Local);
    }

    #[test]
    fn local_first_requires_setup_and_never_silently_uses_cloud() {
        let registry = local_and_cloud_registry(
            LocalMediaReadiness::InstalledNotConfigured,
        );
        let router = MediaRouter::new(&registry);

        assert_eq!(
            router.resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::LocalFirst,
                None,
            )),
            Err(MediaRouteError::LocalSetupRequired {
                provider_id: "local-fixture".to_owned(),
                readiness: LocalMediaReadiness::InstalledNotConfigured,
            })
        );
    }

    #[test]
    fn local_first_with_no_local_provider_never_auto_routes_to_cloud() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "cloud-recommended",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                true,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        assert_eq!(
            router.resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::LocalFirst,
                None,
            )),
            Err(MediaRouteError::NoCompatibleProvider)
        );
    }

    #[test]
    fn explicit_cloud_choice_uses_recommended_cloud_provider() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "cloud-other",
                Some("other-primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "cloud-recommended",
                Some("recommended-primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                true,
                50,
                LawfulContentCompatibility::Restricted,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        let route = router
            .resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::Cloud,
                None,
            ))
            .unwrap();

        assert_eq!(route.provider_id, "cloud-recommended");
        assert_eq!(
            route.provider_instance_id.as_deref(),
            Some("recommended-primary")
        );
    }

    #[test]
    fn recommended_cloud_requires_authorization_without_silent_provider_change() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "cloud-recommended",
                Some("recommended-primary"),
                MediaProviderSource::Cloud,
                None,
                false,
                true,
                true,
                50,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "already-connected-other",
                Some("other-primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        assert_eq!(
            router.resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::Cloud,
                None,
            )),
            Err(MediaRouteError::AuthorizationRequired {
                provider_id: "cloud-recommended".to_owned(),
                provider_instance_id: Some(
                    "recommended-primary".to_owned()
                ),
            })
        );
    }

    #[test]
    fn manual_provider_instance_is_respected_exactly() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "cloud-provider",
                Some("primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "cloud-provider",
                Some("secondary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        let route = router
            .resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Manual,
                MediaExecutionTarget::Cloud,
                Some(MediaProviderSelection {
                    provider_id: "cloud-provider".to_owned(),
                    provider_instance_id: Some("secondary".to_owned()),
                }),
            ))
            .unwrap();

        assert_eq!(
            route.provider_instance_id.as_deref(),
            Some("secondary")
        );
    }

    #[test]
    fn manual_provider_never_falls_back_when_authorization_is_missing() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "manual-provider",
                Some("manual-primary"),
                MediaProviderSource::Cloud,
                None,
                false,
                true,
                false,
                1,
                LawfulContentCompatibility::Restricted,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "other-provider",
                Some("other-primary"),
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                true,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        assert_eq!(
            router.resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Manual,
                MediaExecutionTarget::Cloud,
                Some(MediaProviderSelection {
                    provider_id: "manual-provider".to_owned(),
                    provider_instance_id: Some(
                        "manual-primary".to_owned()
                    ),
                }),
            )),
            Err(MediaRouteError::AuthorizationRequired {
                provider_id: "manual-provider".to_owned(),
                provider_instance_id: Some(
                    "manual-primary".to_owned()
                ),
            })
        );
    }

    #[test]
    fn unsupported_manual_capability_is_rejected_before_execution() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "image-only-provider",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Unknown,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        assert_eq!(
            router.resolve(&request(
                MediaCapability::TextToVideo,
                MediaRouteMode::Manual,
                MediaExecutionTarget::Cloud,
                Some(MediaProviderSelection {
                    provider_id: "image-only-provider".to_owned(),
                    provider_instance_id: None,
                }),
            )),
            Err(MediaRouteError::UnsupportedCapability {
                provider_id: "image-only-provider".to_owned(),
                capability: MediaCapability::TextToVideo,
            })
        );
    }

    #[test]
    fn content_compatibility_metadata_does_not_override_recommended_route() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "broad-provider",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "recommended-provider",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                true,
                50,
                LawfulContentCompatibility::Restricted,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        let route = router
            .resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::Cloud,
                None,
            ))
            .unwrap();

        assert_eq!(route.provider_id, "recommended-provider");
    }

    #[test]
    fn unavailable_recommended_cloud_yields_to_available_cloud() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(provider(
                "unavailable-recommended",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                false,
                true,
                1,
                LawfulContentCompatibility::Broad,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        registry
            .register(provider(
                "available-cloud",
                None,
                MediaProviderSource::Cloud,
                None,
                true,
                true,
                false,
                10,
                LawfulContentCompatibility::Restricted,
                vec![MediaCapability::TextToImage],
            ))
            .unwrap();

        let router = MediaRouter::new(&registry);

        let route = router
            .resolve(&request(
                MediaCapability::TextToImage,
                MediaRouteMode::Auto,
                MediaExecutionTarget::Cloud,
                None,
            ))
            .unwrap();

        assert_eq!(route.provider_id, "available-cloud");
    }
}
