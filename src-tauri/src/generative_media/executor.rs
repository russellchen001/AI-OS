use super::{
    domain::{
        MediaError, MediaErrorCode, MediaProgress, MediaProviderSelection, MediaRequest,
        MediaResult,
    },
    prompt_intelligence::prepare_media_request,
    registry::MediaProviderRegistry,
    router::{MediaRouteError, MediaRouter},
};

/// Executes one provider-neutral Generative Media request.
///
/// Routing and execution deliberately remain separate:
///
/// 1. MediaRouter selects exactly one eligible provider identity.
/// 2. The registry resolves that exact identity.
/// 3. Only that provider is executed.
///
/// This prevents an execution failure or policy rejection from silently turning
/// into a provider switch.
pub(crate) fn execute_media_request(
    registry: &MediaProviderRegistry,
    request: &MediaRequest,
    report: &mut dyn FnMut(MediaProgress),
) -> Result<MediaResult, MediaError> {
    let route = MediaRouter::new(registry)
        .resolve(request)
        .map_err(map_route_error)?;

    let selection = MediaProviderSelection {
        provider_id: route.provider_id.clone(),
        provider_instance_id: route.provider_instance_id.clone(),
    };

    let provider = registry
        .matching_selection(&selection)
        .into_iter()
        .find(|provider| {
            let metadata = provider.metadata();

            metadata.source == route.source && metadata.supports(route.capability)
        })
        .ok_or_else(|| MediaError {
            code: MediaErrorCode::ProviderUnavailable,
            message: "The routed Generative Media provider is no longer available.".to_owned(),
            retryable: true,
        })?;

    let prepared = prepare_media_request(provider.metadata(), request);
    let mut result = provider.execute(&prepared.request, report)?;

    if result.provider_id != route.provider_id
        || result.provider_instance_id != route.provider_instance_id
    {
        return Err(MediaError {
            code: MediaErrorCode::ProviderError,
            message: "The Generative Media provider returned an unexpected provider identity."
                .to_owned(),
            retryable: false,
        });
    }

    let prompt_metadata = serde_json::to_value(prepared.metadata).map_err(|_| MediaError {
        code: MediaErrorCode::ProviderError,
        message: "Prompt Intelligence metadata could not be serialized.".to_owned(),
        retryable: false,
    })?;

    match result.metadata.as_object_mut() {
        Some(metadata) => {
            metadata.insert("promptIntelligence".to_owned(), prompt_metadata);
        }
        None => {
            result.metadata = serde_json::json!({
                "providerMetadata": result.metadata,
                "promptIntelligence": prompt_metadata,
            });
        }
    }

    Ok(result)
}

