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

const MAX_NUWA_METHOD_BYTES: u64 = 512 * 1024;
const MAX_NUWA_COMPLETION_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NuwaExecutionResult {
    /// A successful executor run is still quarantined candidate material.
    pub status: String,
    pub quarantine_root: String,
    pub artifact: String,
    pub candidate_count: usize,
    pub result: NuwaResult,
}

/// Production Nuwa execution.
///
/// AI-OS, not the Agent:
/// - resolves the audited Nuwa installation;
/// - reads the methodology files;
/// - supplies the EvidenceBundle;
/// - validates every evidence reference;
/// - writes the only accepted Nuwa artifact.
///
/// The Nuwa OpenClaw identity has zero tools. It receives everything needed
/// inside one bounded prompt and cannot independently acquire evidence.
pub(crate) fn execute_nuwa(
    bundle: &EvidenceBundle,
    operation_id: &str,
) -> Result<NuwaExecutionResult, String> {
    let skill_root = super::adapters::resolve_nuwa_directory()
        .ok_or_else(|| "No usable Nuwa installation was found.".to_owned())?;

    execute_nuwa_with(bundle, operation_id, &skill_root, |session_id, prompt| {
        crate::runtime::agent_skill_transport::invoke_nuwa_skill(session_id, prompt)
    })
}

fn execute_nuwa_with(
    bundle: &EvidenceBundle,
    operation_id: &str,
    skill_root: &std::path::Path,
    invoke: impl Fn(&str, &str) -> Result<String, String>,
) -> Result<NuwaExecutionResult, String> {
    super::evidence::validate_bundle(bundle).map_err(|error| error.to_string())?;

    if operation_id.trim().is_empty() {
        return Err("Nuwa execution requires an operation identifier.".to_owned());
    }

    let methodology = load_nuwa_methodology(skill_root)?;
    let evidence_json = serde_json::to_string(bundle)
        .map_err(|_| "Could not serialize bounded Nuwa evidence.".to_owned())?;

    let prompt = nuwa_prompt(&methodology, &evidence_json);
    let completion = invoke(operation_id, &prompt)?;

    if completion.len() > MAX_NUWA_COMPLETION_BYTES {
        return Err("Nuwa completion exceeds the bounded result limit.".to_owned());
    }

    // Accept either a bare JSON object or exactly one non-semantic JSON
    // Markdown fence. Commentary, prose prefixes/suffixes, arbitrary Markdown
    // and multiple JSON values remain rejected.
    //
    // This normalizes only the transport envelope. NuwaResult schema and
    // AI-OS evidence validation remain authoritative.
    let normalized = normalize_nuwa_json_envelope(&completion)?;

    let result: NuwaResult = serde_json::from_str(normalized)
        .map_err(|_| "Nuwa did not return the strict AI-OS JSON contract.".to_owned())?;

    validate_nuwa_result(&result, bundle).map_err(|error| error.to_string())?;

    // Only after independent validation does AI-OS create an artifact.
    let quarantine = tempfile::Builder::new()
        .prefix("ai-os-nuwa-quarantine-")
        .tempdir()
        .map_err(|_| "Could not create the Nuwa quarantine.".to_owned())?;

    let artifact_name = "nuwa-result.json";
    let artifact_path = quarantine.path().join(artifact_name);

    let encoded = serde_json::to_vec_pretty(&result)
        .map_err(|_| "Validated Nuwa result could not be serialized.".to_owned())?;

    std::fs::write(&artifact_path, encoded)
        .map_err(|_| "Validated Nuwa result could not be written to quarantine.".to_owned())?;

    let persisted_root = quarantine.keep();

    Ok(NuwaExecutionResult {
        status: "quarantined".to_owned(),
        quarantine_root: persisted_root.to_string_lossy().into_owned(),
        artifact: artifact_name.to_owned(),
        candidate_count: result.candidate_count(),
        result,
    })
}

fn normalize_nuwa_json_envelope(completion: &str) -> Result<&str, String> {
    const FENCE: &str = "\u{60}\u{60}\u{60}";
    const JSON_FENCE: &str = "\u{60}\u{60}\u{60}json";

    let trimmed = completion.trim();

    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed);
    }

    let Some(after_open) = trimmed.strip_prefix(JSON_FENCE) else {
        return Err("Nuwa did not return the strict AI-OS JSON contract.".to_owned());
    };

    let Some(inner) = after_open.strip_suffix(FENCE) else {
        return Err("Nuwa did not return the strict AI-OS JSON contract.".to_owned());
    };

    let inner = inner.trim();

    if !inner.starts_with('{') || !inner.ends_with('}') {
        return Err("Nuwa did not return the strict AI-OS JSON contract.".to_owned());
    }

    Ok(inner)
}

