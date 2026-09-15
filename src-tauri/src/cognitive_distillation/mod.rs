mod adapters;
mod creator;
mod evidence;
mod import;
mod nuwa;
mod profile;
mod review;
mod router;
mod store;
mod toolchain;
mod transcription;
mod visual;

pub(crate) use evidence::{validate_bundle, EvidenceBundle};
pub(crate) use router::{
    route_distillation, CognitiveDistillationRouteRequest, CognitiveDistillationRouteResult,
};

use adapters::AdapterCatalog;
use profile::{PersonDistillationProfile, ProfileStatus, RunnablePersonaSkill};
use review::{ProfileLedger, ReviewDecisions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use store::{ProfileStore, ProfileSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SubjectKind {
    SelfProfile,
    PrivatePerson,
    PublicPerson,
    HistoricalPerson,
    FictionalCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceMediaKind {
    Text,
    Image,
    Audio,
    Video,
    Document,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceKind {
    Chat,
    Email,
    Article,
    Book,
    Interview,
    Meeting,
    Social,
    UserFile,
    PublicWeb,
    TaskEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CognitiveDistillationStatus {
    pub multimodal_first: bool,
    pub canonical_profile_authority: String,
    pub creator_adapters: Vec<adapters::AdapterStatus>,
    pub asr_adapters: Vec<adapters::ExtractorStatus>,
    pub media_demux: adapters::ExtractorStatus,
    pub visual_analysis_capabilities: Vec<String>,
    pub no_silent_cloud_upload: bool,
}

#[tauri::command]
pub(crate) fn get_cognitive_distillation_status() -> CognitiveDistillationStatus {
    let catalog = AdapterCatalog::probe();
    CognitiveDistillationStatus {
        multimodal_first: true,
        canonical_profile_authority: "ai-os".to_owned(),
        creator_adapters: catalog.statuses(),
        asr_adapters: adapters::probe_asr_adapters(),
        media_demux: adapters::probe_ffmpeg(),
        visual_analysis_capabilities: vec![
            "media.reference.image.analyze".to_owned(),
            "media.reference.video.analyze".to_owned(),
        ],
        no_silent_cloud_upload: true,
    }
}

#[tauri::command]
pub(crate) fn preview_cognitive_distillation_route(
    request: CognitiveDistillationRouteRequest,
) -> Result<CognitiveDistillationRouteResult, String> {
    route_distillation(&request, &AdapterCatalog::probe()).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn validate_cognitive_distillation_evidence(
    bundle: EvidenceBundle,
) -> Result<EvidenceBundle, String> {
    validate_bundle(&bundle).map_err(|error| error.to_string())?;
    Ok(bundle)
}

pub(crate) fn prepare_from_value(input: &Value) -> Result<Value, String> {
    let request: CognitiveDistillationRouteRequest = serde_json::from_value(input.clone())
        .map_err(|_| {
            "Cognitive Distillation input does not match the canonical contract.".to_owned()
        })?;
    let result = preview_cognitive_distillation_route(request)?;
    serde_json::to_value(result)
        .map_err(|_| "Cognitive Distillation route could not be serialized.".to_owned())
}

pub(crate) fn invoke_creator_from_value(
    input: &Value,
    operation_id: &str,
) -> Result<Value, String> {
    creator::invoke_from_value(input, operation_id)
}

// ---------------------------------------------------------------------------
// Person profile lifecycle commands
//
// Each command reads the newest stored revision together with the evidence
// bundle it was drafted from, applies one lifecycle step, and appends the result.
// Nothing is updated in place; see `store` for why.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersonProfileView {
    pub profile: PersonDistillationProfile,
    /// The live revision, which is not always the newest one: opening a new
    /// revision leaves the previous one active until review completes.
    pub active_revision: Option<u32>,
    pub revision_count: usize,
}

#[tauri::command]
pub(crate) fn list_person_profiles() -> Result<Vec<ProfileSummary>, String> {
    ProfileStore::open_default()?.list()
}

#[tauri::command]
pub(crate) fn get_person_profile(profile_id: String) -> Result<PersonProfileView, String> {
    view(&ProfileStore::open_default()?, &profile_id)
}

/// Apply a reviewer's decisions to the newest drafted revision.
#[tauri::command]
pub(crate) fn review_person_profile(
    profile_id: String,
    decisions: ReviewDecisions,
) -> Result<PersonProfileView, String> {
    review_in(&ProfileStore::open_default()?, &profile_id, &decisions)
}

fn review_in(
    store: &ProfileStore,
    profile_id: &str,
    decisions: &ReviewDecisions,
) -> Result<PersonProfileView, String> {
    let (profile, bundle) = store.latest(profile_id)?;
    let reviewed =
        review::apply_review(profile, &bundle, decisions).map_err(|error| error.to_string())?;
    store.append(&reviewed, &bundle)?;
    view(store, profile_id)
}

/// Activate the newest revision.
///
/// There is deliberately no `human_reviewed` parameter. A caller-supplied boolean
/// would let any caller assert that review happened; instead activation requires
/// the profile to already be in `Reviewed`, a state only `apply_review` can
/// produce and only by deciding every drafted claim. The evidence of review is the
/// state of the profile, not a flag.
#[tauri::command]
pub(crate) fn activate_person_profile(profile_id: String) -> Result<PersonProfileView, String> {
    activate_in(&ProfileStore::open_default()?, &profile_id)
}

fn activate_in(store: &ProfileStore, profile_id: &str) -> Result<PersonProfileView, String> {
    let (profile, bundle) = store.latest(profile_id)?;
    if profile.status != ProfileStatus::Reviewed {
        return Err("Only a reviewed profile revision can be activated.".to_owned());
    }
    let active = profile::AiOsProfileAuthority::activate(profile, &bundle, true)
        .map_err(|error| error.to_string())?;
    store.append(&active, &bundle)?;
    view(store, profile_id)
}

/// Open a new revision against a new evidence bundle. The result is a Draft; the
/// currently active revision stays live until the new one is reviewed.
#[tauri::command]
pub(crate) fn revise_person_profile(
    profile_id: String,
    bundle: EvidenceBundle,
    reason: String,
) -> Result<PersonProfileView, String> {
    revise_in(
        &ProfileStore::open_default()?,
        &profile_id,
        &bundle,
        &reason,
    )
}

fn revise_in(
    store: &ProfileStore,
    profile_id: &str,
    bundle: &EvidenceBundle,
    reason: &str,
) -> Result<PersonProfileView, String> {
    validate_bundle(bundle).map_err(|error| error.to_string())?;
    let ledger = store.ledger(profile_id)?;
    let active = ledger
        .active()
        .ok_or_else(|| "Only an active profile can be revised.".to_owned())?;
    let revised = review::revise(active, bundle, reason).map_err(|error| error.to_string())?;
    store.append(&revised, bundle)?;
    view(store, profile_id)
}

#[tauri::command]
pub(crate) fn rollback_person_profile(
    profile_id: String,
    revision: u32,
) -> Result<PersonProfileView, String> {
    rollback_in(&ProfileStore::open_default()?, &profile_id, revision)
}

fn rollback_in(
    store: &ProfileStore,
    profile_id: &str,
    revision: u32,
) -> Result<PersonProfileView, String> {
    let mut ledger = store.ledger(profile_id)?;
    ledger
        .rollback_to(revision)
        .map_err(|error| error.to_string())?;
    let restored = ledger
        .active()
        .ok_or_else(|| "Rollback produced no active revision.".to_owned())?
        .clone();
    let bundle = store
        .history(profile_id)?
        .into_iter()
        .rev()
        .find(|(profile, _)| profile.evidence_bundle_id == restored.evidence_bundle_id)
        .map(|(_, bundle)| bundle)
        .ok_or_else(|| "The restored revision has no stored evidence bundle.".to_owned())?;
    store.append(&restored, &bundle)?;
    view(store, profile_id)
}

#[tauri::command]
pub(crate) fn build_person_persona_skill(
    profile_id: String,
) -> Result<RunnablePersonaSkill, String> {
    persona_skill_in(&ProfileStore::open_default()?, &profile_id)
}

fn persona_skill_in(
    store: &ProfileStore,
    profile_id: &str,
) -> Result<RunnablePersonaSkill, String> {
    let ledger = store.ledger(profile_id)?;
    let active = ledger
        .active()
        .ok_or_else(|| "Only an active profile produces a runnable persona skill.".to_owned())?;
    let skill = review::build_persona_skill(active).map_err(|error| error.to_string())?;
    profile::AiOsProfileAuthority::package_skill(active, skill).map_err(|error| error.to_string())
}

fn view(store: &ProfileStore, profile_id: &str) -> Result<PersonProfileView, String> {
    let history = store.history(profile_id)?;
    let ledger = ProfileLedger::from_history(history.iter().map(|(profile, _)| profile.clone()))
        .map_err(|error| error.to_string())?;
    let (profile, _) = history
        .last()
        .cloned()
        .ok_or_else(|| format!("No stored profile revision for {profile_id}."))?;
    Ok(PersonProfileView {
        active_revision: ledger.active().map(|profile| profile.revision),
        revision_count: history.len(),
        profile,
    })
}

/* ===========================
   Handing files over
=========================== */

/// A file that was handed over and could not be read, with the reason.
///
/// Reported rather than swallowed. Reading four of five files and quietly
/// returning four is how a person ends up believing a profile saw material it
/// never saw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkippedMedia {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaReadResult {
    pub bundle: EvidenceBundle,
    pub skipped: Vec<SkippedMedia>,
    /// What the reader could not observe about the files it did read — a video
    /// with no speech, a picture with no legible text. Carried so nobody assumes
    /// the material was understood in full.
    pub notes: Vec<String>,
}

/// Read whatever was handed over into one validated evidence bundle.
///
/// This is the production caller the media readers were missing: everything
/// under `transcription` and `visual` was reachable only from tests, which is
/// why `cargo check` reported the whole pipeline as dead code. A capability with
/// no entry point is not a capability.
///
/// One file failing does not fail the handover. The bundle is built from what
/// could be read and the rest is reported, because a person who drags in a
/// folder should learn which file was unreadable, not be told the whole batch
/// failed.
pub(crate) fn read_media_with(
    paths: &[String],
    subject_kind: SubjectKind,
    bundle_id: &str,
    tools: &transcription::TranscriptionTools,
) -> Result<MediaReadResult, String> {
    if paths.is_empty() {
        return Err("No file was handed over to read.".to_owned());
    }
    let work = tempfile::Builder::new()
        .prefix("ai-os-media-read-")
        .tempdir()
        .map_err(|_| "A working directory for reading media could not be created.".to_owned())?;

    let mut sources = Vec::new();
    let mut evidence = Vec::new();
    let mut skipped = Vec::new();
    let mut notes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for path in paths {
        // The same file handed over twice would produce two sources with one
        // source_id, which `validate_bundle` refuses for a reason nobody could
        // act on. Deduplicating by resolved path says what actually happened.
        let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        if !seen.insert(resolved.clone()) {
            continue;
        }
        match transcription::evidence_from_any_media(
            &resolved,
            subject_kind,
            bundle_id,
            work.path(),
            tools,
        ) {
            Ok(read) => {
                let name = display_name(&resolved);
                if let Some(note) = read.note {
                    notes.push(format!("{name}: {note}"));
                }
                // How much of the picture was actually read. "I read the video"
                // and "I read fourteen distinct screens from it" are different
                // claims, and only the second one can be checked against the
                // material by whoever reviews the profile.
                if read.screen_states_read > 0 {
                    notes.push(format!(
                        "{name}: read {} distinct screen(s).",
                        read.screen_states_read
                    ));
                }
                // Evidence ids are unique only within one extraction: two audio
                // files both start at `transcript-00000`, and `validate_bundle`
                // does not check for collisions across a bundle. Prefixing with
                // the source makes them unique without touching the reader.
                evidence.extend(read.evidence.into_iter().map(|mut item| {
                    item.evidence_id = format!("{}:{}", read.source.source_id, item.evidence_id);
                    item
                }));
                sources.push(read.source);
            }
            Err(reason) => skipped.push(SkippedMedia {
                path: display_name(&resolved),
                reason,
            }),
        }
    }

    if sources.is_empty() {
        let detail = skipped
            .iter()
            .map(|item| format!("{}: {}", item.path, item.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Nothing handed over could be read. {detail}"));
    }

    let kinds = sources
        .iter()
        .map(|source| source.media_kind)
        .collect::<std::collections::HashSet<_>>();
    let media_kind = if kinds.len() == 1 {
        sources[0].media_kind
    } else {
        SourceMediaKind::Mixed
    };

    let bundle = EvidenceBundle {
        bundle_id: bundle_id.to_owned(),
        subject_kind,
        media_kind,
        sources,
        evidence,
    };
    validate_bundle(&bundle).map_err(|error| error.to_string())?;

    Ok(MediaReadResult {
        bundle,
        skipped,
        notes,
    })
}

/// The file's own name. Full paths are the owner's filesystem, not something a
/// report needs to spell out.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Read handed-over files into evidence.
#[tauri::command]
pub(crate) fn read_media_evidence(
    paths: Vec<String>,
    subject_kind: SubjectKind,
    bundle_id: String,
) -> Result<MediaReadResult, String> {
    let tools = transcription::TranscriptionTools::resolve()?;
    read_media_with(&paths, subject_kind, &bundle_id, &tools)
}

/// What each reading tier still needs before it can run on this machine.
#[tauri::command]
pub(crate) fn media_toolchain_status() -> Vec<toolchain::TierRequirement> {
    let profile = toolchain::ModelProfile::for_this_machine();
    [
        toolchain::MediaTier::Text,
        toolchain::MediaTier::ScreenText,
        toolchain::MediaTier::Speech,
        toolchain::MediaTier::Picture,
    ]
    .into_iter()
    .map(|tier| toolchain::requirement(tier, profile))
    .collect()
}

/// Fetch what a tier needs. Downloads are verified against pinned digests.
#[tauri::command]
pub(crate) fn install_media_toolchain(tier: toolchain::MediaTier) -> Result<(), String> {
    toolchain::install(tier, toolchain::ModelProfile::for_this_machine())
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::cognitive_distillation::review::{test_support::*, ClaimCategory};

    fn store() -> (ProfileStore, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let store = ProfileStore::open_at(directory.path().join("profiles.sqlite3")).unwrap();
        (store, directory)
    }

    #[test]
    fn nuwa_candidate_survives_store_review_activation_and_persona_packaging() {
        use crate::cognitive_distillation::{
            profile::{CognitiveCandidateKind, PendingCognitiveCandidate, ProfileStatus},
            review::{ClaimCategory, CognitiveCandidateDecision},
        };

        let (store, _directory) = store();
        let evidence_bundle = bundle("nuwa-store-bundle");
        let mut draft = drafted(&evidence_bundle);

        let evidence_id = evidence_bundle
            .evidence
            .first()
            .expect("test bundle must carry evidence")
            .evidence_id
            .clone();

        draft
            .pending_cognitive_candidates
            .push(PendingCognitiveCandidate {
                candidate_id: "nuwa-mental-model-store-1".to_owned(),
                adapter: "nuwa".to_owned(),
                kind: CognitiveCandidateKind::MentalModel,
                statement: "Compares alternatives before committing.".to_owned(),
                confidence: 0.84,
                evidence_ids: vec![evidence_id],
                contradictory_evidence_ids: Vec::new(),
            });

        store.append(&draft, &evidence_bundle).unwrap();

        // The pending cognitive inference must survive the actual persistence
        // boundary and remain non-canonical.
        let (stored_draft, stored_bundle) = store.latest("profile-alice").unwrap();
        assert_eq!(stored_draft.status, ProfileStatus::Draft);
        assert_eq!(stored_draft.pending_cognitive_candidates.len(), 1);
        assert!(stored_draft.reasoning_frameworks.is_empty());
        assert!(stored_draft.decision_patterns.is_empty());

        // A caller cannot silently omit the Nuwa decision while deciding all
        // ordinary Distilly claims.
        let mut incomplete = crate::cognitive_distillation::review::test_support::decide_all(
            &stored_draft,
            Some(ClaimCategory::ReasoningFrameworks),
        );
        incomplete.cognitive_candidate_decisions.clear();

        assert!(review_in(&store, "profile-alice", &incomplete).is_err());

        // Explicit human acceptance is the promotion boundary.
        let mut decisions = crate::cognitive_distillation::review::test_support::decide_all(
            &stored_draft,
            Some(ClaimCategory::ReasoningFrameworks),
        );

        assert_eq!(decisions.cognitive_candidate_decisions.len(), 1);
        decisions.cognitive_candidate_decisions[0] = CognitiveCandidateDecision {
            candidate_id: "nuwa-mental-model-store-1".to_owned(),
            category: Some(ClaimCategory::ReasoningFrameworks),
            corrected_statement: None,
        };

        let reviewed = review_in(&store, "profile-alice", &decisions).unwrap();

        assert_eq!(reviewed.profile.status, ProfileStatus::Reviewed);
        assert!(reviewed.profile.pending_cognitive_candidates.is_empty());

        let promoted = reviewed
            .profile
            .reasoning_frameworks
            .iter()
            .find(|claim| claim.claim_id == "nuwa-mental-model-store-1")
            .expect("review must promote the accepted Nuwa candidate");

        // Human acceptance permits the inference into the profile but never
        // upgrades model-derived material into directly confirmed fact.
        assert!(!promoted.confirmed);
        assert_eq!(
            promoted.statement,
            "Compares alternatives before committing."
        );

        // Reviewed is still not runnable.
        assert!(persona_skill_in(&store, "profile-alice").is_err());

        let active = activate_in(&store, "profile-alice").unwrap();
        assert_eq!(active.profile.status, ProfileStatus::Active);

        let skill = persona_skill_in(&store, "profile-alice").unwrap();
        assert_eq!(skill.profile_revision, active.profile.revision);
        assert!(skill
            .instructions
            .contains("Compares alternatives before committing. (unconfirmed)"));
        assert!(skill.raw_media_assets.is_empty());

        // Ensure review used the exact stored bundle, not some reconstructed
        // source of truth.
        assert_eq!(stored_bundle.bundle_id, evidence_bundle.bundle_id);
    }

    /// The whole product path, across the storage boundary at every step.
    #[test]
    fn a_profile_goes_from_draft_to_runnable_skill_and_back_again() {
        let (store, _directory) = store();
        let first = bundle("alice-bundle");
        store.append(&drafted(&first), &first).unwrap();

        // Nothing runnable exists while the profile is only a draft.
        assert!(persona_skill_in(&store, "profile-alice").is_err());
        // And a draft cannot be activated straight past review.
        assert!(activate_in(&store, "profile-alice").is_err());

        let (draft, _) = store.latest("profile-alice").unwrap();
        let decisions = decide_all(&draft, Some(ClaimCategory::DecisionPatterns));
        let reviewed = review_in(&store, "profile-alice", &decisions).unwrap();
        assert_eq!(reviewed.profile.status, ProfileStatus::Reviewed);
        assert_eq!(reviewed.active_revision, None);

        let active = activate_in(&store, "profile-alice").unwrap();
        assert_eq!(active.profile.status, ProfileStatus::Active);
        assert_eq!(active.active_revision, Some(1));

        let skill = persona_skill_in(&store, "profile-alice").unwrap();
        assert_eq!(skill.profile_revision, 1);
        assert!(skill.raw_media_assets.is_empty());

        // A second revision opens as a draft and leaves revision 1 live.
        let later = bundle("alice-bundle-2");
        let revised = revise_in(&store, "profile-alice", &later, "new evidence").unwrap();
        assert_eq!(revised.profile.revision, 2);
        assert_eq!(revised.profile.status, ProfileStatus::Draft);
        assert_eq!(
            revised.active_revision,
            Some(1),
            "opening a revision must not take the live profile down"
        );
        // The persona skill still comes from the live revision, not the draft.
        assert_eq!(
            persona_skill_in(&store, "profile-alice")
                .unwrap()
                .profile_revision,
            1
        );

        let (draft_two, _) = store.latest("profile-alice").unwrap();
        let decisions = decide_all(&draft_two, Some(ClaimCategory::Constraints));
        review_in(&store, "profile-alice", &decisions).unwrap();
        let active_two = activate_in(&store, "profile-alice").unwrap();
        assert_eq!(active_two.active_revision, Some(2));

        // Rollback republishes revision 1 as a new revision.
        let rolled_back = rollback_in(&store, "profile-alice", 1).unwrap();
        assert_eq!(rolled_back.active_revision, Some(3));
        assert_eq!(rolled_back.profile.evidence_bundle_id, "alice-bundle");
        assert!(rolled_back
            .profile
            .revision_history
            .last()
            .unwrap()
            .reason
            .contains("rolled back to revision 1"));

        // Everything that happened is still readable.
        assert!(store.history("profile-alice").unwrap().len() >= 7);
    }

    /// Activation takes no caller-supplied review flag, so a caller cannot assert
    /// that review happened. The only way to reach Active is through a review that
    /// decided every drafted claim.
    #[test]
    fn activation_cannot_be_asserted_by_the_caller() {
        let (store, _directory) = store();
        let bundle = bundle("alice-bundle");
        let draft = drafted(&bundle);
        store.append(&draft, &bundle).unwrap();

        let error = activate_in(&store, "profile-alice").unwrap_err();
        assert!(error.contains("reviewed"), "got: {error}");

        // A partial review does not reach Reviewed either.
        let mut decisions = decide_all(&draft, Some(ClaimCategory::Identity));
        decisions.decisions.pop();
        assert!(review_in(&store, "profile-alice", &decisions).is_err());
        assert!(activate_in(&store, "profile-alice").is_err());
    }

    #[test]
    fn an_unknown_profile_is_never_invented_by_any_command() {
        let (store, _directory) = store();
        assert!(view(&store, "profile-nobody").is_err());
        assert!(activate_in(&store, "profile-nobody").is_err());
        assert!(persona_skill_in(&store, "profile-nobody").is_err());
        assert!(rollback_in(&store, "profile-nobody", 1).is_err());
    }
}

#[cfg(test)]
mod handover_tests {
    use super::*;

    /// A toolchain pointing at nothing. Every read will fail, which is exactly
    /// what these tests are about: the entry point's behaviour when files cannot
    /// be read must not depend on any tool being installed.
    fn absent_tools() -> transcription::TranscriptionTools {
        transcription::TranscriptionTools {
            ffmpeg: PathBuf::from("/nonexistent/ffmpeg"),
            whisper: PathBuf::from("/nonexistent/whisper-cli"),
            model: PathBuf::from("/nonexistent/model.bin"),
            tesseract: None,
            ocr_languages: "eng".to_owned(),
            vision: None,
        }
    }

    #[test]
    fn handing_over_nothing_is_refused_rather_than_producing_an_empty_bundle() {
        let error = read_media_with(&[], SubjectKind::SelfProfile, "bundle-1", &absent_tools())
            .unwrap_err();
        assert!(error.contains("No file"), "{error}");
    }

    /// Reading nothing must fail loudly AND name the files, because a bundle
    /// built from zero sources would claim a profile saw material it never saw.
    #[test]
    fn a_handover_where_nothing_could_be_read_fails_and_names_the_files() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("interview.m4a");
        let error = read_media_with(
            &[missing.to_string_lossy().into_owned()],
            SubjectKind::SelfProfile,
            "bundle-1",
            &absent_tools(),
        )
        .unwrap_err();
        assert!(
            error.contains("Nothing handed over could be read"),
            "{error}"
        );
        assert!(error.contains("interview.m4a"), "{error}");
    }

    /// The same file twice used to produce two sources sharing one source_id,
    /// which validation refuses for a reason nobody could act on. It is counted
    /// once, and the duplicate is not reported as a failure either.
    #[test]
    fn the_same_file_handed_over_twice_is_read_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clip.mp4");
        std::fs::write(&path, b"not real media").unwrap();
        let spelled = path.to_string_lossy().into_owned();
        let error = read_media_with(
            &[spelled.clone(), spelled],
            SubjectKind::SelfProfile,
            "bundle-1",
            &absent_tools(),
        )
        .unwrap_err();
        assert_eq!(
            error.matches("clip.mp4").count(),
            1,
            "the duplicate was read a second time: {error}"
        );
    }
}
