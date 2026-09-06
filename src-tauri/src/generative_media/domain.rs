use serde::{Deserialize, Serialize};

/// The media form produced or consumed by a Generative Media operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MediaKind {
    Image,
    Video,
}

/// Stable product capabilities.
///
/// These describe user-visible abilities rather than any specific provider,
/// model, workflow or operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MediaCapability {
    TextToImage,
    ImageEdit,
    TextToVideo,
    ImageToVideo,
    AnalyzeImageReference,
    AnalyzeVideoReference,
    ReferenceConditionedGeneration,
}

/// Where execution ultimately occurs.
///
/// This is intentionally independent of operating-system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MediaProviderSource {
    Local,
    Cloud,
}

/// Whether routing is selected by AI-OS or explicitly fixed by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MediaRouteMode {
    Auto,
    Manual,
}

/// The execution environment requested by the user-facing flow.
///
/// `LocalFirst` is the default. If a compatible local provider exists but
/// is not Ready, routing stops for setup/repair instead of silently spending
/// money through a cloud provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MediaExecutionTarget {
    LocalFirst,
    Local,
    Cloud,
}

impl Default for MediaExecutionTarget {
    fn default() -> Self {
        Self::LocalFirst
    }
}

/// Provider policy metadata used for recommendation and disclosure.
///
/// This is not an instruction to bypass a provider policy rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LawfulContentCompatibility {
    Broad,
    Restricted,
    Prohibited,
    Unknown,
}

/// An opaque reference to user-supplied media.
///
/// `handle` deliberately has no filesystem-path contract. A platform/provider
/// adapter resolves it later. Shared Generative Media code must not require a
/// POSIX path, Windows path, cloud URL or provider-specific locator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaReference {
    pub id: String,
    pub kind: MediaKind,
    pub handle: String,
}

/// Provider-neutral representation of what the user wants.
///
/// GM-1 keeps this intentionally small. Camera, lighting, composition, motion
/// decomposition and provider-specific prompt compilation belong to GM-5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreativeIntent {
    pub request: String,
    pub media_kind: MediaKind,
    #[serde(default)]
    pub references: Vec<MediaReference>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub preferences: Vec<String>,
}

/// An explicitly selected media provider.
///
/// The instance is optional because local providers such as a future ComfyUI
/// adapter may not use the same account-instance identity as cloud providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaProviderSelection {
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<String>,
}

/// Provider-neutral request entering the Generative Media router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaRequest {
    pub capability: MediaCapability,
    pub intent: CreativeIntent,
    pub route_mode: MediaRouteMode,
    #[serde(default)]
    pub execution_target: MediaExecutionTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_provider: Option<MediaProviderSelection>,
}

/// Opaque result produced by a provider.
///
/// As with references, `handle` is intentionally not an absolute-path field.
/// Platform/provider adapters resolve it into the concrete local or remote
/// representation required by later execution phases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaOutput {
    pub id: String,
    pub kind: MediaKind,
    pub mime_type: String,
    pub handle: String,
}

/// Canonical result returned by Generative Media execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaResult {
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<String>,
    #[serde(default)]
    pub outputs: Vec<MediaOutput>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Provider-neutral progress that can later be mapped into Runtime progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaProgress {
    pub phase: String,
    pub completed_units: Option<u64>,
    pub total_units: Option<u64>,
    pub message: String,
}

/// Stable error categories used by the media domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MediaErrorCode {
    InvalidRequest,
    UnsupportedCapability,
    ProviderUnavailable,
    NotReady,
    Authentication,
    BudgetExceeded,
    PolicyRejected,
    Cancelled,
    ProviderError,
}

/// Normalized media-domain failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaError {
    pub code: MediaErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> MediaReference {
        MediaReference {
            id: "reference-1".to_owned(),
            kind: MediaKind::Image,
            handle: "asset://reference-1".to_owned(),
        }
    }

    fn intent() -> CreativeIntent {
        CreativeIntent {
            request: "Create a premium automotive advertisement".to_owned(),
            media_kind: MediaKind::Image,
            references: vec![reference()],
            constraints: vec!["preserve vehicle proportions".to_owned()],
            preferences: vec!["cinematic".to_owned()],
        }
    }

    #[test]
    fn request_contract_is_provider_and_platform_neutral() {
        let request = MediaRequest {
            capability: MediaCapability::TextToImage,
            intent: intent(),
            route_mode: MediaRouteMode::Auto,
            execution_target: MediaExecutionTarget::LocalFirst,
            manual_provider: None,
        };

        let json = serde_json::to_value(request).unwrap();

        assert_eq!(json["capability"], "text-to-image");
        assert_eq!(json["intent"]["mediaKind"], "image");
        assert_eq!(json["routeMode"], "auto");
        assert_eq!(json["executionTarget"], "local-first");
        assert!(json.get("manualProvider").is_none());

        let serialized = serde_json::to_string(&json).unwrap();
        assert!(!serialized.contains("/Users/"));
        assert!(!serialized.contains("\\\\"));
    }

    #[test]
    fn manual_provider_selection_is_preserved_exactly() {
        let request = MediaRequest {
            capability: MediaCapability::ImageEdit,
            intent: intent(),
            route_mode: MediaRouteMode::Manual,
            execution_target: MediaExecutionTarget::LocalFirst,
            manual_provider: Some(MediaProviderSelection {
                provider_id: "openai".to_owned(),
                provider_instance_id: Some("openai-primary".to_owned()),
            }),
        };

        let json = serde_json::to_value(request).unwrap();

        assert_eq!(json["routeMode"], "manual");
        assert_eq!(json["manualProvider"]["providerId"], "openai");
        assert_eq!(
            json["manualProvider"]["providerInstanceId"],
            "openai-primary"
        );
    }

    #[test]
    fn reference_and_output_handles_are_opaque_contracts() {
        let reference = reference();
        let output = MediaOutput {
            id: "output-1".to_owned(),
            kind: MediaKind::Image,
            mime_type: "image/png".to_owned(),
            handle: "asset://output-1".to_owned(),
        };

        assert_eq!(reference.handle, "asset://reference-1");
        assert_eq!(output.handle, "asset://output-1");
    }

    #[test]
    fn result_and_error_use_canonical_serialization() {
        let result = MediaResult {
            provider_id: "comfyui".to_owned(),
            provider_instance_id: None,
            outputs: vec![MediaOutput {
                id: "output-1".to_owned(),
                kind: MediaKind::Image,
                mime_type: "image/png".to_owned(),
                handle: "asset://output-1".to_owned(),
            }],
            metadata: serde_json::json!({"source": "local"}),
        };

        let result_json = serde_json::to_value(result).unwrap();

        assert_eq!(result_json["providerId"], "comfyui");
        assert_eq!(result_json["outputs"][0]["mimeType"], "image/png");
        assert!(result_json.get("providerInstanceId").is_none());

        let error = MediaError {
            code: MediaErrorCode::PolicyRejected,
            message: "Provider rejected the request.".to_owned(),
            retryable: false,
        };

        let error_json = serde_json::to_value(error).unwrap();

        assert_eq!(error_json["code"], "policy-rejected");
        assert_eq!(error_json["retryable"], false);
    }

    #[test]
    fn provider_metadata_vocabulary_keeps_policy_and_source_separate() {
        assert_eq!(
            serde_json::to_string(&MediaProviderSource::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&LawfulContentCompatibility::Broad).unwrap(),
            "\"broad\""
        );
    }
}