#[derive(Debug)]
struct NuwaMethodology {
    skill: String,
    extraction_framework: String,
    fidelity_scorecard: String,
}

fn load_nuwa_methodology(skill_root: &std::path::Path) -> Result<NuwaMethodology, String> {
    Ok(NuwaMethodology {
        skill: read_nuwa_method_file(skill_root, "SKILL.md")?,
        extraction_framework: read_nuwa_method_file(
            skill_root,
            "references/extraction-framework.md",
        )?,
        fidelity_scorecard: read_nuwa_method_file(skill_root, "references/fidelity-scorecard.md")?,
    })
}

fn read_nuwa_method_file(skill_root: &std::path::Path, relative: &str) -> Result<String, String> {
    let path = skill_root.join(relative);

    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| format!("Nuwa methodology file {relative} is missing."))?;

    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Nuwa methodology file {relative} is a symbolic link and was refused."
        ));
    }

    if !metadata.is_file() || metadata.len() > MAX_NUWA_METHOD_BYTES {
        return Err(format!(
            "Nuwa methodology file {relative} is not a bounded regular file."
        ));
    }

    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("Nuwa methodology file {relative} is not readable UTF-8."))?;

    if text.trim().is_empty() {
        return Err(format!("Nuwa methodology file {relative} is empty."));
    }

    Ok(text)
}

fn nuwa_prompt(methodology: &NuwaMethodology, evidence_json: &str) -> String {
    format!(
        r#"You are the zero-tool Nuwa cognitive-analysis turn controlled by AI-OS.

You have NO authority to:
- search the web
- obtain additional evidence
- call tools
- execute shell commands
- read arbitrary files
- create or activate a Person Profile
- invent evidence identifiers
- change AI-OS evidence classifications

Your only job is to apply the audited Nuwa methodology supplied below to the
AI-OS EvidenceBundle supplied below.

Every cognitive candidate MUST cite one or more evidenceIds or
contradictoryEvidenceIds that already exist in the EvidenceBundle.

A contradictoryEvidenceId may be used only when AI-OS already classifies that
evidence item as contradictory.

IMPORTANT EVIDENCE-SUFFICIENCY RULE:
If the supplied evidence is insufficient to support a person-specific cognitive
claim, DO NOT create a generic principle, Nuwa-methodology principle, best
practice, requirement-diagnostic rule, or plausible personality trait.
Instead, leave the relevant candidate arrays empty and explain the limitation
only in honestLimits.

If the evidence is insufficient for all person-specific cognitive claims, then
ALL FIVE candidate arrays MUST be [] and only honestLimits may contain entries.

Preserve uncertainty and contradictions. Do not infer sensitive attributes.
Do not claim knowledge outside the supplied evidence.

Return ONLY one UTF-8 JSON object.
No Markdown fence.
No explanation before or after the JSON.

STRICT ITEM TYPE RULE:
Every item in each of these five arrays MUST be an object with exactly:
- statement: string
- confidence: number from 0.0 through 1.0
- evidenceIds: array of existing AI-OS evidence-id strings
- contradictoryEvidenceIds: array of existing contradictory evidence-id strings

NEVER put a bare string inside:
- mentalModels
- decisionHeuristics
- valuePriorities
- cognitiveTensions
- communicationPatterns

The JSON object must have exactly these camelCase fields and shapes:

{{
  "mentalModels": [
    {{
      "statement": "person-specific evidence-grounded statement",
      "confidence": 0.0,
      "evidenceIds": ["existing-id"],
      "contradictoryEvidenceIds": []
    }}
  ],
  "decisionHeuristics": [
    {{
      "statement": "person-specific evidence-grounded statement",
      "confidence": 0.0,
      "evidenceIds": ["existing-id"],
      "contradictoryEvidenceIds": []
    }}
  ],
  "valuePriorities": [
    {{
      "statement": "person-specific evidence-grounded statement",
      "confidence": 0.0,
      "evidenceIds": ["existing-id"],
      "contradictoryEvidenceIds": []
    }}
  ],
  "cognitiveTensions": [
    {{
      "statement": "person-specific evidence-grounded statement",
      "confidence": 0.0,
      "evidenceIds": ["existing-id"],
      "contradictoryEvidenceIds": []
    }}
  ],
  "communicationPatterns": [
    {{
      "statement": "person-specific evidence-grounded statement",
      "confidence": 0.0,
      "evidenceIds": ["existing-id"],
      "contradictoryEvidenceIds": []
    }}
  ],
  "honestLimits": ["evidence boundary or uncertainty only"]
}}

The example objects above describe TYPE SHAPE only.
Do not copy their statements.
Empty arrays are preferred over unsupported claims.

NUWA_SKILL_START
{skill}
NUWA_SKILL_END

NUWA_EXTRACTION_FRAMEWORK_START
{framework}
NUWA_EXTRACTION_FRAMEWORK_END

NUWA_FIDELITY_SCORECARD_START
{scorecard}
NUWA_FIDELITY_SCORECARD_END

AI_OS_EVIDENCE_BUNDLE_START
{evidence}
AI_OS_EVIDENCE_BUNDLE_END
"#,
        skill = methodology.skill,
        framework = methodology.extraction_framework,
        scorecard = methodology.fidelity_scorecard,
        evidence = evidence_json,
    )
}

