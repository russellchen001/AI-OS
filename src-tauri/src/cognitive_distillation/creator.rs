use super::{
    adapters::{AdapterAvailability, AdapterCatalog, CreatorAdapterId},
    import::{draft_profile, import_quarantined_distillation},
    profile::PersonDistillationProfile,
    route_distillation, CognitiveDistillationRouteRequest,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const MAX_CREATOR_INPUT_BYTES: usize = 256 * 1024;
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DistillyCreatorRequest {
    objective: String,
    profile_slug: String,
    display_name: String,
    route: CognitiveDistillationRouteRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuarantinedArtifact {
    relative_path: String,
    bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnrichmentOutcome {
    adapter: CreatorAdapterId,
    executed: bool,
    evidence_added: usize,
    reason: Option<String>,
}

// No `Eq`: a profile carries f32 confidence values.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DistillyCreatorResult {
    status: String,
    adapter: CreatorAdapterId,
    quarantine_root: String,
    artifacts: Vec<QuarantinedArtifact>,
    /// Always a Draft. The creator produces material for review; it never
    /// activates a profile, which is why this and `active_profile_created`
    /// coexist without contradiction.
    draft_profile: PersonDistillationProfile,
    /// What happened to every enrichment pipeline the route selected. The result
    /// must never be narrower than it claims, so each one is reported as executed
    /// or not, with a reason when it was not.
    enrichment: Vec<EnrichmentOutcome>,
    active_profile_created: bool,
}

pub(crate) fn invoke_from_value(
    input: &serde_json::Value,
    operation_id: &str,
) -> Result<serde_json::Value, String> {
    // Resolved from the SAME installation the readiness probe accepted, so AI-OS
    // cannot report one Distilly as ready and then execute another.
    let writer = super::adapters::resolve_distilly_writer().ok_or_else(|| {
        "No usable Distilly installation was found to execute the creator.".to_owned()
    })?;
    invoke_with(
        input,
        operation_id,
        &AdapterCatalog::probe(),
        &writer,
        |session_id, prompt| {
            crate::runtime::agent_skill_transport::invoke_distilly_skill(session_id, prompt)
        },
    )
    .and_then(|result| {
        // Persist the draft so the review lifecycle can continue in a later
        // session. A draft that existed only in the returned JSON would make the
        // creator run unrepeatable work: close the app and the profile is gone.
        // Storage failure is reported rather than swallowed, because a caller that
        // believes a profile was created and finds nothing later is worse than a
        // creator run that says it could not finish.
        let bundle = serde_json::from_value(
            input
                .get("route")
                .and_then(|route| route.get("evidenceBundle"))
                .cloned()
                .ok_or_else(|| "Distilly result carries no evidence bundle to store.".to_owned())?,
        )
        .map_err(|_| "Distilly result evidence bundle is unreadable.".to_owned())?;
        super::store::ProfileStore::open_default()?.append(&result.draft_profile, &bundle)?;
        serde_json::to_value(result).map_err(|_| "Distilly result serialization failed.".to_owned())
    })
}

fn invoke_with(
    input: &serde_json::Value,
    operation_id: &str,
    catalog: &AdapterCatalog,
    writer: &Path,
    invoke: impl Fn(&str, &str) -> Result<(), String>,
) -> Result<DistillyCreatorResult, String> {
    let serialized =
        serde_json::to_vec(input).map_err(|_| "Distilly creator input is invalid.".to_owned())?;
    if serialized.len() > MAX_CREATOR_INPUT_BYTES {
        return Err("Distilly creator input exceeds the bounded evidence limit.".to_owned());
    }
    let request: DistillyCreatorRequest = serde_json::from_slice(&serialized)
        .map_err(|_| "Distilly creator input does not match the canonical contract.".to_owned())?;
    validate_request(&request)?;
    let route = route_distillation(&request.route, catalog).map_err(|error| error.to_string())?;
    if !route.pipelines.iter().any(|pipeline| {
        pipeline.adapter == CreatorAdapterId::Distilly
            && catalog.availability(CreatorAdapterId::Distilly) == AdapterAvailability::Ready
    }) {
        return Err("Distilly is not an eligible ready creator.".to_owned());
    }

    let quarantine = tempfile::Builder::new()
        .prefix("ai-os-distilly-quarantine-")
        .tempdir()
        .map_err(|_| "Could not create the Distilly quarantine.".to_owned())?;
    let root = quarantine.path().to_path_buf();
    let bundle = request.route.evidence_bundle.clone();
    // Every optional pipeline the route selected but cannot execute is reported
    // rather than dropped: a result must never be narrower than it claims. There
    // is no research executor at all — see the note on this module.
    let enrichment_outcomes: Vec<EnrichmentOutcome> = route
        .pipelines
        .iter()
        .filter(|pipeline| pipeline.adapter != CreatorAdapterId::Distilly)
        .map(|pipeline| EnrichmentOutcome {
            adapter: pipeline.adapter,
            executed: false,
            evidence_added: 0,
            reason: Some("No executor is implemented for this enrichment adapter.".to_owned()),
        })
        .collect();

    let evidence = serde_json::to_string(&bundle)
        .map_err(|_| "Could not serialize bounded Distilly evidence.".to_owned())?;
    let prompt = creator_prompt(&request, &root, &evidence, writer);
    let creator_started = std::time::Instant::now();
    let creator_outcome =
        (|| -> Result<(Vec<QuarantinedArtifact>, PersonDistillationProfile), String> {
            invoke(operation_id, &prompt)?;
            let artifacts = inventory(&root)?;
            require_creator_artifacts(&artifacts, &request.profile_slug)?;
            let imported = import_quarantined_distillation(&root, &request.profile_slug)?;
            let drafted = draft_profile(
                &imported,
                &bundle,
                &format!("profile-{}", request.profile_slug),
            )?;
            Ok((artifacts, drafted))
        })();
    let (artifacts, drafted) = creator_outcome.map_err(|error| {
        format!(
            "{error} [creator turn ran {}s; optional pipelines: {}; evidence carried {} item(s)]",
            creator_started.elapsed().as_secs(),
            describe_enrichment(&enrichment_outcomes),
            bundle.evidence.len()
        )
    })?;
    // Importing before the quarantine is persisted means a creator run that
    // produced unusable artifacts leaves nothing behind.
    let persisted_root = quarantine.keep();

    Ok(DistillyCreatorResult {
        status: "quarantined".to_owned(),
        adapter: CreatorAdapterId::Distilly,
        quarantine_root: persisted_root.to_string_lossy().into_owned(),
        artifacts,
        draft_profile: drafted,
        enrichment: enrichment_outcomes,
        active_profile_created: false,
    })
}

fn validate_request(request: &DistillyCreatorRequest) -> Result<(), String> {
    if request.objective.trim().is_empty()
        || request.objective.len() > 1_024
        || !display_name_is_safe(&request.display_name)
        || request.profile_slug.is_empty()
        || request.profile_slug.len() > 40
        || !request
            .profile_slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("Distilly creator request metadata is invalid.".to_owned());
    }
    Ok(())
}

/// The display name is interpolated into a double-quoted argument of a command
/// the creator Agent is told to execute on the host. A name carrying a quote,
/// backslash, backtick, dollar sign or control character could end that argument
/// early and change the command, so those are refused rather than escaped.
/// Everything else, including spaces and non-Latin scripts, stays acceptable.
fn display_name_is_safe(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 128
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '"' | '\\' | '`' | '$'))
}