fn map_route_error(error: MediaRouteError) -> MediaError {
    let code = match &error {
        MediaRouteError::ManualProviderRequired | MediaRouteError::ProviderNotFound { .. } => {
            MediaErrorCode::InvalidRequest
        }
        MediaRouteError::UnsupportedCapability { .. } => MediaErrorCode::UnsupportedCapability,
        MediaRouteError::AuthorizationRequired { .. } => MediaErrorCode::Authentication,
        MediaRouteError::LocalSetupRequired { .. } => MediaErrorCode::NotReady,
        _ => MediaErrorCode::ProviderUnavailable,
    };

    let retryable = matches!(code, MediaErrorCode::ProviderUnavailable);

    MediaError {
        code,
        message: format!("Generative Media routing failed: {error:?}"),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                CreativeIntent, LawfulContentCompatibility, MediaCapability, MediaExecutionTarget,
                MediaKind, MediaOutput, MediaProviderSource, MediaRouteMode,
            },
            provider::{LocalMediaReadiness, MediaProvider, MediaProviderMetadata},
        },
        provider_selection::{AuthorizationKind, AuthorizationState, ProviderInterfaceKind},
    };

    struct StubProvider {
        metadata: MediaProviderMetadata,
        outcome: Result<MediaResult, MediaError>,
        progress: Vec<MediaProgress>,
    }

    impl MediaProvider for StubProvider {
        fn metadata(&self) -> &MediaProviderMetadata {
            &self.metadata
        }

        fn execute(
            &self,
            _request: &MediaRequest,
            report: &mut dyn FnMut(MediaProgress),
        ) -> Result<MediaResult, MediaError> {
            for progress in &self.progress {
                report(progress.clone());
            }

            self.outcome.clone()
        }
    }

    fn metadata(
        provider_id: &str,
        instance: Option<&str>,
        source: MediaProviderSource,
    ) -> MediaProviderMetadata {
        MediaProviderMetadata {
            provider_id: provider_id.to_owned(),
            provider_instance_id: instance.map(str::to_owned),
            source,
            interface_kind: if source == MediaProviderSource::Local {
                ProviderInterfaceKind::NativeStructured
            } else {
                ProviderInterfaceKind::OfficialApi
            },
            authorization_kind: if source == MediaProviderSource::Local {
                AuthorizationKind::None
            } else {
                AuthorizationKind::ApiKey
            },
            authorization_state: AuthorizationState::Connected,
            authorization_ref: None,
            available: true,
            local_readiness: if source == MediaProviderSource::Local {
                Some(LocalMediaReadiness::Ready)
            } else {
                None
            },
            priority: 10,
            capabilities: vec![MediaCapability::TextToImage],
            lawful_content_compatibility: LawfulContentCompatibility::Unknown,
            recommended_cloud: source == MediaProviderSource::Cloud,
            supports_progress: true,
            supports_cancellation: true,
            supports_cost_estimation: false,
        }
    }

    fn request(
        provider_id: &str,
        instance: Option<&str>,
        target: MediaExecutionTarget,
    ) -> MediaRequest {
        MediaRequest {
            capability: MediaCapability::TextToImage,
            intent: CreativeIntent {
                request: "Create a product photograph".to_owned(),
                media_kind: MediaKind::Image,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
                details: Default::default(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: target,
            manual_provider: Some(MediaProviderSelection {
                provider_id: provider_id.to_owned(),
                provider_instance_id: instance.map(str::to_owned),
            }),
            options: Default::default(),
            normalized_references: Vec::new(),
        }
    }

    fn success(provider_id: &str, instance: Option<&str>) -> MediaResult {
        MediaResult {
            provider_id: provider_id.to_owned(),
            provider_instance_id: instance.map(str::to_owned),
            outputs: vec![MediaOutput {
                id: "output-1".to_owned(),
                kind: MediaKind::Image,
                mime_type: "image/png".to_owned(),
                handle: "asset://output-1".to_owned(),
            }],
            metadata: serde_json::json!({"fixture": true}),
        }
    }

    #[test]
    fn routed_provider_executes_and_progress_is_preserved() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(StubProvider {
                metadata: metadata("local-fixture", None, MediaProviderSource::Local),
                outcome: Ok(success("local-fixture", None)),
                progress: vec![MediaProgress {
                    phase: "generate".to_owned(),
                    completed_units: Some(1),
                    total_units: Some(2),
                    message: "Generating".to_owned(),
                }],
            })
            .unwrap();

        let mut progress = Vec::new();

        let result = execute_media_request(
            &registry,
            &request("local-fixture", None, MediaExecutionTarget::Local),
            &mut |update| progress.push(update),
        )
        .unwrap();

        assert_eq!(result.provider_id, "local-fixture");
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].phase, "generate");
    }

    #[test]
    fn provider_error_is_not_replaced_by_another_provider() {
        let mut registry = MediaProviderRegistry::new();

        let expected = MediaError {
            code: MediaErrorCode::PolicyRejected,
            message: "Provider rejected this request.".to_owned(),
            retryable: false,
        };

        registry
            .register(StubProvider {
                metadata: metadata(
                    "cloud-selected",
                    Some("primary"),
                    MediaProviderSource::Cloud,
                ),
                outcome: Err(expected.clone()),
                progress: Vec::new(),
            })
            .unwrap();

        registry
            .register(StubProvider {
                metadata: metadata("cloud-other", Some("primary"), MediaProviderSource::Cloud),
                outcome: Ok(success("cloud-other", Some("primary"))),
                progress: Vec::new(),
            })
            .unwrap();

        let error = execute_media_request(
            &registry,
            &request(
                "cloud-selected",
                Some("primary"),
                MediaExecutionTarget::Cloud,
            ),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error, expected);
    }

    #[test]
    fn provider_must_return_the_identity_selected_by_router() {
        let mut registry = MediaProviderRegistry::new();

        registry
            .register(StubProvider {
                metadata: metadata(
                    "cloud-selected",
                    Some("primary"),
                    MediaProviderSource::Cloud,
                ),
                outcome: Ok(success("different-provider", Some("primary"))),
                progress: Vec::new(),
            })
            .unwrap();

        let error = execute_media_request(
            &registry,
            &request(
                "cloud-selected",
                Some("primary"),
                MediaExecutionTarget::Cloud,
            ),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.code, MediaErrorCode::ProviderError);
        assert!(!error.retryable);
    }

    #[test]
    fn empty_registry_reports_provider_unavailable_without_execution() {
        let registry = MediaProviderRegistry::new();

        let error = execute_media_request(
            &registry,
            &MediaRequest {
                capability: MediaCapability::TextToImage,
                intent: CreativeIntent {
                    request: "Create an image".to_owned(),
                    media_kind: MediaKind::Image,
                    references: Vec::new(),
                    constraints: Vec::new(),
                    preferences: Vec::new(),
                    details: Default::default(),
                },
                route_mode: MediaRouteMode::Auto,
                execution_target: MediaExecutionTarget::LocalFirst,
                manual_provider: None,
                options: Default::default(),
                normalized_references: Vec::new(),
            },
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.code, MediaErrorCode::ProviderUnavailable);
    }
}
