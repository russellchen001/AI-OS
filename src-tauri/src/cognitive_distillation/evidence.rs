use super::{SourceKind, SourceMediaKind, SubjectKind};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

const MAX_EXTRACTED_TEXT_BYTES: usize = 128 * 1024;
const MAX_OBSERVATIONS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceArtifact {
    pub source_id: String,
    pub media_kind: SourceMediaKind,
    pub source_kind: SourceKind,
    pub opaque_reference: String,
    pub source_digest: String,
    pub correlation_group: String,
    pub authorized: bool,
    pub private: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExtractionTarget {
    Local,
    Cloud,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtractionPolicy {
    pub target: ExtractionTarget,
    pub cloud_authorized: bool,
    pub public_research_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceLocation {
    pub page_start: Option<u32>,
    pub page_end: Option<u32>,
    pub time_start_ms: Option<u64>,
    pub time_end_ms: Option<u64>,
    pub region: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EvidenceAssertion {
    Confirmed,
    Inferred,
    Contradictory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtractedEvidenceItem {
    pub evidence_id: String,
    pub location: EvidenceLocation,
    pub speaker: Option<String>,
    pub extracted_text: Option<String>,
    #[serde(default)]
    pub visual_observations: Vec<String>,
    #[serde(default)]
    pub structural_relations: Vec<String>,
    #[serde(default)]
    pub contextual_observations: Vec<String>,
    pub assertion: EvidenceAssertion,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceExtraction {
    pub extractor_identity: String,
    pub extractor_revision: String,
    #[serde(default)]
    pub asserted_sensitive_traits: Vec<String>,
    pub items: Vec<ExtractedEvidenceItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DistillationEvidence {
    pub evidence_id: String,
    pub source_id: String,
    pub media_kind: SourceMediaKind,
    pub source_digest: String,
    pub correlation_group: String,
    pub location: EvidenceLocation,
    pub speaker: Option<String>,
    pub extracted_text: Option<String>,
    pub visual_observations: Vec<String>,
    pub structural_relations: Vec<String>,
    pub contextual_observations: Vec<String>,
    pub assertion: EvidenceAssertion,
    pub provenance: String,
    pub confidence: f32,
    pub extractor_identity: String,
    pub extractor_revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceBundle {
    pub bundle_id: String,
    pub subject_kind: SubjectKind,
    pub media_kind: SourceMediaKind,
    pub sources: Vec<SourceArtifact>,
    pub evidence: Vec<DistillationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvidenceError {
    InvalidSource,
    UnauthorizedSource,
    CloudAuthorizationRequired,
    PrivatePublicResearchForbidden,
    SensitiveInferenceForbidden,
    InvalidExtraction,
    MissingModalityEvidence,
    InvalidBundle,
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSource => "Distillation source metadata is invalid.",
            Self::UnauthorizedSource => "Distillation source is not authorized.",
            Self::CloudAuthorizationRequired => {
                "Cloud extraction requires explicit authorization before source content is sent."
            }
            Self::PrivatePublicResearchForbidden => {
                "Private-person media cannot trigger public research."
            }
            Self::SensitiveInferenceForbidden => {
                "Sensitive personal-attribute inference is forbidden."
            }
            Self::InvalidExtraction => {
                "Extracted evidence does not match the bounded evidence contract."
            }
            Self::MissingModalityEvidence => {
                "Extraction omitted evidence required by the source modality."
            }
            Self::InvalidBundle => "Evidence bundle is invalid or has lost source provenance.",
        })
    }
}

impl Error for EvidenceError {}

pub(crate) fn normalize_extraction(
    subject_kind: SubjectKind,
    source: &SourceArtifact,
    policy: &ExtractionPolicy,
    extraction: EvidenceExtraction,
) -> Result<Vec<DistillationEvidence>, EvidenceError> {
    validate_source(source)?;
    if !source.authorized {
        return Err(EvidenceError::UnauthorizedSource);
    }
    if policy.target == ExtractionTarget::Cloud && !policy.cloud_authorized {
        return Err(EvidenceError::CloudAuthorizationRequired);
    }
    if subject_kind == SubjectKind::PrivatePerson && policy.public_research_authorized {
        return Err(EvidenceError::PrivatePublicResearchForbidden);
    }
    if !extraction.asserted_sensitive_traits.is_empty() {
        return Err(EvidenceError::SensitiveInferenceForbidden);
    }
    if extraction.extractor_identity.trim().is_empty()
        || extraction.extractor_revision.trim().is_empty()
        || extraction.items.is_empty()
    {
        return Err(EvidenceError::InvalidExtraction);
    }

    let mut identifiers = HashSet::new();
    let mut normalized = Vec::with_capacity(extraction.items.len());
    for item in extraction.items {
        if !identifiers.insert(item.evidence_id.clone())
            || item.evidence_id.trim().is_empty()
            || !(0.0..=1.0).contains(&item.confidence)
            || item
                .extracted_text
                .as_ref()
                .is_some_and(|text| text.len() > MAX_EXTRACTED_TEXT_BYTES)
            || item.visual_observations.len() > MAX_OBSERVATIONS
            || item.structural_relations.len() > MAX_OBSERVATIONS
            || item.contextual_observations.len() > MAX_OBSERVATIONS
            || !valid_location(&item.location)
        {
            return Err(EvidenceError::InvalidExtraction);
        }
        require_modality_evidence(source.media_kind, &item)?;
        normalized.push(DistillationEvidence {
            evidence_id: item.evidence_id,
            source_id: source.source_id.clone(),
            media_kind: source.media_kind,
            source_digest: source.source_digest.clone(),
            correlation_group: source.correlation_group.clone(),
            location: item.location,
            speaker: item.speaker,
            extracted_text: item.extracted_text,
            visual_observations: item.visual_observations,
            structural_relations: item.structural_relations,
            contextual_observations: item.contextual_observations,
            assertion: item.assertion,
            provenance: source.opaque_reference.clone(),
            confidence: item.confidence,
            extractor_identity: extraction.extractor_identity.clone(),
            extractor_revision: extraction.extractor_revision.clone(),
        });
    }
    Ok(normalized)
}

fn validate_source(source: &SourceArtifact) -> Result<(), EvidenceError> {
    if source.source_id.trim().is_empty()
        || source.opaque_reference.trim().is_empty()
        || source.source_digest.len() != 64
        || !source
            .source_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || source.correlation_group.trim().is_empty()
        || source.media_kind == SourceMediaKind::Mixed
    {
        return Err(EvidenceError::InvalidSource);
    }
    Ok(())
}

fn valid_location(location: &EvidenceLocation) -> bool {
    location
        .page_start
        .zip(location.page_end)
        .is_none_or(|(start, end)| start > 0 && start <= end)
        && location
            .time_start_ms
            .zip(location.time_end_ms)
            .is_none_or(|(start, end)| start <= end)
}

fn require_modality_evidence(
    kind: SourceMediaKind,
    item: &ExtractedEvidenceItem,
) -> Result<(), EvidenceError> {
    let has_text = item
        .extracted_text
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_visual = !item.visual_observations.is_empty() || !item.structural_relations.is_empty();
    let has_time = item.location.time_start_ms.is_some() && item.location.time_end_ms.is_some();
    let valid = match kind {
        SourceMediaKind::Text => has_text,
        SourceMediaKind::Image => has_text || has_visual,
        SourceMediaKind::Audio => has_text && has_time,
        // A video must carry VISUAL observations and be placeable in time. It must
        // not additionally be required to carry text: a soundless demonstration
        // with nothing written on screen — someone showing a technique with their
        // hands — is legitimate evidence, and requiring text made it impossible to
        // represent at all.
        //
        // The rule this replaces demanded text as well, which was never what the
        // rule was for: `video_cannot_collapse_to_audio_only_transcript` clears
        // visual_observations, so what it guards is the visual half. Audio-only
        // collapse is still refused, because audio-only carries no visual.
        SourceMediaKind::Video => has_time && has_visual,
        SourceMediaKind::Document => (has_text || has_visual) && item.location.page_start.is_some(),
        SourceMediaKind::Mixed => false,
    };
    if valid {
        Ok(())
    } else {
        Err(EvidenceError::MissingModalityEvidence)
    }
}

pub(crate) fn validate_bundle(bundle: &EvidenceBundle) -> Result<(), EvidenceError> {
    if bundle.bundle_id.trim().is_empty() || bundle.sources.is_empty() || bundle.evidence.is_empty()
    {
        return Err(EvidenceError::InvalidBundle);
    }
    let sources = bundle
        .sources
        .iter()
        .map(|source| (source.source_id.as_str(), source))
        .collect::<HashMap<_, _>>();
    if sources.len() != bundle.sources.len()
        || bundle
            .sources
            .iter()
            .any(|source| validate_source(source).is_err() || !source.authorized)
    {
        return Err(EvidenceError::InvalidBundle);
    }
    for evidence in &bundle.evidence {
        let Some(source) = sources.get(evidence.source_id.as_str()) else {
            return Err(EvidenceError::InvalidBundle);
        };
        if evidence.source_digest != source.source_digest
            || evidence.media_kind != source.media_kind
            || evidence.correlation_group != source.correlation_group
            || evidence.provenance != source.opaque_reference
        {
            return Err(EvidenceError::InvalidBundle);
        }
    }
    let distinct_media = bundle
        .sources
        .iter()
        .map(|source| source.media_kind)
        .collect::<HashSet<_>>();
    let expected = if distinct_media.len() > 1 {
        SourceMediaKind::Mixed
    } else {
        *distinct_media
            .iter()
            .next()
            .ok_or(EvidenceError::InvalidBundle)?
    };
    if bundle.media_kind != expected {
        return Err(EvidenceError::InvalidBundle);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaimEvidenceAssessment {
    pub supporting_evidence_ids: Vec<String>,
    pub contradictory_evidence_ids: Vec<String>,
    pub independent_support_groups: usize,
    pub confidence: f32,
    pub contradiction_preserved: bool,
}

pub(crate) fn assess_claim(evidence: &[DistillationEvidence]) -> ClaimEvidenceAssessment {
    let mut support_groups = HashMap::<&str, (&DistillationEvidence, f32)>::new();
    let mut supporting = Vec::new();
    let mut contradictory = Vec::new();
    for item in evidence {
        if item.assertion == EvidenceAssertion::Contradictory {
            contradictory.push(item.evidence_id.clone());
        } else {
            supporting.push(item.evidence_id.clone());
            support_groups
                .entry(&item.correlation_group)
                .and_modify(|best| {
                    if item.confidence > best.1 {
                        *best = (item, item.confidence);
                    }
                })
                .or_insert((item, item.confidence));
        }
    }
    let confidence = support_groups
        .values()
        .fold(0.0_f32, |combined, (_, next)| {
            1.0 - (1.0 - combined) * (1.0 - *next)
        })
        .min(0.99);
    ClaimEvidenceAssessment {
        supporting_evidence_ids: supporting,
        contradictory_evidence_ids: contradictory.clone(),
        independent_support_groups: support_groups.len(),
        confidence,
        contradiction_preserved: !contradictory.is_empty(),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn source(
        id: &str,
        media_kind: SourceMediaKind,
        source_kind: SourceKind,
        private: bool,
    ) -> SourceArtifact {
        SourceArtifact {
            source_id: id.to_owned(),
            media_kind,
            source_kind,
            opaque_reference: format!("artifact://{id}"),
            source_digest: "a".repeat(64),
            correlation_group: id.to_owned(),
            authorized: true,
            private,
        }
    }

    pub(crate) fn item(id: &str, media_kind: SourceMediaKind) -> ExtractedEvidenceItem {
        let timed = matches!(media_kind, SourceMediaKind::Audio | SourceMediaKind::Video);
        ExtractedEvidenceItem {
            evidence_id: id.to_owned(),
            location: EvidenceLocation {
                page_start: (media_kind == SourceMediaKind::Document).then_some(1),
                page_end: (media_kind == SourceMediaKind::Document).then_some(1),
                time_start_ms: timed.then_some(1_000),
                time_end_ms: timed.then_some(2_000),
                region: None,
            },
            speaker: timed.then(|| "speaker-1".to_owned()),
            extracted_text: Some("bounded observation".to_owned()),
            visual_observations: matches!(
                media_kind,
                SourceMediaKind::Image | SourceMediaKind::Video
            )
            .then(|| "visible diagram".to_owned())
            .into_iter()
            .collect(),
            structural_relations: Vec::new(),
            contextual_observations: Vec::new(),
            assertion: EvidenceAssertion::Confirmed,
            confidence: 0.7,
        }
    }

    pub(crate) fn extraction(item: ExtractedEvidenceItem) -> EvidenceExtraction {
        EvidenceExtraction {
            extractor_identity: "test-extractor".to_owned(),
            extractor_revision: "capability-v1".to_owned(),
            asserted_sensitive_traits: Vec::new(),
            items: vec![item],
        }
    }

    pub(crate) fn local_policy() -> ExtractionPolicy {
        ExtractionPolicy {
            target: ExtractionTarget::Local,
            cloud_authorized: false,
            public_research_authorized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{test_support::*, *};

    fn normalize(kind: SourceMediaKind) -> DistillationEvidence {
        let source = source("source", kind, SourceKind::UserFile, false);
        normalize_extraction(
            SubjectKind::SelfProfile,
            &source,
            &local_policy(),
            extraction(item("evidence", kind)),
        )
        .unwrap()
        .remove(0)
    }

    #[test]
    fn text_image_audio_and_video_ingestion_preserve_modality_context() {
        assert_eq!(
            normalize(SourceMediaKind::Text).media_kind,
            SourceMediaKind::Text
        );
        assert!(!normalize(SourceMediaKind::Image)
            .visual_observations
            .is_empty());
        assert_eq!(
            normalize(SourceMediaKind::Audio).location.time_start_ms,
            Some(1_000)
        );
        let video = normalize(SourceMediaKind::Video);
        assert_eq!(video.location.time_end_ms, Some(2_000));
        assert!(!video.visual_observations.is_empty());
    }

    #[test]
    fn document_preserves_page_provenance() {
        let document = normalize(SourceMediaKind::Document);
        assert_eq!(document.location.page_start, Some(1));
        assert_eq!(document.provenance, "artifact://source");
    }

    #[test]
    fn video_cannot_collapse_to_audio_only_transcript() {
        let source = source(
            "video",
            SourceMediaKind::Video,
            SourceKind::Interview,
            false,
        );
        let mut item = item("evidence", SourceMediaKind::Video);
        item.visual_observations.clear();
        assert_eq!(
            normalize_extraction(
                SubjectKind::PublicPerson,
                &source,
                &local_policy(),
                extraction(item)
            )
            .unwrap_err(),
            EvidenceError::MissingModalityEvidence
        );
    }

    /// A soundless demonstration with nothing written on screen — hands showing a
    /// technique — is legitimate evidence. Requiring text as well as visuals made
    /// it impossible to represent, which is a gap in the contract rather than a
    /// property of the video.
    #[test]
    fn a_soundless_video_with_no_text_is_still_admissible_when_it_carries_visuals() {
        let source = source(
            "silent-demonstration",
            SourceMediaKind::Video,
            SourceKind::UserFile,
            true,
        );
        let mut item = item("evidence", SourceMediaKind::Video);
        item.extracted_text = None;
        item.visual_observations = vec!["hands fold the dough toward the centre".to_owned()];

        let evidence = normalize_extraction(
            SubjectKind::PrivatePerson,
            &source,
            &local_policy(),
            extraction(item),
        )
        .unwrap();
        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].extracted_text.is_none());
        assert!(!evidence[0].visual_observations.is_empty());
    }

    #[test]
    fn mixed_bundle_requires_multiple_modalities_and_preserves_each_source() {
        let text = source("text", SourceMediaKind::Text, SourceKind::Email, true);
        let image = source("image", SourceMediaKind::Image, SourceKind::UserFile, true);
        let mut evidence = normalize_extraction(
            SubjectKind::PrivatePerson,
            &text,
            &local_policy(),
            extraction(item("e1", SourceMediaKind::Text)),
        )
        .unwrap();
        evidence.extend(
            normalize_extraction(
                SubjectKind::PrivatePerson,
                &image,
                &local_policy(),
                extraction(item("e2", SourceMediaKind::Image)),
            )
            .unwrap(),
        );
        let bundle = EvidenceBundle {
            bundle_id: "mixed".to_owned(),
            subject_kind: SubjectKind::PrivatePerson,
            media_kind: SourceMediaKind::Mixed,
            sources: vec![text, image],
            evidence,
        };
        validate_bundle(&bundle).unwrap();
        assert_eq!(bundle.evidence[1].source_id, "image");
    }

    #[test]
    fn independent_modalities_reinforce_confidence_but_duplicates_do_not() {
        let mut text = normalize(SourceMediaKind::Text);
        text.correlation_group = "origin-a".to_owned();
        let mut duplicate = text.clone();
        duplicate.evidence_id = "duplicate".to_owned();
        duplicate.confidence = 0.6;
        let duplicate_assessment = assess_claim(&[text.clone(), duplicate]);
        assert_eq!(duplicate_assessment.independent_support_groups, 1);
        assert!((duplicate_assessment.confidence - 0.7).abs() < f32::EPSILON);

        let mut video = normalize(SourceMediaKind::Video);
        video.correlation_group = "origin-b".to_owned();
        let reinforced = assess_claim(&[text, video]);
        assert_eq!(reinforced.independent_support_groups, 2);
        assert!(reinforced.confidence > 0.7);
    }

    #[test]
    fn cross_modal_contradiction_is_preserved() {
        let support = normalize(SourceMediaKind::Text);
        let mut contradiction = normalize(SourceMediaKind::Video);
        contradiction.assertion = EvidenceAssertion::Contradictory;
        contradiction.evidence_id = "opposing-video".to_owned();
        let assessment = assess_claim(&[support, contradiction]);
        assert!(assessment.contradiction_preserved);
        assert_eq!(assessment.contradictory_evidence_ids, ["opposing-video"]);
    }

    #[test]
    fn cloud_is_rejected_before_evidence_normalization_without_authorization() {
        let source = source(
            "private-audio",
            SourceMediaKind::Audio,
            SourceKind::Chat,
            true,
        );
        let policy = ExtractionPolicy {
            target: ExtractionTarget::Cloud,
            cloud_authorized: false,
            public_research_authorized: false,
        };
        assert_eq!(
            normalize_extraction(
                SubjectKind::PrivatePerson,
                &source,
                &policy,
                extraction(item("e1", SourceMediaKind::Audio))
            )
            .unwrap_err(),
            EvidenceError::CloudAuthorizationRequired
        );
    }

    #[test]
    fn creator_receives_only_authorized_bounded_evidence() {
        let mut source = source(
            "unauthorized",
            SourceMediaKind::Text,
            SourceKind::Chat,
            true,
        );
        source.authorized = false;
        assert_eq!(
            normalize_extraction(
                SubjectKind::PrivatePerson,
                &source,
                &local_policy(),
                extraction(item("e1", SourceMediaKind::Text))
            )
            .unwrap_err(),
            EvidenceError::UnauthorizedSource
        );
    }

    #[test]
    fn sensitive_trait_inference_is_rejected_for_every_modality() {
        let source = source("image", SourceMediaKind::Image, SourceKind::UserFile, true);
        let mut extraction = extraction(item("e1", SourceMediaKind::Image));
        extraction
            .asserted_sensitive_traits
            .push("health-condition".to_owned());
        assert_eq!(
            normalize_extraction(
                SubjectKind::PrivatePerson,
                &source,
                &local_policy(),
                extraction
            )
            .unwrap_err(),
            EvidenceError::SensitiveInferenceForbidden
        );
    }
}
