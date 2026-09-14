use super::evidence::{EvidenceAssertion, EvidenceBundle};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_NUWA_STATEMENT_BYTES: usize = 16 * 1024;
const MAX_NUWA_LIMIT_BYTES: usize = 8 * 1024;
const MAX_ITEMS_PER_CATEGORY: usize = 64;
const MAX_HONEST_LIMITS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NuwaClaimCandidate {
    pub statement: String,
    pub confidence: f32,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub contradictory_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NuwaResult {
    #[serde(default)]
    pub mental_models: Vec<NuwaClaimCandidate>,
    #[serde(default)]
    pub decision_heuristics: Vec<NuwaClaimCandidate>,
    #[serde(default)]
    pub value_priorities: Vec<NuwaClaimCandidate>,
    #[serde(default)]
    pub cognitive_tensions: Vec<NuwaClaimCandidate>,
    #[serde(default)]
    pub communication_patterns: Vec<NuwaClaimCandidate>,

    /// Nuwa may explain where the evidence is insufficient or where the model
    /// should not be trusted. These are analysis boundaries, not CognitiveClaims,
    /// and therefore are never promoted into the canonical profile automatically.
    #[serde(default)]
    pub honest_limits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NuwaValidationError {
    EmptyResult,
    TooManyCandidates,
    InvalidStatement,
    InvalidConfidence,
    MissingEvidence,
    UnknownEvidenceId(String),
    InvalidContradiction(String),
    DuplicateEvidenceReference(String),
    InvalidHonestLimit,
}

impl std::fmt::Display for NuwaValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyResult => formatter.write_str(
                "Nuwa returned no cognitive candidates or honest limits.",
            ),
            Self::TooManyCandidates => formatter.write_str(
                "Nuwa returned more cognitive candidates than the bounded contract allows.",
            ),
            Self::InvalidStatement => formatter.write_str(
                "A Nuwa cognitive candidate has an empty or oversized statement.",
            ),
            Self::InvalidConfidence => formatter.write_str(
                "A Nuwa cognitive candidate confidence must be between 0 and 1.",
            ),
            Self::MissingEvidence => formatter.write_str(
                "Every Nuwa cognitive candidate must reference AI-OS evidence.",
            ),
            Self::UnknownEvidenceId(id) => write!(
                formatter,
                "Nuwa referenced evidence id {id} that is not present in the AI-OS EvidenceBundle."
            ),
            Self::InvalidContradiction(id) => write!(
                formatter,
                "Nuwa marked evidence id {id} as contradictory although AI-OS did not classify it as contradictory."
            ),
            Self::DuplicateEvidenceReference(id) => write!(
                formatter,
                "Nuwa referenced evidence id {id} as both supporting and contradictory evidence for the same candidate."
            ),
            Self::InvalidHonestLimit => formatter.write_str(
                "A Nuwa honest-limit entry is empty, oversized, or exceeds the bounded count.",
            ),
        }
    }
}

impl std::error::Error for NuwaValidationError {}

impl NuwaResult {
    pub(crate) fn candidate_count(&self) -> usize {
        self.mental_models.len()
            + self.decision_heuristics.len()
            + self.value_priorities.len()
            + self.cognitive_tensions.len()
            + self.communication_patterns.len()
    }

    fn candidates(&self) -> impl Iterator<Item = &NuwaClaimCandidate> {
        self.mental_models
            .iter()
            .chain(self.decision_heuristics.iter())
            .chain(self.value_priorities.iter())
            .chain(self.cognitive_tensions.iter())
            .chain(self.communication_patterns.iter())
    }
}

/// Validate Nuwa output against facts AI-OS independently owns.
///
/// Passing this function means only:
/// - the result obeys the bounded Nuwa schema;
/// - every evidence reference exists in THIS EvidenceBundle;
/// - every claimed contradiction agrees with AI-OS evidence classification.
///
/// It does NOT mean Nuwa is correct, reviewed, canonical or authorised to
/// activate a PersonDistillationProfile.
pub(crate) fn validate_nuwa_result(
    result: &NuwaResult,
    bundle: &EvidenceBundle,
) -> Result<(), NuwaValidationError> {
    let total = result.candidate_count();

    if total == 0 && result.honest_limits.is_empty() {
        return Err(NuwaValidationError::EmptyResult);
    }

    if [
        result.mental_models.len(),
        result.decision_heuristics.len(),
        result.value_priorities.len(),
        result.cognitive_tensions.len(),
        result.communication_patterns.len(),
    ]
    .into_iter()
    .any(|count| count > MAX_ITEMS_PER_CATEGORY)
    {
        return Err(NuwaValidationError::TooManyCandidates);
    }

    if result.honest_limits.len() > MAX_HONEST_LIMITS
        || result
            .honest_limits
            .iter()
            .any(|limit| limit.trim().is_empty() || limit.len() > MAX_NUWA_LIMIT_BYTES)
    {
        return Err(NuwaValidationError::InvalidHonestLimit);
    }

    let evidence: HashMap<&str, EvidenceAssertion> = bundle
        .evidence
        .iter()
        .map(|item| (item.evidence_id.as_str(), item.assertion.clone()))
        .collect();

    for candidate in result.candidates() {
        validate_candidate(candidate, &evidence)?;
    }

    Ok(())
}

