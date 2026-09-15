//! The human review lifecycle: correction, activation, revision and rollback.
//!
//! A drafted profile arrives with every claim unclassified and the creator's
//! prose attached. This module is what a reviewer's decisions actually do to it.
//!
//! The rule that shapes everything here is that **review may remove, correct and
//! categorise, but it may not invent**. A reviewer can reject a claim, rewrite its
//! wording, or say which cognitive category it belongs to. A reviewer cannot
//! create a claim that no evidence supports, and cannot attach an evidence id the
//! bundle does not contain. Review is judgement applied to evidence, not a second
//! source of truth.

use super::{
    evidence::EvidenceBundle,
    profile::{
        CognitiveClaim, PersonDistillationProfile, ProfileError, ProfileStatus, RevisionRecord,
        RunnablePersonaSkill,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ClaimCategory {
    Identity,
    DomainExpertise,
    KnowledgeModel,
    DecisionPatterns,
    ReasoningFrameworks,
    Preferences,
    Constraints,
    BehavioralPatterns,
    CommunicationStyle,
    RepresentativeExamples,
}

/// What a reviewer decided about one drafted claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaimDecision {
    pub claim_id: String,
    /// `None` rejects the claim. Rejection is a legitimate outcome: evidence can
    /// be real and still not belong in a profile.
    pub category: Option<ClaimCategory>,
    /// Optional corrected wording. The evidence links are never editable, so a
    /// correction can sharpen a statement but cannot relocate it onto other
    /// evidence.
    #[serde(default)]
    pub corrected_statement: Option<String>,
}

/// What a reviewer decided about one derived cognitive candidate.
///
/// A separate contract keeps derived Nuwa material distinguishable from
/// evidence-derived Distilly claims all the way to the human-review boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveCandidateDecision {
    pub candidate_id: String,
    /// `None` rejects the candidate.
    pub category: Option<ClaimCategory>,
    /// Optional human correction. Evidence links remain immutable.
    #[serde(default)]
    pub corrected_statement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewDecisions {
    pub reviewer: String,
    pub decisions: Vec<ClaimDecision>,
    /// Backward-compatible: old review callers and stored request fixtures that
    /// predate Nuwa candidate review deserialize as an empty candidate decision set.
    #[serde(default)]
    pub cognitive_candidate_decisions: Vec<CognitiveCandidateDecision>,
}

/// Apply a reviewer's decisions to a drafted profile.
///
/// Every unclassified claim must be decided. A profile where some claims were
/// silently left undecided would activate with material no one looked at, which is
/// the exact failure the unreviewed-material rule exists to prevent.
pub(crate) fn apply_review(
    mut profile: PersonDistillationProfile,
    bundle: &EvidenceBundle,
    review: &ReviewDecisions,
) -> Result<PersonDistillationProfile, ProfileError> {
    if profile.status != ProfileStatus::Draft || review.reviewer.trim().is_empty() {
        return Err(ProfileError::InvalidProfile);
    }
    if profile.evidence_bundle_id != bundle.bundle_id {
        return Err(ProfileError::InvalidProfile);
    }

    let drafted = std::mem::take(&mut profile.unclassified_claims);
    let cognitive_drafts = std::mem::take(&mut profile.pending_cognitive_candidates);

    let drafted_ids = drafted
        .iter()
        .map(|claim| claim.claim_id.as_str())
        .collect::<BTreeSet<_>>();
    let decided_ids = review
        .decisions
        .iter()
        .map(|decision| decision.claim_id.as_str())
        .collect::<BTreeSet<_>>();

    let cognitive_ids = cognitive_drafts
        .iter()
        .map(|candidate| candidate.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    let cognitive_decided_ids = review
        .cognitive_candidate_decisions
        .iter()
        .map(|decision| decision.candidate_id.as_str())
        .collect::<BTreeSet<_>>();

    // Every pending item must be decided, and a caller may not submit decisions
    // for material that is not actually present in this draft.
    if drafted_ids != decided_ids
        || decided_ids.len() != review.decisions.len()
        || cognitive_ids != cognitive_decided_ids
        || cognitive_decided_ids.len() != review.cognitive_candidate_decisions.len()
    {
        return Err(ProfileError::IncompleteReview);
    }

    let known_evidence = bundle
        .evidence
        .iter()
        .map(|evidence| evidence.evidence_id.as_str())
        .collect::<HashSet<_>>();

    let mut rejected = 0_usize;
    let mut accepted = 0_usize;
    for mut claim in drafted {
        let decision = review
            .decisions
            .iter()
            .find(|decision| decision.claim_id == claim.claim_id)
            .ok_or(ProfileError::IncompleteReview)?;

        let Some(category) = decision.category else {
            rejected += 1;
            continue;
        };

        if let Some(corrected) = decision.corrected_statement.as_deref() {
            if corrected.trim().is_empty() {
                return Err(ProfileError::InvalidProfile);
            }
            claim.statement = corrected.trim().to_owned();
        }

        // Review cannot invent provenance, so the links are re-checked rather than
        // trusted: they came from the draft, but the draft is user-reachable data.
        if !claim_is_evidence_backed(&claim, &known_evidence) {
            return Err(ProfileError::UnsupportedClaim);
        }

        category_slot(&mut profile, category).push(claim);
        accepted += 1;
    }

    let mut cognitive_rejected = 0_usize;
    let mut cognitive_accepted = 0_usize;

    for candidate in cognitive_drafts {
        let decision = review
            .cognitive_candidate_decisions
            .iter()
            .find(|decision| decision.candidate_id == candidate.candidate_id)
            .ok_or(ProfileError::IncompleteReview)?;

        let Some(category) = decision.category else {
            cognitive_rejected += 1;
            continue;
        };

        // At this boundary AI-OS still treats the stored candidate as
        // user-reachable data. Re-check the parts of the contract that matter
        // before promoting it.
        if candidate.adapter != "nuwa"
            || !candidate.confidence.is_finite()
            || !(0.0..=1.0).contains(&candidate.confidence)
        {
            return Err(ProfileError::InvalidProfile);
        }

        let mut statement = candidate.statement.clone();
        if let Some(corrected) = decision.corrected_statement.as_deref() {
            if corrected.trim().is_empty() {
                return Err(ProfileError::InvalidProfile);
            }
            statement = corrected.trim().to_owned();
        }

        let claim = CognitiveClaim {
            // Keep the stable Nuwa-prefixed id so the canonical claim remains
            // traceable to the reviewed candidate without expanding the mature
            // CognitiveClaim schema.
            claim_id: candidate.candidate_id.clone(),
            statement,
            // Human acceptance means "allow this inference into the profile".
            // It does not transform an inference into directly observed fact.
            confirmed: false,
            confidence: candidate.confidence,
            evidence_ids: candidate.evidence_ids.clone(),
            contradictory_evidence_ids: candidate.contradictory_evidence_ids.clone(),
        };

        if !claim_is_evidence_backed(&claim, &known_evidence) {
            return Err(ProfileError::UnsupportedClaim);
        }

        category_slot(&mut profile, category).push(claim);
        cognitive_accepted += 1;
    }

    // The narrative has been read; it is no longer pending material. It is not
    // retained, because keeping unattributed prose inside an active profile is
    // exactly the thing the claim/narrative split exists to avoid.
    profile.draft_narrative.clear();
    profile.status = ProfileStatus::Reviewed;
    profile.revision_history.push(RevisionRecord {
        revision: profile.revision,
        reason: format!(
            "reviewed by {}: {accepted} categorised, {rejected} rejected; {cognitive_accepted} cognitive candidates accepted, {cognitive_rejected} rejected",
            review.reviewer.trim()
        ),
        evidence_bundle_id: bundle.bundle_id.clone(),
    });
    Ok(profile)
}

/// Open a new revision of an active profile against a new evidence bundle.
///
/// The result is a Draft again. New evidence does not silently amend a profile
/// people are already relying on; it produces something that has to be reviewed on
/// its own terms.
pub(crate) fn revise(
    profile: &PersonDistillationProfile,
    new_bundle: &EvidenceBundle,
    reason: &str,
) -> Result<PersonDistillationProfile, ProfileError> {
    if profile.status != ProfileStatus::Active || reason.trim().is_empty() {
        return Err(ProfileError::InvalidProfile);
    }
    if profile.subject_kind != new_bundle.subject_kind {
        return Err(ProfileError::InvalidProfile);
    }
    let mut next = profile.clone();
    next.revision = profile
        .revision
        .checked_add(1)
        .ok_or(ProfileError::InvalidProfile)?;
    next.status = ProfileStatus::Draft;
    next.evidence_bundle_id = new_bundle.bundle_id.clone();
    next.revision_history.push(RevisionRecord {
        revision: next.revision,
        reason: reason.trim().to_owned(),
        evidence_bundle_id: new_bundle.bundle_id.clone(),
    });
    Ok(next)
}

/// Every revision of one profile, and which one is live.
///
/// Rollback never rewrites history: it republishes an earlier revision as a new
/// one, so the ledger always shows that a rollback happened and when.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileLedger {
    revisions: Vec<PersonDistillationProfile>,
}

impl ProfileLedger {
    pub(crate) fn from_history(
        revisions: impl IntoIterator<Item = PersonDistillationProfile>,
    ) -> Result<Self, ProfileError> {
        let mut ledger = Self::default();
        for profile in revisions {
            ledger.record(profile)?;
        }
        Ok(ledger)
    }

    pub(crate) fn record(
        &mut self,
        profile: PersonDistillationProfile,
    ) -> Result<(), ProfileError> {
        if let Some(previous) = self.revisions.last() {
            if previous.profile_id != profile.profile_id {
                return Err(ProfileError::InvalidProfile);
            }
        }
        self.revisions.push(profile);
        Ok(())
    }

    pub(crate) fn active(&self) -> Option<&PersonDistillationProfile> {
        self.revisions
            .iter()
            .rev()
            .find(|profile| profile.status == ProfileStatus::Active)
    }

    pub(crate) fn revision(&self, revision: u32) -> Option<&PersonDistillationProfile> {
        self.revisions
            .iter()
            .rev()
            .find(|profile| profile.revision == revision)
    }

    pub(crate) fn rollback_to(&mut self, revision: u32) -> Result<(), ProfileError> {
        let target = self
            .revision(revision)
            .filter(|profile| profile.status == ProfileStatus::Active)
            .ok_or(ProfileError::InvalidProfile)?
            .clone();
        let live = self.active().ok_or(ProfileError::InvalidProfile)?;
        if live.revision == revision {
            return Err(ProfileError::InvalidProfile);
        }

        let mut restored = target;
        let next_revision = self
            .revisions
            .iter()
            .map(|profile| profile.revision)
            .max()
            .unwrap_or(restored.revision)
            .checked_add(1)
            .ok_or(ProfileError::InvalidProfile)?;
        restored.revision_history.push(RevisionRecord {
            revision: next_revision,
            reason: format!("rolled back to revision {revision}"),
            evidence_bundle_id: restored.evidence_bundle_id.clone(),
        });
        restored.revision = next_revision;
        self.revisions.push(restored);
        Ok(())
    }
}

/// Build the runnable persona skill from an active profile.
///
/// Only categorised claims reach the skill. Raw media never does, and the
/// generator does not read the evidence bundle at all — by the time a profile is
/// active, the claims are the profile, and reaching back into source material
/// would put private media into a runnable artifact.
pub(crate) fn build_persona_skill(
    profile: &PersonDistillationProfile,
) -> Result<RunnablePersonaSkill, ProfileError> {
    if profile.status != ProfileStatus::Active {
        return Err(ProfileError::InvalidProfile);
    }

    let sections = [
        ("Identity", &profile.identity),
        ("Domain expertise", &profile.domain_expertise),
        ("Knowledge model", &profile.knowledge_model),
        ("Decision patterns", &profile.decision_patterns),
        ("Reasoning frameworks", &profile.reasoning_frameworks),
        ("Preferences", &profile.preferences),
        ("Constraints", &profile.constraints),
        ("Behavioral patterns", &profile.behavioral_patterns),
        ("Communication style", &profile.communication_style),
    ];

    let mut instructions = String::new();
    for (heading, claims) in sections {
        if claims.is_empty() {
            continue;
        }
        instructions.push_str(&format!("## {heading}\n"));
        for claim in claims {
            // Unconfirmed material stays marked. A persona that presents inferred
            // and contradicted observations as settled fact misrepresents the
            // person it is modelled on.
            let qualifier = if claim.confirmed {
                ""
            } else {
                " (unconfirmed)"
            };
            instructions.push_str(&format!("- {}{qualifier}\n", claim.statement));
        }
        instructions.push('\n');
    }

    if instructions.trim().is_empty() {
        return Err(ProfileError::InvalidProfile);
    }
    if !profile.contradictions.is_empty() {
        instructions.push_str(
            "## Unresolved\n- The evidence for this person contains contradictions that review did not resolve; prefer asking over guessing.\n",
        );
    }

    Ok(RunnablePersonaSkill {
        skill_id: format!("persona-{}", profile.profile_id),
        profile_id: profile.profile_id.clone(),
        profile_revision: profile.revision,
        instructions,
        examples: profile
            .representative_examples
            .iter()
            .map(|claim| claim.statement.clone())
            .collect(),
        raw_media_assets: Vec::new(),
    })
}

fn claim_is_evidence_backed(claim: &CognitiveClaim, known: &HashSet<&str>) -> bool {
    let referenced = claim
        .evidence_ids
        .iter()
        .chain(claim.contradictory_evidence_ids.iter())
        .collect::<Vec<_>>();
    !referenced.is_empty()
        && referenced.into_iter().all(|id| known.contains(id.as_str()))
        && !claim.statement.trim().is_empty()
}

fn category_slot(
    profile: &mut PersonDistillationProfile,
    category: ClaimCategory,
) -> &mut Vec<CognitiveClaim> {
    match category {
        ClaimCategory::Identity => &mut profile.identity,
        ClaimCategory::DomainExpertise => &mut profile.domain_expertise,
        ClaimCategory::KnowledgeModel => &mut profile.knowledge_model,
        ClaimCategory::DecisionPatterns => &mut profile.decision_patterns,
        ClaimCategory::ReasoningFrameworks => &mut profile.reasoning_frameworks,
        ClaimCategory::Preferences => &mut profile.preferences,
        ClaimCategory::Constraints => &mut profile.constraints,
        ClaimCategory::BehavioralPatterns => &mut profile.behavioral_patterns,
        ClaimCategory::CommunicationStyle => &mut profile.communication_style,
        ClaimCategory::RepresentativeExamples => &mut profile.representative_examples,
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::cognitive_distillation::{
        evidence::{normalize_extraction, test_support::*, EvidenceAssertion},
        import::{draft_profile, import_quarantined_distillation, test_support::quarantine},
        profile::AiOsProfileAuthority,
        SourceKind, SourceMediaKind, SubjectKind,
    };

    pub(crate) fn bundle(id: &str) -> EvidenceBundle {
        let observations = [
            ("Weighs downside risk first.", EvidenceAssertion::Confirmed),
            (
                "Prefers concise bullet points.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "Under urgency acted before verifying.",
                EvidenceAssertion::Contradictory,
            ),
        ];
        let mut sources = Vec::new();
        let mut evidence = Vec::new();
        for (index, (text, assertion)) in observations.into_iter().enumerate() {
            let source = source(
                &format!("{id}-source-{index}"),
                SourceMediaKind::Text,
                SourceKind::UserFile,
                false,
            );
            let mut extracted = item(&format!("{id}-evidence-{index}"), SourceMediaKind::Text);
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
            bundle_id: id.to_owned(),
            subject_kind: SubjectKind::FictionalCharacter,
            media_kind: SourceMediaKind::Text,
            sources,
            evidence,
        }
    }

    pub(crate) fn drafted(bundle: &EvidenceBundle) -> PersonDistillationProfile {
        let quarantine = quarantine("alice-synthetic");
        let imported =
            import_quarantined_distillation(quarantine.path(), "alice-synthetic").unwrap();
        draft_profile(&imported, bundle, "profile-alice").unwrap()
    }

    pub(crate) fn decide_all(
        profile: &PersonDistillationProfile,
        category: Option<ClaimCategory>,
    ) -> ReviewDecisions {
        ReviewDecisions {
            reviewer: "russell".to_owned(),
            decisions: profile
                .unclassified_claims
                .iter()
                .map(|claim| ClaimDecision {
                    claim_id: claim.claim_id.clone(),
                    category,
                    corrected_statement: None,
                })
                .collect(),
            cognitive_candidate_decisions: profile
                .pending_cognitive_candidates
                .iter()
                .map(|candidate| CognitiveCandidateDecision {
                    candidate_id: candidate.candidate_id.clone(),
                    category,
                    corrected_statement: None,
                })
                .collect(),
        }
    }

    pub(crate) fn review_everything(
        profile: PersonDistillationProfile,
        bundle: &EvidenceBundle,
    ) -> PersonDistillationProfile {
        let decisions = decide_all(&profile, Some(ClaimCategory::DecisionPatterns));
        apply_review(profile, bundle, &decisions).unwrap()
    }

    pub(crate) fn activate(
        profile: PersonDistillationProfile,
        bundle: &EvidenceBundle,
    ) -> PersonDistillationProfile {
        AiOsProfileAuthority::activate(profile, bundle, true).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::cognitive_distillation::{profile::AiOsProfileAuthority, SubjectKind};

    fn activated() -> (PersonDistillationProfile, EvidenceBundle) {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let decisions = decide_all(&draft, Some(ClaimCategory::DecisionPatterns));
        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();
        let active = AiOsProfileAuthority::activate(reviewed, &bundle, true).unwrap();
        (active, bundle)
    }

    #[test]
    fn review_categorises_claims_and_clears_the_creator_narrative() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let drafted_count = draft.unclassified_claims.len();
        assert!(!draft.draft_narrative.is_empty());

        let decisions = decide_all(&draft, Some(ClaimCategory::CommunicationStyle));
        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();

        assert_eq!(reviewed.status, ProfileStatus::Reviewed);
        assert_eq!(reviewed.communication_style.len(), drafted_count);
        assert!(reviewed.unclassified_claims.is_empty());
        assert!(reviewed.draft_narrative.is_empty());
        assert!(reviewed
            .revision_history
            .last()
            .unwrap()
            .reason
            .contains("reviewed by russell"));
    }

    #[test]
    fn a_review_that_leaves_a_claim_undecided_is_refused() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let mut decisions = decide_all(&draft, Some(ClaimCategory::Identity));
        decisions.decisions.pop();
        assert_eq!(
            apply_review(draft, &bundle, &decisions).unwrap_err(),
            ProfileError::IncompleteReview
        );
    }

    #[test]
    fn nuwa_candidate_requires_explicit_review_before_promotion() {
        let bundle = bundle("alice-bundle");
        let mut draft = drafted(&bundle);
        let evidence_id = bundle.evidence[0].evidence_id.clone();

        draft.pending_cognitive_candidates.push(
            crate::cognitive_distillation::profile::PendingCognitiveCandidate {
                candidate_id: "nuwa-mental-model-1".to_owned(),
                adapter: "nuwa".to_owned(),
                kind: crate::cognitive_distillation::profile::CognitiveCandidateKind::MentalModel,
                statement: "Tests alternatives before committing.".to_owned(),
                confidence: 0.82,
                evidence_ids: vec![evidence_id],
                contradictory_evidence_ids: Vec::new(),
            },
        );

        let mut incomplete = decide_all(&draft, Some(ClaimCategory::ReasoningFrameworks));
        incomplete.cognitive_candidate_decisions.clear();
        assert_eq!(
            apply_review(draft.clone(), &bundle, &incomplete).unwrap_err(),
            ProfileError::IncompleteReview
        );

        let decisions = decide_all(&draft, Some(ClaimCategory::ReasoningFrameworks));
        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();

        assert!(reviewed.pending_cognitive_candidates.is_empty());

        let promoted = reviewed
            .reasoning_frameworks
            .iter()
            .find(|claim| claim.claim_id == "nuwa-mental-model-1")
            .expect("accepted Nuwa candidate should become a canonical CognitiveClaim");

        assert!(!promoted.confirmed);
        assert_eq!(promoted.statement, "Tests alternatives before committing.");
    }

    #[test]
    fn rejecting_a_nuwa_candidate_does_not_promote_it() {
        let bundle = bundle("alice-bundle");
        let mut draft = drafted(&bundle);

        draft
            .pending_cognitive_candidates
            .push(crate::cognitive_distillation::profile::PendingCognitiveCandidate {
            candidate_id: "nuwa-decision-heuristic-1".to_owned(),
            adapter: "nuwa".to_owned(),
            kind: crate::cognitive_distillation::profile::CognitiveCandidateKind::DecisionHeuristic,
            statement: "Prefers reversible decisions.".to_owned(),
            confidence: 0.7,
            evidence_ids: vec![bundle.evidence[0].evidence_id.clone()],
            contradictory_evidence_ids: Vec::new(),
        });

        let mut decisions = decide_all(&draft, Some(ClaimCategory::DecisionPatterns));
        decisions.cognitive_candidate_decisions[0].category = None;

        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();

        assert!(reviewed.pending_cognitive_candidates.is_empty());
        assert!(reviewed
            .decision_patterns
            .iter()
            .all(|claim| claim.claim_id != "nuwa-decision-heuristic-1"));
    }

    #[test]
    fn rejecting_a_claim_is_allowed_and_recorded() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let decisions = decide_all(&draft, None);
        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();

        assert!(reviewed.identity.is_empty());
        assert!(reviewed.decision_patterns.is_empty());
        assert!(reviewed
            .revision_history
            .last()
            .unwrap()
            .reason
            .contains("0 categorised, 3 rejected"));
    }

    /// Review is judgement applied to evidence, not a second source of truth.
    #[test]
    fn review_cannot_introduce_a_claim_the_evidence_does_not_support() {
        let bundle = bundle("alice-bundle");
        let mut draft = drafted(&bundle);

        // A claim whose evidence link points at nothing in the bundle.
        draft.unclassified_claims[0].evidence_ids = vec!["invented-evidence".to_owned()];
        draft.unclassified_claims[0]
            .contradictory_evidence_ids
            .clear();
        let decisions = decide_all(&draft, Some(ClaimCategory::Identity));
        assert_eq!(
            apply_review(draft.clone(), &bundle, &decisions).unwrap_err(),
            ProfileError::UnsupportedClaim
        );

        // A claim with no evidence link at all.
        draft.unclassified_claims[0].evidence_ids.clear();
        draft.unclassified_claims[0]
            .contradictory_evidence_ids
            .clear();
        assert_eq!(
            apply_review(draft, &bundle, &decisions).unwrap_err(),
            ProfileError::UnsupportedClaim
        );
    }

    #[test]
    fn a_correction_may_reword_a_claim_but_not_relocate_its_evidence() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let original_links = draft.unclassified_claims[0].evidence_ids.clone();
        let mut decisions = decide_all(&draft, Some(ClaimCategory::Preferences));
        decisions.decisions[0].corrected_statement =
            Some("  Prefers a direct recommendation.  ".to_owned());

        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();
        let corrected = reviewed
            .preferences
            .iter()
            .find(|claim| claim.statement == "Prefers a direct recommendation.")
            .expect("the corrected wording should be stored, trimmed");
        assert_eq!(corrected.evidence_ids, original_links);
    }

    #[test]
    fn an_empty_correction_is_refused() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        let mut decisions = decide_all(&draft, Some(ClaimCategory::Preferences));
        decisions.decisions[0].corrected_statement = Some("   ".to_owned());
        assert_eq!(
            apply_review(draft, &bundle, &decisions).unwrap_err(),
            ProfileError::InvalidProfile
        );
    }

    #[test]
    fn a_reviewed_profile_activates_and_new_evidence_opens_a_draft_revision() {
        let (active, _) = activated();
        assert_eq!(active.status, ProfileStatus::Active);
        assert_eq!(active.revision, 1);

        let later = bundle("alice-bundle-2");
        let revised = revise(&active, &later, "new interview evidence").unwrap();
        assert_eq!(revised.revision, 2);
        // New evidence does not silently amend a profile people already rely on.
        assert_eq!(revised.status, ProfileStatus::Draft);
        assert_eq!(revised.evidence_bundle_id, "alice-bundle-2");
        assert!(revised
            .revision_history
            .last()
            .unwrap()
            .reason
            .contains("new interview evidence"));
    }

    #[test]
    fn a_draft_cannot_be_revised_and_a_subject_kind_cannot_change_across_revisions() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        assert!(revise(&draft, &bundle, "reason").is_err());

        let (active, _) = activated();
        let mut different = bundle.clone();
        different.subject_kind = SubjectKind::PrivatePerson;
        assert_eq!(
            revise(&active, &different, "reason").unwrap_err(),
            ProfileError::InvalidProfile
        );
    }

    #[test]
    fn rollback_republishes_an_earlier_revision_without_rewriting_history() {
        let (first, first_bundle) = activated();
        let mut ledger = ProfileLedger::default();
        ledger.record(first.clone()).unwrap();

        let second_bundle = bundle("alice-bundle-2");
        let revised = revise(&first, &second_bundle, "second pass").unwrap();
        let decisions = decide_all(&revised, Some(ClaimCategory::Constraints));
        let reviewed = apply_review(revised, &second_bundle, &decisions).unwrap();
        let second = AiOsProfileAuthority::activate(reviewed, &second_bundle, true).unwrap();
        ledger.record(second).unwrap();

        assert_eq!(ledger.active().unwrap().revision, 2);
        ledger.rollback_to(1).unwrap();

        let live = ledger.active().unwrap();
        // Republished as a NEW revision, carrying revision 1's content.
        assert_eq!(live.revision, 3);
        assert_eq!(live.evidence_bundle_id, first_bundle.bundle_id);
        assert!(live
            .revision_history
            .last()
            .unwrap()
            .reason
            .contains("rolled back to revision 1"));
        // History is appended to, never truncated.
        assert!(ledger.revision(2).is_some());
        assert!(
            ledger.rollback_to(3).is_err(),
            "cannot roll back to the live revision"
        );
    }

    #[test]
    fn the_persona_skill_is_built_only_from_categorised_claims_and_never_carries_media() {
        let (active, _) = activated();
        let skill = build_persona_skill(&active).unwrap();
        let packaged = AiOsProfileAuthority::package_skill(&active, skill.clone()).unwrap();

        assert!(packaged.raw_media_assets.is_empty());
        assert_eq!(packaged.profile_revision, active.revision);
        assert!(packaged.instructions.contains("## Decision patterns"));
        assert!(packaged
            .instructions
            .contains("Weighs downside risk first."));
        // The contradicted observation is carried, but never as settled fact.
        assert!(packaged.instructions.contains("(unconfirmed)"));
        assert!(packaged.instructions.contains("## Unresolved"));
    }

    #[test]
    fn a_profile_that_is_not_active_produces_no_runnable_skill() {
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        assert!(build_persona_skill(&draft).is_err());

        let decisions = decide_all(&draft, Some(ClaimCategory::Identity));
        let reviewed = apply_review(draft, &bundle, &decisions).unwrap();
        assert!(build_persona_skill(&reviewed).is_err());
    }
}