#[cfg(test)]
mod executor_tests {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*},
        SourceKind, SourceMediaKind, SubjectKind,
    };

    fn methodology() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("references")).unwrap();

        std::fs::write(
            root.path().join("SKILL.md"),
            "name: huashu-nuwa\nUse local corpus only.\n",
        )
        .unwrap();

        std::fs::write(
            root.path().join("references/extraction-framework.md"),
            "跨域复现\n有生成力\n有排他性\n",
        )
        .unwrap();

        std::fs::write(
            root.path().join("references/fidelity-scorecard.md"),
            "Preserve uncertainty and contradictions.\n",
        )
        .unwrap();

        root
    }

    fn evidence_bundle() -> EvidenceBundle {
        let source = source(
            "private-chat",
            SourceMediaKind::Text,
            SourceKind::Chat,
            true,
        );

        let evidence = normalize_extraction(
            SubjectKind::PrivatePerson,
            &source,
            &local_policy(),
            extraction(item("evidence-1", SourceMediaKind::Text)),
        )
        .unwrap();

        EvidenceBundle {
            bundle_id: "nuwa-executor-bundle".to_owned(),
            subject_kind: SubjectKind::PrivatePerson,
            media_kind: SourceMediaKind::Text,
            sources: vec![source],
            evidence,
        }
    }

    fn valid_completion() -> String {
        serde_json::json!({
            "mentalModels": [{
                "statement": "Compares alternatives before committing.",
                "confidence": 0.75,
                "evidenceIds": ["evidence-1"],
                "contradictoryEvidenceIds": []
            }],
            "decisionHeuristics": [],
            "valuePriorities": [],
            "cognitiveTensions": [],
            "communicationPatterns": [],
            "honestLimits": [
                "The evidence covers only the supplied conversation context."
            ]
        })
        .to_string()
    }

    #[test]
    fn bounded_json_fence_is_accepted_without_accepting_prose() {
        let bare = r#"{"mentalModels":[]}"#;

        assert_eq!(normalize_nuwa_json_envelope(bare).unwrap(), bare);

        let fence = "\u{60}\u{60}\u{60}";
        let fenced = format!("{fence}json\n{{\"mentalModels\":[]}}\n{fence}");

        assert_eq!(
            normalize_nuwa_json_envelope(&fenced).unwrap(),
            r#"{"mentalModels":[]}"#
        );

        let prose_prefix =
            format!("Here is the result:\n{fence}json\n{{\"mentalModels\":[]}}\n{fence}");

        assert!(normalize_nuwa_json_envelope(&prose_prefix).is_err());

        let prose_suffix = format!("{fence}json\n{{\"mentalModels\":[]}}\n{fence}\nExplanation");

        assert!(normalize_nuwa_json_envelope(&prose_suffix).is_err());

        let unlabeled = format!("{fence}\n{{\"mentalModels\":[]}}\n{fence}");

        assert!(normalize_nuwa_json_envelope(&unlabeled).is_err());

        let two_objects =
            format!("{fence}json\n{{\"mentalModels\":[]}}{{\"mentalModels\":[]}}\n{fence}");

        let normalized = normalize_nuwa_json_envelope(&two_objects).unwrap();

        assert!(serde_json::from_str::<NuwaResult>(normalized).is_err());
    }

    #[test]
    fn nuwa_prompt_requires_object_items_and_honest_limits_for_insufficient_evidence() {
        let methodology = NuwaMethodology {
            skill: "skill".to_owned(),
            extraction_framework: "framework".to_owned(),
            fidelity_scorecard: "scorecard".to_owned(),
        };

        let prompt = nuwa_prompt(&methodology, r#"{"bundleId":"bundle"}"#);

        assert!(prompt.contains("ALL FIVE candidate arrays MUST be []"));
        assert!(prompt.contains("NEVER put a bare string inside"));
        assert!(prompt.contains("Empty arrays are preferred over unsupported claims."));
        assert!(prompt.contains("person-specific evidence-grounded statement"));
        assert!(prompt.contains("honestLimits"));
    }

    #[test]
    fn validated_nuwa_output_is_written_by_ai_os_into_quarantine() {
        let method = methodology();
        let bundle = evidence_bundle();

        let result = execute_nuwa_with(
            &bundle,
            "operation-1",
            method.path(),
            |_session_id, prompt| {
                assert!(prompt.contains("AI_OS_EVIDENCE_BUNDLE_START"));
                assert!(prompt.contains("evidence-1"));
                assert!(prompt.contains("NO authority"));
                Ok(valid_completion())
            },
        )
        .unwrap();

        assert_eq!(result.status, "quarantined");
        assert_eq!(result.candidate_count, 1);
        assert_eq!(result.artifact, "nuwa-result.json");

        let artifact = std::path::Path::new(&result.quarantine_root).join(&result.artifact);

        assert!(artifact.is_file());

        let persisted: NuwaResult =
            serde_json::from_slice(&std::fs::read(artifact).unwrap()).unwrap();

        assert_eq!(persisted, result.result);
    }

    #[test]
    fn agent_claiming_fabricated_evidence_never_creates_a_valid_artifact() {
        let method = methodology();
        let bundle = evidence_bundle();

        let fabricated = serde_json::json!({
            "mentalModels": [{
                "statement": "Invented conclusion.",
                "confidence": 0.99,
                "evidenceIds": ["fabricated-by-agent"],
                "contradictoryEvidenceIds": []
            }],
            "decisionHeuristics": [],
            "valuePriorities": [],
            "cognitiveTensions": [],
            "communicationPatterns": [],
            "honestLimits": []
        })
        .to_string();

        let error = execute_nuwa_with(
            &bundle,
            "operation-2",
            method.path(),
            |_session_id, _prompt| Ok(fabricated.clone()),
        )
        .unwrap_err();

        assert!(error.contains("fabricated-by-agent"));
    }

    /// Real integration smoke test.
    ///
    /// This is ignored during normal CI because it requires the local OpenClaw
    /// gateway and the configured zero-tool `ai-os-cognitive-distillation`
    /// identity. It deliberately uses a PrivatePerson EvidenceBundle and accepts
    /// only an independently validated Nuwa quarantine artifact.
    #[test]
    #[ignore = "requires local OpenClaw and the zero-tool Nuwa execution identity"]
    fn real_zero_tool_nuwa_smoke_test() {
        let bundle = evidence_bundle();

        let operation_id = format!("nuwa-real-smoke-{}", std::process::id());

        println!("NUWA_SMOKE_OPERATION_ID={operation_id}");

        let result = execute_nuwa(&bundle, &operation_id)
            .expect("real zero-tool Nuwa execution must succeed");

        println!("NUWA_SMOKE_STATUS={}", result.status);
        println!("NUWA_SMOKE_CANDIDATES={}", result.candidate_count);
        println!("NUWA_SMOKE_ARTIFACT={}", result.artifact);

        assert_eq!(result.status, "quarantined");
        assert_eq!(result.artifact, "nuwa-result.json");

        let artifact = std::path::Path::new(&result.quarantine_root).join(&result.artifact);

        assert!(
            artifact.is_file(),
            "AI-OS must persist the validated Nuwa artifact"
        );

        let persisted: NuwaResult = serde_json::from_slice(
            &std::fs::read(&artifact).expect("Nuwa quarantine artifact must be readable"),
        )
        .expect("Nuwa quarantine artifact must satisfy the strict contract");

        validate_nuwa_result(&persisted, &bundle)
            .expect("persisted Nuwa result must still validate against AI-OS evidence");

        assert_eq!(persisted, result.result);
    }

    #[test]
    fn prose_or_markdown_instead_of_the_contract_is_rejected() {
        let method = methodology();
        let bundle = evidence_bundle();

        let error = execute_nuwa_with(
            &bundle,
            "operation-3",
            method.path(),
            |_session_id, _prompt| {
                Ok(format!(
                    "Here is the result:\n```json\n{}\n```",
                    valid_completion()
                ))
            },
        )
        .unwrap_err();

        assert_eq!(error, "Nuwa did not return the strict AI-OS JSON contract.");
    }
}
