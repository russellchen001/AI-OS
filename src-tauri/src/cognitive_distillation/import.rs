//! Importing what Distilly wrote, without letting Distilly become the authority.
//!
//! Distilly is a creator adapter. It writes Markdown prose and two JSON artifacts
//! into a quarantine. This module turns that quarantine into typed AI-OS values
//! and then drafts a [`PersonDistillationProfile`] from it.
//!
//! Two boundaries are load-bearing here, and both exist because Distilly does not
//! know what AI-OS knows:
//!
//! 1. **Prose never becomes a claim.** A [`CognitiveClaim`] carries `evidence_ids`.
//!    Distilly emits narrative with no per-statement evidence links, so turning its
//!    prose into claims would mean inventing provenance. The prose is carried as
//!    draft narrative for a human to read, and claims are derived only from the
//!    evidence bundle, where every item already has a real `evidence_id`.
//!
//! 2. **Subject identity comes from AI-OS.** Distilly's `meta.json` carries a
//!    `source_context` block with `is_real_person`, but that is a property of the
//!    character preset it was invoked with, not of the evidence AI-OS supplied —
//!    a `colleague` create reports `is_real_person: true` even for evidence AI-OS
//!    classified as a fictional character. The bundle's `SubjectKind` is
//!    authoritative and `source_context` is deliberately never read.