fn creator_prompt(
    request: &DistillyCreatorRequest,
    root: &Path,
    evidence: &str,
    writer: &Path,
) -> String {
    let work_input = root.join("work-input.md");
    let persona_input = root.join("persona-input.md");
    let profiles = root.join("profiles");

    format!(
        r#"This is an AI-OS controlled Cognitive Distillation creator invocation.

Use ONLY the bounded evidence below.

Do not:
- search for another Distilly installation
- inspect ~/.agents or other OpenClaw workspaces
- modify PYTHONPATH
- use sys.path.insert
- use network tools
- use collectors
- use Xquik
- use credentials
- perform public research
- install or register the resulting Skill
- write outside QUARANTINE_ROOT

The Distilly creator entrypoint has already been resolved and validated by AI-OS.

DISTILLY_WRITER={writer}
QUARANTINE_ROOT={root}
WORK_INPUT={work_input}
PERSONA_INPUT={persona_input}
PROFILE_OUTPUT={profiles}

First distill the bounded evidence into exactly two non-empty UTF-8 Markdown inputs:

1. WORK_INPUT
   Capture demonstrated work methods, decision heuristics, reasoning patterns,
   constraints, expertise and the stated exception/conflict.
   Do not invent unsupported facts.

2. PERSONA_INPUT
   Capture demonstrated communication preference, behavioral tendencies and
   interaction style.
   Preserve uncertainty and the stated conflict.
   Do not infer sensitive attributes.

Both files MUST remain below QUARANTINE_ROOT.

After both files are non-empty, execute EXACTLY the validated Distilly CLI entrypoint:

python3 "{writer}" \
  --action create \
  --character {character} \
  --slug "{slug}" \
  --name "{name}" \
  --work "{work_input}" \
  --persona "{persona_input}" \
  --base-dir "{profiles}" \
  --no-install-claude-skill

Do not replace this command with Python -c.
Do not import skill_writer as a Python module.
Do not search for skill_writer.py.
Do not use installation flags for OpenClaw, Codex or another host.

After the command succeeds, verify that these files exist and are non-empty below:
{profiles}/{slug}/

- SKILL.md
- manifest.json
- meta.json
- persona.md
- work.md

Then return one short completion message.

Objective:
{objective}

BOUNDED_EVIDENCE_JSON_START
{evidence}
BOUNDED_EVIDENCE_JSON_END
"#,
        writer = writer.display(),
        root = root.display(),
        work_input = work_input.display(),
        persona_input = persona_input.display(),
        profiles = profiles.display(),
        character = super::import::DISTILLY_CHARACTER,
        slug = request.profile_slug,
        name = request.display_name,
        objective = request.objective.trim(),
        evidence = evidence,
    )
}

