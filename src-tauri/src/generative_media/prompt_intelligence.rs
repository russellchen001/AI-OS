use super::{
    domain::{MediaKind, MediaRequest, ReferenceSpec},
    model_advisor::{advise_model, MediaModelAdvice},
    provider::MediaProviderMetadata,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(crate) const PROMPT_COMPILER_SOURCE: &str = "ai-os/provider-dialect-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptOutputSpec {
    pub media_kind: MediaKind,
    pub aspect_ratio: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptSpec {
    pub original_prompt: String,
    pub subject: Option<String>,
    pub scene_context: Option<String>,
    pub composition: Option<String>,
    pub camera_framing: Option<String>,
    pub style: Option<String>,
    pub lighting: Option<String>,
    pub mood: Option<String>,
    pub color: Option<String>,
    pub motion: Option<String>,
    pub temporal_behavior: Option<String>,
    pub negative_constraints: Vec<String>,
    pub hard_constraints: Vec<String>,
    pub preferences: Vec<String>,
    pub reference_constraints: Vec<String>,
    pub output: PromptOutputSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptPreparationMetadata {
    pub compiler: String,
    pub provider_id: String,
    pub provider_instance_id: Option<String>,
    pub model_advice: MediaModelAdvice,
    pub original_prompt_preserved: bool,
    pub compiled_prompt_sha256: String,
    pub normalized_reference_count: usize,
}

pub(crate) struct PreparedMediaRequest {
    pub request: MediaRequest,
    pub metadata: PromptPreparationMetadata,
}

pub(crate) fn prompt_spec(request: &MediaRequest) -> PromptSpec {
    let details = &request.intent.details;

    PromptSpec {
        original_prompt: request.intent.request.clone(),
        subject: details.subject.clone(),
        scene_context: details.scene_context.clone(),
        composition: details.composition.clone(),
        camera_framing: details.camera_framing.clone(),
        style: details.style.clone(),
        lighting: details.lighting.clone(),
        mood: details.mood.clone(),
        color: details.color.clone(),
        motion: details.motion.clone(),
        temporal_behavior: details.temporal_behavior.clone(),
        negative_constraints: details.negative_constraints.clone(),
        hard_constraints: request.intent.constraints.clone(),
        preferences: request.intent.preferences.clone(),
        reference_constraints: reference_constraints(&request.normalized_references),
        output: PromptOutputSpec {
            media_kind: request.intent.media_kind,
            aspect_ratio: request.options.aspect_ratio.clone(),
            width: request.options.width,
            height: request.options.height,
            duration_seconds: request.options.duration_seconds,
        },
    }
}

/// Prepare a request only after MediaRouter fixed a Provider identity.
pub(crate) fn prepare_media_request(
    provider: &MediaProviderMetadata,
    request: &MediaRequest,
) -> PreparedMediaRequest {
    let spec = prompt_spec(request);
    let preserve = should_preserve_verbatim(request, &spec);
    let prompt = if preserve {
        request.intent.request.clone()
    } else {
        compile_for_provider(provider.provider_id.as_str(), &spec)
    };
    let advice = advise_model(provider, request);

    let mut prepared = request.clone();
    prepared.intent.request = prompt.clone();
    if prepared.options.model_id.is_none() {
        prepared.options.model_id = advice.model_id.clone();
    }
    if prepared.options.profile_id.is_none() {
        prepared.options.profile_id = advice.profile_id.clone();
    }

    PreparedMediaRequest {
        request: prepared,
        metadata: PromptPreparationMetadata {
            compiler: PROMPT_COMPILER_SOURCE.to_owned(),
            provider_id: provider.provider_id.clone(),
            provider_instance_id: provider.provider_instance_id.clone(),
            model_advice: advice,
            original_prompt_preserved: preserve,
            compiled_prompt_sha256: sha256(prompt.as_bytes()),
            normalized_reference_count: request.normalized_references.len(),
        },
    }
}

fn should_preserve_verbatim(request: &MediaRequest, spec: &PromptSpec) -> bool {
    !request.options.enhance_prompt
        && request.intent.details.is_empty()
        && spec.hard_constraints.is_empty()
        && spec.preferences.is_empty()
        && spec.reference_constraints.is_empty()
}

fn compile_for_provider(provider_id: &str, spec: &PromptSpec) -> String {
    if provider_id == "comfyui" {
        compile_tag_dialect(spec)
    } else {
        compile_instruction_dialect(spec)
    }
}

fn compile_tag_dialect(spec: &PromptSpec) -> String {
    let mut positive = vec![spec.original_prompt.trim().to_owned()];
    append_fields(&mut positive, spec);

    let mut compiled = positive
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let negatives = joined_constraints(&spec.negative_constraints);
    if !negatives.is_empty() {
        compiled.push_str(". Avoid: ");
        compiled.push_str(&negatives);
    }

    compiled
}

fn compile_instruction_dialect(spec: &PromptSpec) -> String {
    let mut clauses = vec![spec.original_prompt.trim().to_owned()];
    append_fields(&mut clauses, spec);

    let mut compiled = clauses
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(". ");

    let negatives = joined_constraints(&spec.negative_constraints);
    if !negatives.is_empty() {
        compiled.push_str(". Do not include: ");
        compiled.push_str(&negatives);
    }

    compiled
}

fn append_fields(values: &mut Vec<String>, spec: &PromptSpec) {
    for value in [
        spec.subject.as_ref(),
        spec.scene_context.as_ref(),
        spec.composition.as_ref(),
        spec.camera_framing.as_ref(),
        spec.style.as_ref(),
        spec.lighting.as_ref(),
        spec.mood.as_ref(),
        spec.color.as_ref(),
        spec.motion.as_ref(),
        spec.temporal_behavior.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            values.push(trimmed.to_owned());
        }
    }

    values.extend(non_empty(&spec.hard_constraints));
    values.extend(non_empty(&spec.reference_constraints));
    values.extend(non_empty(&spec.preferences));
}

fn non_empty(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn joined_constraints(values: &[String]) -> String {
    non_empty(values).join(", ")
}

fn reference_constraints(references: &[ReferenceSpec]) -> Vec<String> {
    references
        .iter()
        .flat_map(|reference| {
            std::iter::once(reference.summary.as_str())
                .chain(reference.constraints.iter().map(String::as_str))
                .chain(reference.temporal_events.iter().map(String::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generative_media::{
            domain::{
                CreativeIntent, CreativeIntentDetails, LawfulContentCompatibility, MediaCapability,
                MediaExecutionTarget, MediaGenerationOptions, MediaProviderSource, MediaRouteMode,
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

    fn request(prompt: &str) -> MediaRequest {
        MediaRequest {
            capability: MediaCapability::TextToImage,
            intent: CreativeIntent {
                request: prompt.to_owned(),
                media_kind: MediaKind::Image,
                references: Vec::new(),
                constraints: Vec::new(),
                preferences: Vec::new(),
                details: CreativeIntentDetails::default(),
            },
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::Cloud,
            manual_provider: None,
            options: MediaGenerationOptions::default(),
            normalized_references: Vec::new(),
        }
    }

    #[test]
    fn deliberate_expert_prompt_remains_verbatim() {
        let original = "85 mm portrait, f/2, Rembrandt lighting -- no text";
        let prepared = prepare_media_request(
            &provider("openai", "owner-account", MediaProviderSource::Cloud),
            &request(original),
        );

        assert_eq!(prepared.request.intent.request, original);
        assert!(prepared.metadata.original_prompt_preserved);
    }

    #[test]
    fn structured_fields_compile_for_the_fixed_provider() {
        let mut request = request("Photograph the product");
        request.intent.details.subject = Some("a red ceramic teapot".to_owned());
        request.intent.details.lighting = Some("soft window light".to_owned());
        request.intent.details.negative_constraints = vec!["logos".to_owned()];

        let fixed = provider("comfyui", "local-a", MediaProviderSource::Local);
        let prepared = prepare_media_request(&fixed, &request);

        assert_eq!(prepared.metadata.provider_id, "comfyui");
        assert_eq!(
            prepared.metadata.provider_instance_id.as_deref(),
            Some("local-a")
        );
        assert!(prepared
            .request
            .intent
            .request
            .contains("red ceramic teapot"));
        assert!(prepared.request.intent.request.contains("Avoid: logos"));
    }

    #[test]
    fn manual_model_is_preserved_while_provider_stays_fixed() {
        let mut request = request("Generate a clean icon");
        request.options.model_id = Some("owner-model".to_owned());
        request.options.enhance_prompt = true;

        let prepared = prepare_media_request(
            &provider("grok", "xai-a", MediaProviderSource::Cloud),
            &request,
        );

        assert_eq!(
            prepared.request.options.model_id.as_deref(),
            Some("owner-model")
        );
        assert_eq!(prepared.metadata.provider_id, "grok");
    }

    #[test]
    fn normalized_reference_constraints_feed_the_compiled_request() {
        let mut request = request("Create a matching poster");
        request.normalized_references.push(ReferenceSpec {
            source_reference_id: "ref-1".to_owned(),
            kind: MediaKind::Image,
            analyzer_provider_id: "comfyui-vlm".to_owned(),
            analyzer_provider_instance_id: Some("local-a".to_owned()),
            summary: "blue geometric poster".to_owned(),
            constraints: vec!["preserve the blue geometric layout".to_owned()],
            temporal_events: Vec::new(),
            provenance_sha256: "a".repeat(64),
        });

        let prepared = prepare_media_request(
            &provider("openai", "account-a", MediaProviderSource::Cloud),
            &request,
        );

        assert!(prepared
            .request
            .intent
            .request
            .contains("preserve the blue geometric layout"));
        assert!(prepared
            .request
            .intent
            .request
            .contains("blue geometric poster"));
        assert_eq!(prepared.metadata.normalized_reference_count, 1);
    }

    #[test]
    fn metadata_contains_only_prompt_digest_not_prompt_text() {
        let secret_prompt = "private unpublished campaign concept";
        let prepared = prepare_media_request(
            &provider("openai", "account-a", MediaProviderSource::Cloud),
            &request(secret_prompt),
        );
        let metadata = serde_json::to_string(&prepared.metadata).unwrap();

        assert!(!metadata.contains(secret_prompt));
        assert_eq!(prepared.metadata.compiled_prompt_sha256.len(), 64);
    }
}