use super::{
    evidence::{DistillationEvidence, EvidenceAssertion, EvidenceBundle},
    profile::{
        CognitiveClaim, DraftNarrativeSection, NarrativeOrigin, PersonDistillationProfile,
        ProfileStatus, RevisionRecord,
    },
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The character family the creator prompt pins. Distilly derives the artifact
/// identity from it, so the prompt and provenance validation must agree.
pub(crate) const DISTILLY_CHARACTER: &str = "colleague";

/// Upper bound on any single narrative artifact read out of the quarantine. A
/// creator that emits more than this is not producing a profile narrative.
const MAX_NARRATIVE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportedDistillation {
    pub adapter: String,
    pub artifact_id: String,
    pub slug: String,
    pub display_name: String,
    pub narrative: Vec<DraftNarrativeSection>,
}

/// Existence and non-emptiness cannot tell a real Distilly run from an Agent that
/// simply wrote five files with the right names. Distilly stamps its own identity
/// into `manifest.json` and `meta.json`, so those stamps are what proves the
/// validated writer actually ran.
///
/// Only structural invariants are asserted, never incidental values: a Distilly
/// upgrade may change `schema_version` or the preset string, and that must not
/// fail acceptance. What may not change is that the artifacts claim the Distilly
/// engine and the identity AI-OS asked for.
pub(crate) fn require_distilly_provenance(root: &Path, slug: &str) -> Result<(), String> {
    let manifest = artifact_json(root, slug, "manifest.json")?;
    let meta = artifact_json(root, slug, "meta.json")?;
    let expected_id = artifact_id(slug);

    let carries_writer_provenance = text_at(&manifest, &["engine", "name"]) == Some("distilly")
        && text_at(&manifest, &["kind"]) == Some("meta-skill")
        && text_at(&manifest, &["id"]) == Some(expected_id.as_str())
        && text_at(&manifest, &["manifest_version"]).is_some()
        && text_at(&meta, &["generation", "engine"]) == Some("distilly")
        && text_at(&meta, &["engine", "name"]) == Some("distilly")
        && text_at(&meta, &["slug"]) == Some(slug)
        && text_at(&meta, &["id"]) == Some(expected_id.as_str())
        && text_at(&meta, &["schema_version"]).is_some();

    if carries_writer_provenance {
        Ok(())
    } else {
        Err(
            "Quarantined artifacts do not carry Distilly writer provenance, so the validated Distilly CLI did not produce them."
                .to_owned(),
        )
    }
}

/// Read one quarantined Distilly profile into typed AI-OS values.
///
/// This performs no inference whatsoever. It verifies provenance again — the
/// quarantine may have been written by one process and imported by another — and
/// then carries across only what Distilly actually stated.
pub(crate) fn import_quarantined_distillation(
    quarantine_root: &Path,
    slug: &str,
) -> Result<ImportedDistillation, String> {
    require_distilly_provenance(quarantine_root, slug)?;
    let meta = artifact_json(quarantine_root, slug, "meta.json")?;

    let display_name = text_at(&meta, &["display_name"])
        .or_else(|| text_at(&meta, &["name"]))
        .ok_or_else(|| "Distilly meta.json does not state a display name.".to_owned())?
        .to_owned();

    let mut narrative = Vec::new();
    for (file, origin) in [
        ("work.md", NarrativeOrigin::Work),
        ("persona.md", NarrativeOrigin::Persona),
        ("SKILL.md", NarrativeOrigin::Combined),
    ] {
        narrative.push(DraftNarrativeSection {
            origin,
            adapter: "distilly".to_owned(),
            text: read_narrative(quarantine_root, slug, file)?,
        });
    }

    Ok(ImportedDistillation {
        adapter: "distilly".to_owned(),
        artifact_id: artifact_id(slug),
        slug: slug.to_owned(),
        display_name,
        narrative,
    })
}

/// Draft a profile from an import and the evidence bundle that produced it.
///
/// The result is always [`ProfileStatus::Draft`]. Every claim carries the
/// `evidence_id` of the evidence item it came from, and no claim is placed in a
/// cognitive category: AI-OS will not guess whether an observation is a decision
/// pattern or a communication style, so categorisation is human review work and
/// the claims arrive unclassified.
pub(crate) fn draft_profile(
    import: &ImportedDistillation,
    bundle: &EvidenceBundle,
    profile_id: &str,
) -> Result<PersonDistillationProfile, String> {
    if profile_id.trim().is_empty() {
        return Err("A drafted profile needs a profile identifier.".to_owned());
    }
    if bundle.evidence.is_empty() {
        return Err("A profile cannot be drafted from an empty evidence bundle.".to_owned());
    }

    let unclassified_claims = bundle.evidence.iter().map(claim_from_evidence).collect();
    let contradictions = bundle
        .evidence
        .iter()
        .filter(|evidence| evidence.assertion == EvidenceAssertion::Contradictory)
        .map(|evidence| evidence.evidence_id.clone())
        .collect();

    Ok(PersonDistillationProfile {
        profile_id: profile_id.to_owned(),
        revision: 1,
        status: ProfileStatus::Draft,
        // Authoritative: the bundle, never Distilly's own source_context.
        subject_kind: bundle.subject_kind,
        identity: Vec::new(),
        domain_expertise: Vec::new(),
        knowledge_model: Vec::new(),
        decision_patterns: Vec::new(),
        reasoning_frameworks: Vec::new(),
        preferences: Vec::new(),
        constraints: Vec::new(),
        behavioral_patterns: Vec::new(),
        communication_style: Vec::new(),
        representative_examples: Vec::new(),
        unclassified_claims,
        pending_cognitive_candidates: Vec::new(),
        draft_narrative: import.narrative.clone(),
        evidence_bundle_id: bundle.bundle_id.clone(),
        contradictions,
        revision_history: vec![RevisionRecord {
            revision: 1,
            reason: format!(
                "drafted from {} artifact {}",
                import.adapter, import.artifact_id
            ),
            evidence_bundle_id: bundle.bundle_id.clone(),
        }],
    })
}

fn claim_from_evidence(evidence: &DistillationEvidence) -> CognitiveClaim {
    let statement = evidence
        .extracted_text
        .clone()
        .or_else(|| evidence.visual_observations.first().cloned())
        .or_else(|| evidence.contextual_observations.first().cloned())
        .unwrap_or_else(|| {
            format!(
                "Observation {} carries no stated text.",
                evidence.evidence_id
            )
        });

    let (contradictory, supporting) = match evidence.assertion {
        EvidenceAssertion::Contradictory => (vec![evidence.evidence_id.clone()], Vec::new()),
        _ => (Vec::new(), vec![evidence.evidence_id.clone()]),
    };

    CognitiveClaim {
        claim_id: format!("claim-{}", evidence.evidence_id),
        statement,
        confirmed: evidence.assertion == EvidenceAssertion::Confirmed,
        confidence: evidence.confidence,
        evidence_ids: supporting,
        contradictory_evidence_ids: contradictory,
    }
}

fn artifact_id(slug: &str) -> String {
    format!("meta-skill.{DISTILLY_CHARACTER}.{slug}")
}

fn profile_path(root: &Path, slug: &str, name: &str) -> PathBuf {
    root.join("profiles").join(slug).join(name)
}

fn artifact_json(root: &Path, slug: &str, name: &str) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(profile_path(root, slug, name))
        .map_err(|_| format!("Distilly quarantine is missing {name}."))?;
    serde_json::from_str(&text).map_err(|_| {
        format!("Distilly {name} is not valid JSON, so the validated writer did not produce it.")
    })
}

