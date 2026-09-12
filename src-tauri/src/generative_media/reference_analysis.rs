use super::domain::{MediaError, MediaKind, MediaReference, ReferenceSpec};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub(crate) const MAX_REFERENCE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_REFERENCE_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReferenceAnalyzerIdentity {
    pub provider_id: String,
    pub provider_instance_id: Option<String>,
}

pub(crate) trait ReferenceAnalysisAdapter: Send + Sync {
    fn identity(&self) -> ReferenceAnalyzerIdentity;
    fn analyze(&self, reference: &MediaReference) -> Result<ReferenceSpec, MediaError>;
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceResponse {
    summary: String,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    temporal_events: Vec<String>,
}

pub(crate) fn normalize_reference_response(
    reference: &MediaReference,
    identity: &ReferenceAnalyzerIdentity,
    source_bytes: &[u8],
    response: &str,
) -> Result<ReferenceSpec, String> {
    if source_bytes.is_empty() || source_bytes.len() > MAX_REFERENCE_BYTES {
        return Err("Reference asset is empty or exceeds the analysis bound".to_owned());
    }
    if response.is_empty() || response.len() > MAX_REFERENCE_TEXT_BYTES {
        return Err("Reference analysis output is empty or exceeds the text bound".to_owned());
    }

    let parsed = parse_response(response).unwrap_or_else(|| ReferenceResponse {
        summary: response.trim().to_owned(),
        constraints: Vec::new(),
        temporal_events: Vec::new(),
    });
    let summary = parsed.summary.trim();
    if summary.is_empty() {
        return Err("Reference analysis produced no summary".to_owned());
    }

    let temporal_events = if reference.kind == MediaKind::Video {
        bounded_strings(parsed.temporal_events, 32, 1_024)
    } else {
        Vec::new()
    };

    Ok(ReferenceSpec {
        source_reference_id: reference.id.clone(),
        kind: reference.kind,
        analyzer_provider_id: identity.provider_id.clone(),
        analyzer_provider_instance_id: identity.provider_instance_id.clone(),
        summary: summary.chars().take(8_192).collect(),
        constraints: bounded_strings(parsed.constraints, 32, 1_024),
        temporal_events,
        provenance_sha256: sha256(source_bytes),
    })
}

fn parse_response(response: &str) -> Option<ReferenceResponse> {
    let trimmed = response.trim();
    let candidate = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim();

    serde_json::from_str(candidate).ok()
}

fn bounded_strings(values: Vec<String>, count: usize, length: usize) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().chars().take(length).collect::<String>())
        .filter(|value| !value.is_empty())
        .take(count)
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

    fn identity() -> ReferenceAnalyzerIdentity {
        ReferenceAnalyzerIdentity {
            provider_id: "comfyui-vlm".to_owned(),
            provider_instance_id: Some("local-a".to_owned()),
        }
    }

    #[test]
    fn image_response_normalizes_with_traceable_provenance() {
        let reference = MediaReference {
            id: "image-1".to_owned(),
            kind: MediaKind::Image,
            handle: "asset://image-1".to_owned(),
        };
        let spec = normalize_reference_response(
            &reference,
            &identity(),
            b"bounded image bytes",
            r#"{"summary":"blue package on white","constraints":["preserve blue packaging"],"temporalEvents":["ignored"]}"#,
        )
        .unwrap();

        assert_eq!(spec.source_reference_id, "image-1");
        assert_eq!(spec.kind, MediaKind::Image);
        assert_eq!(spec.analyzer_provider_id, "comfyui-vlm");
        assert_eq!(spec.constraints, ["preserve blue packaging"]);
        assert!(spec.temporal_events.is_empty());
        assert_eq!(spec.provenance_sha256.len(), 64);
        assert!(!spec.provenance_sha256.contains("bounded image bytes"));
    }

    #[test]
    fn video_response_preserves_bounded_temporal_events() {
        let reference = MediaReference {
            id: "video-1".to_owned(),
            kind: MediaKind::Video,
            handle: "asset://video-1".to_owned(),
        };
        let spec = normalize_reference_response(
            &reference,
            &identity(),
            b"bounded video bytes",
            r#"```json
{"summary":"camera circles a bottle","constraints":["keep clockwise motion"],"temporalEvents":["0-1s reveal","1-2s orbit"]}
```"#,
        )
        .unwrap();

        assert_eq!(spec.kind, MediaKind::Video);
        assert_eq!(spec.temporal_events.len(), 2);
    }

    #[test]
    fn plain_text_fallback_is_normalized_without_inventing_constraints() {
        let reference = MediaReference {
            id: "image-2".to_owned(),
            kind: MediaKind::Image,
            handle: "asset://image-2".to_owned(),
        };
        let spec = normalize_reference_response(
            &reference,
            &identity(),
            b"bytes",
            "A close portrait with warm rim light.",
        )
        .unwrap();

        assert_eq!(spec.summary, "A close portrait with warm rim light.");
        assert!(spec.constraints.is_empty());
    }

    #[test]
    fn oversized_reference_is_rejected_before_analysis_normalization() {
        let reference = MediaReference {
            id: "large".to_owned(),
            kind: MediaKind::Image,
            handle: "asset://large".to_owned(),
        };
        let bytes = vec![0_u8; MAX_REFERENCE_BYTES + 1];

        assert!(normalize_reference_response(&reference, &identity(), &bytes, "summary").is_err());
    }
}