fn inventory(root: &Path) -> Result<Vec<QuarantinedArtifact>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        output: &mut Vec<QuarantinedArtifact>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|_| "Could not inspect Distilly quarantine.".to_owned())?
        {
            let entry = entry.map_err(|_| "Could not inspect Distilly quarantine.".to_owned())?;
            let file_type = entry
                .file_type()
                .map_err(|_| "Could not inspect Distilly quarantine.".to_owned())?;
            if file_type.is_symlink() {
                return Err("Distilly quarantine contains a forbidden symbolic link.".to_owned());
            }
            if file_type.is_dir() {
                visit(root, &entry.path(), output)?;
            } else if file_type.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| "Distilly artifact escaped quarantine.".to_owned())?
                    .to_string_lossy()
                    .into_owned();
                let bytes = entry
                    .metadata()
                    .map_err(|_| "Could not inspect Distilly artifact.".to_owned())?
                    .len();
                output.push(QuarantinedArtifact {
                    relative_path: relative,
                    bytes,
                });
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    visit(root, root, &mut output)?;
    output.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(output)
}

/// Renders the optional pipelines for a human reading a failure, so a creator
/// error never hides what else the route selected or why it did nothing.
fn describe_enrichment(outcomes: &[EnrichmentOutcome]) -> String {
    if outcomes.is_empty() {
        return "none were routed".to_owned();
    }
    outcomes
        .iter()
        .map(|outcome| {
            format!(
                "{:?} executed={} evidenceAdded={}{}",
                outcome.adapter,
                outcome.executed,
                outcome.evidence_added,
                outcome
                    .reason
                    .as_deref()
                    .map(|reason| format!(" reason={reason}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn require_creator_artifacts(artifacts: &[QuarantinedArtifact], slug: &str) -> Result<(), String> {
    let required = [
        "SKILL.md",
        "manifest.json",
        "meta.json",
        "persona.md",
        "work.md",
    ];
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|name| {
            !artifacts.iter().any(|artifact| {
                artifact.relative_path == format!("profiles/{slug}/{name}") && artifact.bytes > 0
            })
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    // Naming what is missing AND what did land separates "the writer never ran"
    // from "the writer ran under a different slug" from "the writer wrote empty
    // files" — three different faults that the old one-line error merged into one.
    let found = if artifacts.is_empty() {
        "nothing".to_owned()
    } else {
        artifacts
            .iter()
            .map(|artifact| format!("{} ({}B)", artifact.relative_path, artifact.bytes))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(format!(
        "Distilly did not produce the required quarantined artifacts for slug {slug}. Missing or empty: {}. The quarantine holds: {found}.",
        missing.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_distillation::{
        adapters::AdapterStatus,
        evidence::{normalize_extraction, test_support::*, EvidenceAssertion, EvidenceBundle},
        SourceKind, SourceMediaKind, SubjectKind,
    };

    fn fixture() -> serde_json::Value {
        fixture_with(SubjectKind::FictionalCharacter)
    }

    fn fixture_with(subject: SubjectKind) -> serde_json::Value {
        let observations = [
            (
                "decision",
                "Alice compares downside risk before choosing a plan.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "communication",
                "Alice prefers concise bullet points and a direct recommendation.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "pace",
                "Alice usually pauses to verify assumptions before acting.",
                EvidenceAssertion::Confirmed,
            ),
            (
                "conflict",
                "In one urgent simulation Alice acted before verifying every assumption.",
                EvidenceAssertion::Contradictory,
            ),
        ];
        let mut sources = Vec::new();
        let mut evidence = Vec::new();
        for (index, (label, text, assertion)) in observations.into_iter().enumerate() {
            let source = source(
                &format!("alice-{label}-source"),
                SourceMediaKind::Text,
                SourceKind::UserFile,
                false,
            );
            let mut extracted = item(&format!("alice-evidence-{index}"), SourceMediaKind::Text);
            extracted.extracted_text = Some(text.to_owned());
            extracted.assertion = assertion;
            evidence.extend(
                normalize_extraction(subject, &source, &local_policy(), extraction(extracted))
                    .unwrap(),
            );
            sources.push(source);
        }
        serde_json::json!({
            "objective": "Create a synthetic Alice profile",
            "profileSlug": "alice-synthetic",
            "displayName": "Alice",
            "route": {
                "subjectKind": subject,
                "evidenceBundle": EvidenceBundle {
                    bundle_id: "alice-bundle".to_owned(),
                    subject_kind: subject,
                    media_kind: SourceMediaKind::Text,
                    sources,
                    evidence,
                },
                "articleHeavy": false,
                "highAssuranceEvidence": false
            }
        })
    }

    /// The unit tests never execute the writer; they only need a path the prompt
    /// can name. Resolution against a real installation is the real entry's job.
    fn test_writer() -> std::path::PathBuf {
        std::path::PathBuf::from("/test/distilly/tools/skill_writer.py")
    }

    fn ready_catalog() -> AdapterCatalog {
        AdapterCatalog::for_test(vec![AdapterStatus {
            id: CreatorAdapterId::Distilly,
            availability: AdapterAvailability::Ready,
            capability_probe: "test".to_owned(),
            reason: None,
        }])
    }

    fn quarantine_root_from(prompt: &str) -> std::path::PathBuf {
        let marker = "QUARANTINE_ROOT=";
        let start = prompt.find(marker).unwrap() + marker.len();
        std::path::PathBuf::from(prompt[start..].split_whitespace().next().unwrap())
    }

    /// Shaped after the real artifacts a Distilly `create` writes. The provenance
    /// fields are the ones `require_distilly_provenance` reads; the rest of the
    /// real files is deliberately not reproduced, because acceptance must not
    /// depend on incidental Distilly output.
    fn write_writer_shaped_artifacts(root: &Path, slug: &str) {
        let output = root.join("profiles").join(slug);
        fs::create_dir_all(&output).unwrap();
        let id = format!("meta-skill.colleague.{slug}");
        fs::write(
            output.join("manifest.json"),
            serde_json::json!({
                "manifest_version": "1",
                "id": id,
                "kind": "meta-skill",
                "character": "colleague",
                "engine": {"name": "distilly", "kind": "meta-skill"},
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
                "generation": {"engine": "distilly"},
                "engine": {"name": "distilly"},
            })
            .to_string(),
        )
        .unwrap();
        for name in ["SKILL.md", "persona.md", "work.md"] {
            fs::write(output.join(name), "distilled content").unwrap();
        }
    }

    #[test]
    fn creator_invocation_accepts_only_quarantined_distilly_artifacts() {
        let result = invoke_with(
            &fixture(),
            "test-operation",
            &ready_catalog(),
            &test_writer(),
            |_, prompt| {
                write_writer_shaped_artifacts(&quarantine_root_from(prompt), "alice-synthetic");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result.status, "quarantined");
        assert!(!result.active_profile_created);
        assert_eq!(result.artifacts.len(), 5);

        // The creator now completes the pipeline as far as a reviewable draft, and
        // no further: the profile is Draft, its subject identity came from the
        // AI-OS bundle rather than from Distilly, and nothing is categorised yet.
        let drafted = &result.draft_profile;
        assert_eq!(
            drafted.status,
            crate::cognitive_distillation::profile::ProfileStatus::Draft
        );
        assert_eq!(drafted.subject_kind, SubjectKind::FictionalCharacter);
        assert_eq!(drafted.profile_id, "profile-alice-synthetic");
        assert!(drafted.identity.is_empty());
        assert!(!drafted.unclassified_claims.is_empty());
        assert!(!drafted.draft_narrative.is_empty());

        fs::remove_dir_all(result.quarantine_root).unwrap();
    }

    /// The acceptance question CD-1B-1 actually asks is whether the validated
    /// Distilly CLI ran, not whether five files exist. An Agent that skipped the
    /// CLI and wrote the five names itself must be refused.
    /// Research is now executed rather than dropped: its checked claims join
    /// the bundle the creator distils from, and the result says how many.

    /// Enrichment failure must not fail the creator, and must not be silent
    /// either: the run continues on the evidence already authorised, and says so.

    #[test]
    fn artifacts_without_distilly_provenance_are_refused() {
        // Plain files under the right names: refused because the JSON artifacts
        // are not even JSON.
        let error = invoke_with(
            &fixture(),
            "test-operation",
            &ready_catalog(),
            &test_writer(),
            |_, prompt| {
                let output = quarantine_root_from(prompt).join("profiles/alice-synthetic");
                fs::create_dir_all(&output).unwrap();
                for name in [
                    "SKILL.md",
                    "manifest.json",
                    "meta.json",
                    "persona.md",
                    "work.md",
                ] {
                    fs::write(output.join(name), "synthetic artifact").unwrap();
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(
            error.contains("validated writer"),
            "expected a writer-provenance refusal, got: {error}"
        );

        // Well-formed JSON that simply does not claim the Distilly engine: refused
        // by the provenance check itself, which is the case that matters most,
        // because it is what a capable Agent would produce if it skipped the CLI.
        let error = invoke_with(
            &fixture(),
            "test-operation",
            &ready_catalog(),
            &test_writer(),
            |_, prompt| {
                let output = quarantine_root_from(prompt).join("profiles/alice-synthetic");
                fs::create_dir_all(&output).unwrap();
                fs::write(
                    output.join("manifest.json"),
                    serde_json::json!({"manifest_version": "1", "kind": "meta-skill"}).to_string(),
                )
                .unwrap();
                fs::write(
                    output.join("meta.json"),
                    serde_json::json!({"schema_version": "3", "slug": "alice-synthetic"})
                        .to_string(),
                )
                .unwrap();
                for name in ["SKILL.md", "persona.md", "work.md"] {
                    fs::write(output.join(name), "content").unwrap();
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(
            error.contains("provenance"),
            "expected a provenance refusal, got: {error}"
        );
    }

    /// Artifacts carrying a different identity than the one AI-OS asked for are a
    /// different profile, not this one.
    #[test]
    fn artifacts_claiming_another_identity_are_refused() {
        let error = invoke_with(
            &fixture(),
            "test-operation",
            &ready_catalog(),
            &test_writer(),
            |_, prompt| {
                let root = quarantine_root_from(prompt);
                write_writer_shaped_artifacts(&root, "alice-synthetic");
                let meta = root.join("profiles/alice-synthetic/meta.json");
                let mut value: serde_json::Value =
                    serde_json::from_str(&fs::read_to_string(&meta).unwrap()).unwrap();
                value["slug"] = serde_json::json!("someone-else");
                fs::write(&meta, value.to_string()).unwrap();
                Ok(())
            },
        )
        .unwrap_err();
        assert!(
            error.contains("provenance"),
            "expected a provenance refusal, got: {error}"
        );
    }

    /// The display name reaches a double-quoted argument of a command the creator
    /// Agent executes on the host, so a name that could end that argument early is
    /// refused before any prompt is built.
    #[test]
    fn a_display_name_that_could_break_the_writer_command_is_refused() {
        for hostile in [
            "Alice\" ; rm -rf ~ ; echo \"",
            "Alice`whoami`",
            "Alice$(whoami)",
            "Alice\\",
            "Alice\nBob",
        ] {
            let mut input = fixture();
            input["displayName"] = serde_json::json!(hostile);
            let error = invoke_with(
                &input,
                "test-operation",
                &ready_catalog(),
                &test_writer(),
                |_, _| panic!("a hostile display name must be refused before the Agent is invoked"),
            )
            .unwrap_err();
            assert!(
                error.contains("metadata is invalid"),
                "expected {hostile:?} to be refused, got: {error}"
            );
        }

        for acceptable in ["Alice", "Alice O'Brien", "陈小明", "Anna-Maria Schmidt"] {
            let mut input = fixture();
            input["displayName"] = serde_json::json!(acceptable);
            let result = invoke_with(
                &input,
                "test-operation",
                &ready_catalog(),
                &test_writer(),
                |_, prompt| {
                    write_writer_shaped_artifacts(&quarantine_root_from(prompt), "alice-synthetic");
                    Ok(())
                },
            )
            .unwrap();
            fs::remove_dir_all(result.quarantine_root).unwrap();
        }
    }

    #[test]
    fn failed_invocation_does_not_return_or_persist_an_artifact_root() {
        let error = invoke_with(
            &fixture(),
            "test-operation",
            &ready_catalog(),
            &test_writer(),
            |_, _| Err("transport failed".to_owned()),
        )
        .unwrap_err();
        // The transport's own words survive, and the run context travels with
        // them. Asserting equality here would forbid exactly the context that a
        // half-hour two-turn failure needs in order to be readable.
        assert!(error.starts_with("transport failed"), "{error}");
        assert!(error.contains("creator turn ran"), "{error}");
        assert!(error.contains("optional pipelines:"), "{error}");
    }

    /// Prove the public-person fixture's own premise before any real run spends a
    /// web search on it. A fixture bug discovered AFTER a real search about a real
    /// person is a bug discovered too late.

    /// Isolates the research half. The full smoke runs two OpenClaw turns and can
    /// take half an hour; when it fails, this probe says which half failed without
    /// paying for the other one. It reports rather than asserts, because "the
    /// research turn produced nothing, and here is what it said" is exactly the
    /// observation worth having. Same boundaries as the full smoke: quick mode,
    /// public sources, nothing activated.

    /// The second real smoke: the research-enrichment path, end to end.
    ///
    /// Unlike the Distilly smoke, this one causes a REAL public web search about a
    /// REAL named person, so it is gated behind its own environment variable and
    /// never runs as part of any suite. The subject was chosen by the owner.
    ///
    /// It stays inside the boundaries the adapter enforces: quick mode only, so no
    /// browser, no Douyin, no login; public sources only; and the result is a
    /// quarantined Draft that is never activated.

    #[test]
    #[ignore = "requires installed Distilly and a reachable local OpenClaw gateway"]
    fn real_distilly_creator_invocation_writes_only_quarantine() {
        assert_eq!(
            std::env::var("AI_OS_RUN_DISTILLY_REAL_SMOKE").as_deref(),
            Ok("1")
        );
        // The operation id becomes OpenClaw's idempotency key. A fixed id makes
        // every later run replay the FIRST run's terminal record, so a smoke that
        // once failed can never succeed again and the failure looks live. A
        // per-run id keeps each smoke a real invocation.
        let operation_id = format!(
            "cd1b1-real-smoke-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock must be after the epoch")
                .as_millis()
        );
        let result = match invoke_from_value(&fixture(), &operation_id) {
            Ok(value) => value,
            Err(error) => panic!("DISTILLY_REAL_SMOKE_FAILED operation_id={operation_id}: {error}"),
        };
        println!(
            "DISTILLY_REAL_RESULT={}",
            serde_json::to_string(&result).unwrap()
        );
    }
}