fn read_narrative(root: &Path, slug: &str, name: &str) -> Result<String, String> {
    let path = profile_path(root, slug, name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| format!("Distilly quarantine is missing {name}."))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Distilly quarantine artifact {name} is a symbolic link and was not imported."
        ));
    }
    if metadata.len() > MAX_NARRATIVE_BYTES {
        return Err(format!(
            "Distilly quarantine artifact {name} exceeds the bounded narrative limit."
        ));
    }
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("Distilly quarantine artifact {name} is not readable UTF-8 text."))?;
    if text.trim().is_empty() {
        return Err(format!("Distilly quarantine artifact {name} is empty."));
    }
    Ok(text)
}

fn text_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Shaped after the artifacts a real Distilly `create` writes. The provenance
    /// fields are the ones `require_distilly_provenance` reads; the rest of the real
    /// output is deliberately not reproduced, so acceptance never depends on
    /// incidental Distilly detail.
    pub(crate) fn quarantine(slug: &str) -> tempfile::TempDir {
        let quarantine = tempfile::Builder::new()
            .prefix("ai-os-import-test-")
            .tempdir()
            .unwrap();
        let output = quarantine.path().join("profiles").join(slug);
        fs::create_dir_all(&output).unwrap();
        let id = format!("meta-skill.colleague.{slug}");
        fs::write(
            output.join("manifest.json"),
            serde_json::json!({
                "manifest_version": "1",
                "id": id,
                "kind": "meta-skill",
                "engine": {"name": "distilly"},
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            output.join("meta.json"),
            serde_json::json!({
                "schema_version": "3",
                "slug": slug,
                "id": id,
                "name": "Alice",
                "display_name": "Alice",
                "generation": {"engine": "distilly"},
                "engine": {"name": "distilly"},
                // Distilly's colleague preset always says this. AI-OS must ignore it.
                "source_context": {"is_real_person": true, "is_fictional": false},
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            output.join("work.md"),
            "# Work\n\n- Weighs downside first.\n",
        )
        .unwrap();
        fs::write(
            output.join("persona.md"),
            "# Persona\n\n- Concise bullets.\n",
        )
        .unwrap();
        fs::write(output.join("SKILL.md"), "# Alice\n\nCombined skill.\n").unwrap();
        quarantine
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::quarantine;
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*, EvidenceAssertion, EvidenceBundle},
        profile::{AiOsProfileAuthority, ProfileError},
        SourceKind, SourceMediaKind, SubjectKind,
    };
    use std::collections::HashSet;

    /// A fictional-character bundle, so the disagreement with Distilly's
    /// `source_context` is observable rather than theoretical.
    fn fictional_bundle() -> EvidenceBundle {
        let observations = [
            (
                "Alice weighs downside risk first.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "Alice prefers concise bullet points.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "Under urgency Alice acted before verifying.",
                EvidenceAssertion::Contradictory,
            ),
        ];
        let mut sources = Vec::new();
        let mut evidence = Vec::new();
        for (index, (text, assertion)) in observations.into_iter().enumerate() {
            let source = source(
                &format!("alice-source-{index}"),
                SourceMediaKind::Text,
                SourceKind::UserFile,
                false,
            );
            let mut extracted = item(&format!("alice-evidence-{index}"), SourceMediaKind::Text);
            extracted.extracted_text = Some(text.to_owned());
            extracted.assertion = assertion;
            evidence.extend(
                normalize_extraction(
                    SubjectKind::FictionalCharacter,
                    &source,
                    &local_policy(),
                    extraction(extracted),
                )
                .unwrap(),
            );
            sources.push(source);
        }
        EvidenceBundle {
            bundle_id: "alice-bundle".to_owned(),
            subject_kind: SubjectKind::FictionalCharacter,
            media_kind: SourceMediaKind::Text,
            sources,
            evidence,
        }
    }

    #[test]
    fn import_carries_creator_prose_without_turning_it_into_claims() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        assert_eq!(imported.adapter, "distilly");
        assert_eq!(imported.display_name, "Alice");
        assert_eq!(imported.narrative.len(), 3);

        let profile = draft_profile(&imported, &fictional_bundle(), "profile-alice").unwrap();

        // Every prose section survives as narrative,
        assert_eq!(profile.draft_narrative.len(), 3);
        // and none of it appears as a claim.
        let claim_statements = profile
            .unclassified_claims
            .iter()
            .map(|claim| claim.statement.as_str())
            .collect::<HashSet<_>>();
        for section in &profile.draft_narrative {
            assert!(
                !claim_statements.contains(section.text.as_str()),
                "creator prose must never become a claim"
            );
        }
    }

    #[test]
    fn every_drafted_claim_carries_a_real_evidence_id() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        let bundle = fictional_bundle();
        let profile = draft_profile(&imported, &bundle, "profile-alice").unwrap();

        let known = bundle
            .evidence
            .iter()
            .map(|evidence| evidence.evidence_id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(profile.unclassified_claims.len(), bundle.evidence.len());
        for claim in &profile.unclassified_claims {
            let referenced = claim
                .evidence_ids
                .iter()
                .chain(claim.contradictory_evidence_ids.iter());
            let mut any = false;
            for id in referenced {
                assert!(
                    known.contains(id.as_str()),
                    "claim invented evidence id {id}"
                );
                any = true;
            }
            assert!(any, "a claim with no evidence link is not admissible");
        }

        // A drafted profile places nothing in a cognitive category.
        assert!(profile.identity.is_empty());
        assert!(profile.decision_patterns.is_empty());
        assert!(profile.communication_style.is_empty());
        assert_eq!(profile.status, ProfileStatus::Draft);
    }

    #[test]
    fn subject_identity_comes_from_ai_os_not_from_distilly() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        let bundle = fictional_bundle();
        let profile = draft_profile(&imported, &bundle, "profile-alice").unwrap();

        // meta.json says is_real_person: true. The bundle says fictional character.
        // AI-OS is the authority.
        assert_eq!(profile.subject_kind, SubjectKind::FictionalCharacter);
        assert_eq!(profile.subject_kind, bundle.subject_kind);
        assert_eq!(profile.evidence_bundle_id, bundle.bundle_id);
    }

    #[test]
    fn contradictions_survive_the_draft() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        let bundle = fictional_bundle();
        let profile = draft_profile(&imported, &bundle, "profile-alice").unwrap();

        assert_eq!(profile.contradictions.len(), 1);
        let contradicted = &profile.contradictions[0];
        let claim = profile
            .unclassified_claims
            .iter()
            .find(|claim| claim.contradictory_evidence_ids.contains(contradicted))
            .expect("the contradictory evidence must still be reachable from a claim");
        assert!(!claim.confirmed);
        assert!(claim.evidence_ids.is_empty());
    }

    #[test]
    fn a_drafted_profile_cannot_be_activated_until_review_categorises_it() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        let bundle = fictional_bundle();
        let drafted = draft_profile(&imported, &bundle, "profile-alice").unwrap();

        // Even with the review flag set, unresolved material blocks activation.
        assert_eq!(
            AiOsProfileAuthority::activate(drafted.clone(), &bundle, true).unwrap_err(),
            ProfileError::UnreviewedMaterialRemains
        );

        // Review is modelled as moving the material into categorised claims.
        let mut reviewed = drafted;
        reviewed.decision_patterns = std::mem::take(&mut reviewed.unclassified_claims);
        reviewed.draft_narrative.clear();
        assert_eq!(
            AiOsProfileAuthority::activate(reviewed, &bundle, true)
                .unwrap()
                .status,
            ProfileStatus::Active
        );
    }

    #[test]
    fn artifacts_without_provenance_are_never_imported() {
        let quarantine = quarantine("alice-synthetic");
        let meta = quarantine.path().join("profiles/alice-synthetic/meta.json");
        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&meta).unwrap()).unwrap();
        value["engine"]["name"] = serde_json::json!("something-else");
        fs::write(&meta, value.to_string()).unwrap();

        let error =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap_err();
        assert!(
            error.contains("provenance"),
            "expected a provenance refusal, got: {error}"
        );
    }

    #[test]
    fn an_empty_or_oversized_narrative_artifact_is_refused() {
        let quarantine = quarantine("alice-synthetic");
        let work = quarantine.path().join("profiles/alice-synthetic/work.md");
        fs::write(&work, "   \n").unwrap();
        let error =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap_err();
        assert!(error.contains("work.md"), "got: {error}");

        fs::write(&work, "x".repeat((MAX_NARRATIVE_BYTES + 1) as usize)).unwrap();
        let error =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap_err();
        assert!(error.contains("bounded narrative limit"), "got: {error}");
    }

    #[test]
    fn a_profile_cannot_be_drafted_without_evidence() {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        let mut bundle = fictional_bundle();
        bundle.evidence.clear();
        assert!(draft_profile(&imported, &bundle, "profile-alice").is_err());
        assert!(draft_profile(&imported, &fictional_bundle(), "  ").is_err());
    }
}
