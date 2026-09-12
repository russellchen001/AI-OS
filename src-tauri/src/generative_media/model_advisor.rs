use super::{
    domain::{MediaCapability, MediaRequest},
    provider::MediaProviderMetadata,
};
use serde::Serialize;

pub(crate) const MODEL_ADVISOR_SOURCE: &str = "ai-os/provider-scoped-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaModelAdvice {
    pub provider_id: String,
    pub provider_instance_id: Option<String>,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
    pub source: String,
    pub user_authoritative: bool,
}

/// Recommend only beneath an already selected Provider identity.
///
/// GM-5 deliberately does not embed a hardware database. The local result is
/// a stable profile recommendation which a future llmfit-compatible adapter
/// may refine; Ready First remains the execution authority.
pub(crate) fn advise_model(
    provider: &MediaProviderMetadata,
    request: &MediaRequest,
) -> MediaModelAdvice {
    let user_authoritative =
        request.options.model_id.is_some() || request.options.profile_id.is_some();

    let (default_model, default_profile) = match provider.provider_id.as_str() {
        "grok" => match request.capability {
            MediaCapability::TextToVideo | MediaCapability::ImageToVideo => {
                (Some("grok-imagine-video-1.5"), None)
            }
            _ => (Some("grok-imagine-image-2.0"), None),
        },
        "openai" => (Some("gpt-image-2"), None),
        "comfyui" => match request.capability {
            MediaCapability::AnalyzeImageReference | MediaCapability::AnalyzeVideoReference => (
                Some("Qwen 3 VL 2B Instruct"),
                Some("reference.vlm.standard"),
            ),
            MediaCapability::ReferenceConditionedGeneration => {
                (None, Some("comfyui-checkpoint-t2i-v1"))
            }
            _ => (None, Some("comfyui-checkpoint-t2i-v1")),
        },
        _ => (None, None),
    };

    MediaModelAdvice {
        provider_id: provider.provider_id.clone(),
        provider_instance_id: provider.provider_instance_id.clone(),
        model_id: request
            .options
            .model_id
            .clone()
            .or_else(|| default_model.map(str::to_owned)),
        profile_id: request
            .options
            .profile_id
            .clone()
            .or_else(|| default_profile.map(str::to_owned)),
        source: MODEL_ADVISOR_SOURCE.to_owned(),
        user_authoritative,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                CreativeIntent, LawfulContentCompatibility, MediaExecutionTarget,
                MediaGenerationOptions, MediaKind, MediaProviderSource, MediaRouteMode,
            },
            provider::{LocalMediaReadiness, MediaProviderMetadata},
        },
        provider_selection::{AuthorizationKind, AuthorizationState, ProviderInterfaceKind},
    };

    fn provider(id: &str, instance: &str, source: MediaProviderSource) -> MediaProviderMetadata {
        MediaProviderMetadata {
            provider_id: id.to_owned(),
            provider_instance_id: Some(instance.to_owned()),
            source,
            interface_kind: ProviderInterfaceKind::NativeStructured,
            authorization_kind: AuthorizationKind::None,
            authorization_state: AuthorizationState::Connected,
            authorization_ref: None,
            available: true,
            local_readiness: (source == MediaProviderSource::Local)
                .then_some(LocalMediaReadiness::Ready),
            priority: 1,
            capabilities: vec![MediaCapability::TextToImage],
            lawful_content_compatibility: LawfulContentCompatibility::Unknown,
            recommended_cloud: false,
            supports_progress: true,
            supports_cancellation: true,
            supports_cost_estimation: false,
        }
    }

    fn request() -> MediaRequest {
        MediaRequest {
            capability: MediaCapability::TextToImage,
            intent: CreativeIntent {
                request: "studio portrait".to_owned(),
                media_kind: MediaKind::Image,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
                details: Default::default(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::Cloud,
            manual_provider: None,
            options: MediaGenerationOptions::default(),
            normalized_references: Vec::new(),
        }
    }

    #[test]
    fn advice_is_scoped_to_the_selected_provider_instance() {
        let advice = advise_model(
            &provider("grok", "account-a", MediaProviderSource::Cloud),
            &request(),
        );

        assert_eq!(advice.provider_id, "grok");
        assert_eq!(advice.provider_instance_id.as_deref(), Some("account-a"));
        assert_eq!(advice.model_id.as_deref(), Some("grok-imagine-image-2.0"));
    }

    #[test]
    fn manual_model_and_profile_are_never_replaced() {
        let mut request = request();
        request.options.model_id = Some("owner-selected".to_owned());
        request.options.profile_id = Some("owner-profile".to_owned());

        let advice = advise_model(
            &provider("openai", "account-b", MediaProviderSource::Cloud),
            &request,
        );

        assert_eq!(advice.provider_id, "openai");
        assert_eq!(advice.model_id.as_deref(), Some("owner-selected"));
        assert_eq!(advice.profile_id.as_deref(), Some("owner-profile"));
        assert!(advice.user_authoritative);
    }
}
