use super::{
    adapters::{AdapterAvailability, AdapterCatalog, CreatorAdapterId},
    evidence::{validate_bundle, EvidenceBundle, EvidenceError},
    SubjectKind,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveDistillationRouteRequest {
    pub subject_kind: SubjectKind,
    pub evidence_bundle: EvidenceBundle,
    pub article_heavy: bool,
    pub high_assurance_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PipelineRole {
    PrimaryCreator,
    CognitiveAnalyzer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectedPipeline {
    pub adapter: CreatorAdapterId,
    pub role: PipelineRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveDistillationRouteResult {
    pub pipelines: Vec<SelectedPipeline>,
    pub skipped_optional_adapters: Vec<CreatorAdapterId>,
    pub source_count: usize,
    pub canonical_profile_authority: String,
    pub active_profile_permitted: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RouteError {
    Evidence(EvidenceError),
    SubjectMismatch,
    PrimaryCreatorUnavailable,
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evidence(error) => write!(formatter, "{error}"),
            Self::SubjectMismatch => {
                formatter.write_str("Evidence bundle subject does not match the route request.")
            }
            Self::PrimaryCreatorUnavailable => {
                formatter.write_str("No eligible primary persona creator is ready.")
            }
        }
    }
}
impl Error for RouteError {}

pub(crate) fn route_distillation(
    request: &CognitiveDistillationRouteRequest,
    catalog: &AdapterCatalog,
) -> Result<CognitiveDistillationRouteResult, RouteError> {
    validate_bundle(&request.evidence_bundle).map_err(RouteError::Evidence)?;
    if request.subject_kind != request.evidence_bundle.subject_kind {
        return Err(RouteError::SubjectMismatch);
    }
    if catalog.availability(CreatorAdapterId::Distilly) != AdapterAvailability::Ready {
        return Err(RouteError::PrimaryCreatorUnavailable);
    }

    let mut pipelines = vec![SelectedPipeline {
        adapter: CreatorAdapterId::Distilly,
        role: PipelineRole::PrimaryCreator,
    }];

    let mut skipped_optional_adapters = Vec::new();

    if catalog.availability(CreatorAdapterId::Nuwa) == AdapterAvailability::Ready {
        pipelines.push(SelectedPipeline {
            adapter: CreatorAdapterId::Nuwa,
            role: PipelineRole::CognitiveAnalyzer,
        });
    } else {
        skipped_optional_adapters.push(CreatorAdapterId::Nuwa);
    }

    Ok(CognitiveDistillationRouteResult {
        pipelines,
        skipped_optional_adapters,
        source_count: request.evidence_bundle.sources.len(),
        canonical_profile_authority: "ai-os".to_owned(),
        active_profile_permitted: false,
        reasons: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*},
        SourceKind, SourceMediaKind,
    };

    fn status(
        id: CreatorAdapterId,
        availability: AdapterAvailability,
    ) -> super::super::adapters::AdapterStatus {
        super::super::adapters::AdapterStatus {
            id,
            availability,
            capability_probe: "test".to_owned(),
            reason: None,
        }
    }
    fn catalog() -> AdapterCatalog {
        AdapterCatalog::for_test(vec![
            status(CreatorAdapterId::Distilly, AdapterAvailability::Ready),
            status(CreatorAdapterId::Nuwa, AdapterAvailability::Ready),
        ])
    }

    fn catalog_without_nuwa() -> AdapterCatalog {
        AdapterCatalog::for_test(vec![
            status(CreatorAdapterId::Distilly, AdapterAvailability::Ready),
            status(CreatorAdapterId::Nuwa, AdapterAvailability::Unavailable),
        ])
    }

    fn request(subject: SubjectKind, source_kind: SourceKind) -> CognitiveDistillationRouteRequest {
        let source = source(
            "s1",
            SourceMediaKind::Text,
            source_kind,
            subject == SubjectKind::PrivatePerson,
        );
        let evidence = normalize_extraction(
            subject,
            &source,
            &local_policy(),
            extraction(item("e1", SourceMediaKind::Text)),
        )
        .unwrap();
        CognitiveDistillationRouteRequest {
            subject_kind: subject,
            evidence_bundle: EvidenceBundle {
                bundle_id: "b1".to_owned(),
                subject_kind: subject,
                media_kind: SourceMediaKind::Text,
                sources: vec![source],
                evidence,
            },
            article_heavy: false,
            high_assurance_evidence: false,
        }
    }

    #[test]
    fn unavailable_nuwa_is_reported_as_optional_and_does_not_block_distilly() {
        let route = route_distillation(
            &request(SubjectKind::PrivatePerson, SourceKind::UserFile),
            &catalog_without_nuwa(),
        )
        .unwrap();

        assert_eq!(
            route.pipelines,
            [SelectedPipeline {
                adapter: CreatorAdapterId::Distilly,
                role: PipelineRole::PrimaryCreator
            }]
        );
        assert_eq!(route.skipped_optional_adapters, [CreatorAdapterId::Nuwa]);
        assert!(!route.active_profile_permitted);
    }

    #[test]
    fn router_selects_implementations_without_user_choice() {
        let route = route_distillation(
            &request(SubjectKind::SelfProfile, SourceKind::Chat),
            &catalog(),
        )
        .unwrap();
        assert_eq!(
            route.pipelines,
            [
                SelectedPipeline {
                    adapter: CreatorAdapterId::Distilly,
                    role: PipelineRole::PrimaryCreator
                },
                SelectedPipeline {
                    adapter: CreatorAdapterId::Nuwa,
                    role: PipelineRole::CognitiveAnalyzer
                }
            ]
        );
        assert!(route.skipped_optional_adapters.is_empty());
        assert!(!route.active_profile_permitted);
    }
}
