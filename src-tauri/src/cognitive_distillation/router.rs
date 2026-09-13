use super::{
    adapters::{AdapterAvailability, AdapterCatalog, CreatorAdapterId},
    evidence::{validate_bundle, EvidenceBundle, EvidenceError},
    SourceKind, SubjectKind,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, error::Error, fmt};

const HIGH_VOLUME_SOURCE_THRESHOLD: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveDistillationRouteRequest {
    pub subject_kind: SubjectKind,
    pub evidence_bundle: EvidenceBundle,
    pub public_research_required: bool,
    pub article_heavy: bool,
    pub high_assurance_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PipelineRole {
    PrimaryCreator,
    ResearchEnrichment,
    CorpusEnrichment,
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
    pub public_research_permitted: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RouteError {
    Evidence(EvidenceError),
    SubjectMismatch,
    PrivateResearchForbidden,
    PrimaryCreatorUnavailable,
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evidence(error) => write!(formatter, "{error}"),
            Self::SubjectMismatch => {
                formatter.write_str("Evidence bundle subject does not match the route request.")
            }
            Self::PrivateResearchForbidden => formatter
                .write_str("Private-person distillation cannot use public research adapters."),
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
    let private = matches!(request.subject_kind, SubjectKind::PrivatePerson);
    if private && request.public_research_required {
        return Err(RouteError::PrivateResearchForbidden);
    }
    if catalog.availability(CreatorAdapterId::Distilly) != AdapterAvailability::Ready {
        return Err(RouteError::PrimaryCreatorUnavailable);
    }

    let mut pipelines = Vec::new();
    let mut skipped = Vec::new();
    let mut reasons = Vec::new();
    let source_kinds = request
        .evidence_bundle
        .sources
        .iter()
        .map(|source| source.source_kind)
        .collect::<HashSet<_>>();
    let public_research_permitted = matches!(
        request.subject_kind,
        SubjectKind::PublicPerson | SubjectKind::HistoricalPerson
    );

    if public_research_permitted && request.public_research_required {
        if catalog.availability(CreatorAdapterId::HumanDistill) == AdapterAvailability::Ready {
            pipelines.push(SelectedPipeline {
                adapter: CreatorAdapterId::HumanDistill,
                role: PipelineRole::ResearchEnrichment,
            });
        } else {
            skipped.push(CreatorAdapterId::HumanDistill);
            reasons.push("Public research enrichment is unavailable; existing authorized evidence remains valid.".to_owned());
        }
    }
    if request.article_heavy
        || source_kinds.contains(&SourceKind::Article)
        || source_kinds.contains(&SourceKind::Book)
    {
        if catalog.availability(CreatorAdapterId::DistillBlog) == AdapterAvailability::Ready {
            pipelines.push(SelectedPipeline {
                adapter: CreatorAdapterId::DistillBlog,
                role: PipelineRole::CorpusEnrichment,
            });
        } else {
            skipped.push(CreatorAdapterId::DistillBlog);
            reasons.push("Article specialization is not legally/technically ready; Distilly remains the valid creator.".to_owned());
        }
    }
    let evidence_enrichment_requested = request.high_assurance_evidence
        || request.evidence_bundle.sources.len() >= HIGH_VOLUME_SOURCE_THRESHOLD;
    if evidence_enrichment_requested {
        if catalog.availability(CreatorAdapterId::AnyoneStyle) == AdapterAvailability::Ready {
            pipelines.push(SelectedPipeline {
                adapter: CreatorAdapterId::AnyoneStyle,
                role: PipelineRole::CorpusEnrichment,
            });
        } else {
            skipped.push(CreatorAdapterId::AnyoneStyle);
        }
    }
    pipelines.push(SelectedPipeline {
        adapter: CreatorAdapterId::Distilly,
        role: PipelineRole::PrimaryCreator,
    });

    Ok(CognitiveDistillationRouteResult {
        pipelines,
        skipped_optional_adapters: skipped,
        source_count: request.evidence_bundle.sources.len(),
        canonical_profile_authority: "ai-os".to_owned(),
        active_profile_permitted: false,
        public_research_permitted,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*},
        SourceMediaKind,
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
    fn catalog(human: AdapterAvailability) -> AdapterCatalog {
        catalog_with_any(human, AdapterAvailability::ReferenceOnly)
    }
    fn catalog_with_any(human: AdapterAvailability, anyone: AdapterAvailability) -> AdapterCatalog {
        AdapterCatalog::for_test(vec![
            status(CreatorAdapterId::Distilly, AdapterAvailability::Ready),
            status(CreatorAdapterId::HumanDistill, human),
            status(CreatorAdapterId::AnyoneStyle, anyone),
            status(
                CreatorAdapterId::DistillBlog,
                AdapterAvailability::ReferenceOnly,
            ),
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
            public_research_required: false,
            article_heavy: false,
            high_assurance_evidence: false,
        }
    }

    #[test]
    fn router_selects_implementations_without_user_choice() {
        let route = route_distillation(
            &request(SubjectKind::SelfProfile, SourceKind::Chat),
            &catalog(AdapterAvailability::Unavailable),
        )
        .unwrap();
        assert_eq!(
            route.pipelines,
            [SelectedPipeline {
                adapter: CreatorAdapterId::Distilly,
                role: PipelineRole::PrimaryCreator
            }]
        );
        assert!(!route.active_profile_permitted);
    }

    #[test]
    fn private_media_never_triggers_public_research() {
        let mut request = request(SubjectKind::PrivatePerson, SourceKind::Chat);
        request.public_research_required = true;
        assert_eq!(
            route_distillation(&request, &catalog(AdapterAvailability::Ready)).unwrap_err(),
            RouteError::PrivateResearchForbidden
        );
    }

    #[test]
    fn unavailable_specialized_adapter_falls_back_to_valid_primary() {
        let mut request = request(SubjectKind::PublicPerson, SourceKind::PublicWeb);
        request.public_research_required = true;
        let route =
            route_distillation(&request, &catalog(AdapterAvailability::Unavailable)).unwrap();
        assert_eq!(
            route.pipelines.last().unwrap().adapter,
            CreatorAdapterId::Distilly
        );
        assert!(route
            .skipped_optional_adapters
            .contains(&CreatorAdapterId::HumanDistill));
    }

    #[test]
    fn ready_public_research_enriches_before_primary_creator() {
        let mut request = request(SubjectKind::HistoricalPerson, SourceKind::PublicWeb);
        request.public_research_required = true;
        let route = route_distillation(&request, &catalog(AdapterAvailability::Ready)).unwrap();
        assert_eq!(route.pipelines[0].role, PipelineRole::ResearchEnrichment);
        assert_eq!(route.pipelines[1].role, PipelineRole::PrimaryCreator);
    }

    #[test]
    fn reference_only_unlicensed_adapters_are_never_selected() {
        let mut request = request(SubjectKind::PublicPerson, SourceKind::Article);
        request.article_heavy = true;
        request.high_assurance_evidence = true;
        let route =
            route_distillation(&request, &catalog(AdapterAvailability::Unavailable)).unwrap();
        assert_eq!(route.pipelines.len(), 1);
        assert!(route
            .skipped_optional_adapters
            .contains(&CreatorAdapterId::AnyoneStyle));
        assert!(route
            .skipped_optional_adapters
            .contains(&CreatorAdapterId::DistillBlog));
    }

    #[test]
    fn source_volume_automatically_requests_evidence_enrichment() {
        let mut request = request(SubjectKind::SelfProfile, SourceKind::Chat);
        for index in 2..=HIGH_VOLUME_SOURCE_THRESHOLD {
            let source_id = format!("s{index}");
            let evidence_id = format!("e{index}");
            let source = source(&source_id, SourceMediaKind::Text, SourceKind::Chat, false);
            request.evidence_bundle.evidence.extend(
                normalize_extraction(
                    SubjectKind::SelfProfile,
                    &source,
                    &local_policy(),
                    extraction(item(&evidence_id, SourceMediaKind::Text)),
                )
                .unwrap(),
            );
            request.evidence_bundle.sources.push(source);
        }

        let route = route_distillation(
            &request,
            &catalog_with_any(AdapterAvailability::Unavailable, AdapterAvailability::Ready),
        )
        .unwrap();
        assert_eq!(route.source_count, HIGH_VOLUME_SOURCE_THRESHOLD);
        assert_eq!(route.pipelines[0].adapter, CreatorAdapterId::AnyoneStyle);
        assert_eq!(route.pipelines[1].adapter, CreatorAdapterId::Distilly);
    }
}