fn validate_candidate(
    candidate: &NuwaClaimCandidate,
    evidence: &HashMap<&str, EvidenceAssertion>,
) -> Result<(), NuwaValidationError> {
    if candidate.statement.trim().is_empty() || candidate.statement.len() > MAX_NUWA_STATEMENT_BYTES
    {
        return Err(NuwaValidationError::InvalidStatement);
    }

    if !candidate.confidence.is_finite() || !(0.0..=1.0).contains(&candidate.confidence) {
        return Err(NuwaValidationError::InvalidConfidence);
    }

    if candidate.evidence_ids.is_empty() && candidate.contradictory_evidence_ids.is_empty() {
        return Err(NuwaValidationError::MissingEvidence);
    }

    let supporting: HashSet<&str> = candidate.evidence_ids.iter().map(String::as_str).collect();
    let contradictory: HashSet<&str> = candidate
        .contradictory_evidence_ids
        .iter()
        .map(String::as_str)
        .collect();

    for id in supporting.intersection(&contradictory) {
        return Err(NuwaValidationError::DuplicateEvidenceReference(
            (*id).to_owned(),
        ));
    }

    for id in candidate
        .evidence_ids
        .iter()
        .chain(candidate.contradictory_evidence_ids.iter())
    {
        if !evidence.contains_key(id.as_str()) {
            return Err(NuwaValidationError::UnknownEvidenceId(id.clone()));
        }
    }

    for id in &candidate.contradictory_evidence_ids {
        if evidence.get(id.as_str()) != Some(&EvidenceAssertion::Contradictory) {
            return Err(NuwaValidationError::InvalidContradiction(id.clone()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*},
        SourceKind, SourceMediaKind, SubjectKind,
    };

    fn bundle() -> EvidenceBundle {
        let source = source("chat-source", SourceMediaKind::Text, SourceKind::Chat, true);

        let support = normalize_extraction(
            SubjectKind::PrivatePerson,
            &source,
            &local_policy(),
            extraction(item("support-1", SourceMediaKind::Text)),
        )
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

        let mut contradiction = normalize_extraction(
            SubjectKind::PrivatePerson,
            &source,
            &local_policy(),
            extraction(item("contradiction-1", SourceMediaKind::Text)),
        )
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

        contradiction.assertion = EvidenceAssertion::Contradictory;

        EvidenceBundle {
            bundle_id: "nuwa-test-bundle".to_owned(),
            subject_kind: SubjectKind::PrivatePerson,
            media_kind: SourceMediaKind::Text,
            sources: vec![source],
            evidence: vec![support, contradiction],
        }
    }

    fn candidate() -> NuwaClaimCandidate {
        NuwaClaimCandidate {
            statement: "Tests alternatives before committing to a decision.".to_owned(),
            confidence: 0.8,
            evidence_ids: vec!["support-1".to_owned()],
            contradictory_evidence_ids: vec!["contradiction-1".to_owned()],
        }
    }

    fn result(candidate: NuwaClaimCandidate) -> NuwaResult {
        NuwaResult {
            mental_models: vec![candidate],
            decision_heuristics: Vec::new(),
            value_priorities: Vec::new(),
            cognitive_tensions: Vec::new(),
            communication_patterns: Vec::new(),
            honest_limits: vec![
                "Evidence is insufficient to predict behaviour outside the observed context."
                    .to_owned(),
            ],
        }
    }

    #[test]
    fn valid_nuwa_candidates_reference_only_ai_os_evidence() {
        let bundle = bundle();
        let result = result(candidate());

        assert_eq!(validate_nuwa_result(&result, &bundle), Ok(()));
    }

    #[test]
    fn fabricated_evidence_id_is_rejected() {
        let bundle = bundle();
        let mut candidate = candidate();
        candidate.evidence_ids = vec!["nuwa-invented-this".to_owned()];

        assert_eq!(
            validate_nuwa_result(&result(candidate), &bundle),
            Err(NuwaValidationError::UnknownEvidenceId(
                "nuwa-invented-this".to_owned()
            ))
        );
    }

    #[test]
    fn model_cannot_label_supporting_evidence_as_a_contradiction() {
        let bundle = bundle();
        let mut candidate = candidate();
        candidate.evidence_ids.clear();
        candidate.contradictory_evidence_ids = vec!["support-1".to_owned()];

        assert_eq!(
            validate_nuwa_result(&result(candidate), &bundle),
            Err(NuwaValidationError::InvalidContradiction(
                "support-1".to_owned()
            ))
        );
    }

    #[test]
    fn candidate_without_any_provenance_is_rejected() {
        let bundle = bundle();
        let mut candidate = candidate();
        candidate.evidence_ids.clear();
        candidate.contradictory_evidence_ids.clear();

        assert_eq!(
            validate_nuwa_result(&result(candidate), &bundle),
            Err(NuwaValidationError::MissingEvidence)
        );
    }

    #[test]
    fn same_evidence_cannot_be_support_and_contradiction() {
        let bundle = bundle();
        let mut candidate = candidate();
        candidate.evidence_ids = vec!["contradiction-1".to_owned()];
        candidate.contradictory_evidence_ids = vec!["contradiction-1".to_owned()];

        assert_eq!(
            validate_nuwa_result(&result(candidate), &bundle),
            Err(NuwaValidationError::DuplicateEvidenceReference(
                "contradiction-1".to_owned()
            ))
        );
    }

    #[test]
    fn honest_limits_are_not_required_to_be_claims() {
        let bundle = bundle();

        let result = NuwaResult {
            mental_models: Vec::new(),
            decision_heuristics: Vec::new(),
            value_priorities: Vec::new(),
            cognitive_tensions: Vec::new(),
            communication_patterns: Vec::new(),
            honest_limits: vec![
                "The supplied evidence does not cover high-pressure decisions.".to_owned(),
            ],
        };

        assert_eq!(validate_nuwa_result(&result, &bundle), Ok(()));
    }
}
