use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CreatorAdapterId {
    Distilly,
    Nuwa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AdapterAvailability {
    Ready,
    Unavailable,
    ReferenceOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdapterStatus {
    pub id: CreatorAdapterId,
    pub availability: AdapterAvailability,
    pub capability_probe: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtractorStatus {
    pub id: String,
    pub availability: AdapterAvailability,
    pub local: bool,
    pub capability_probe: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AdapterCatalog {
    statuses: Vec<AdapterStatus>,
}

impl AdapterCatalog {
    pub(crate) fn probe() -> Self {
        Self {
            statuses: vec![probe_distilly(), probe_nuwa()],
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(statuses: Vec<AdapterStatus>) -> Self {
        Self { statuses }
    }

    pub(crate) fn statuses(&self) -> Vec<AdapterStatus> {
        self.statuses.clone()
    }

    pub(crate) fn availability(&self, id: CreatorAdapterId) -> AdapterAvailability {
        self.statuses
            .iter()
            .find(|status| status.id == id)
            .map(|status| status.availability)
            .unwrap_or(AdapterAvailability::Unavailable)
    }
}

/// Where a Distilly installation may live, in the order AI-OS should prefer.
///
/// `workspace-ai-os-files` comes first because that is the workspace of the
/// dedicated OpenClaw execution Agent — the only copy the Agent that runs the
/// writer can actually reach. Probing a copy the executor cannot reach would let
/// AI-OS report Ready about one installation and then run a different one.
fn distilly_candidates() -> Vec<PathBuf> {
    let mut candidates = std::env::var_os("AI_OS_DISTILLY_SKILL_DIR")
        .map(|path| vec![PathBuf::from(path)])
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.extend([
            home.join(".openclaw/workspace-ai-os-files/skills/distilly"),
            home.join(".openclaw/workspace/skills/distilly"),
            home.join(".openclaw/skills/distilly"),
            home.join(".agents/skills/distilly"),
        ]);
    }
    candidates
}

/// True when this directory is a Distilly installation AI-OS can use.
fn distilly_installation_is_usable(directory: &Path) -> bool {
    let required = [
        directory.join("SKILL.md"),
        directory.join("tools/skill_writer.py"),
        directory.join("tools/version_manager.py"),
    ];
    let skill_contract = fs::read_to_string(&required[0]).unwrap_or_default();
    required.iter().all(|path| path.is_file())
        && ["create", "update", "rollback"]
            .iter()
            .all(|word| skill_contract.to_ascii_lowercase().contains(word))
}

/// The writer the creator must execute, resolved from the SAME installation the
/// readiness probe accepts.
///
/// The probe and the executor share this resolver on purpose. They used to
/// disagree: readiness was probed against `~/.openclaw/workspace/...` while the
/// creator prompt hardcoded `~/.openclaw/workspace-ai-os-files/...`. That worked
/// only while both copies happened to exist, and would have failed silently and
/// confusingly the moment one was removed.
pub(crate) fn resolve_distilly_writer() -> Option<PathBuf> {
    distilly_candidates()
        .into_iter()
        .find(|directory| distilly_installation_is_usable(directory))
        .map(|directory| directory.join("tools").join("skill_writer.py"))
}

fn probe_distilly() -> AdapterStatus {
    probe_distilly_candidates(distilly_candidates())
}

fn probe_distilly_candidates(candidates: Vec<PathBuf>) -> AdapterStatus {
    for directory in candidates {
        if distilly_installation_is_usable(&directory) {
            return AdapterStatus {
                id: CreatorAdapterId::Distilly,
                availability: AdapterAvailability::Ready,
                capability_probe: "installed-skill:create+update+rollback".to_owned(),
                reason: None,
            };
        }
    }
    AdapterStatus {
        id: CreatorAdapterId::Distilly,
        availability: AdapterAvailability::Unavailable,
        capability_probe: "installed-skill:create+update+rollback".to_owned(),
        reason: Some(
            "A capability-compatible local Distilly Skill installation was not found.".to_owned(),
        ),
    }
}

/// Nuwa is deliberately probed as an analysis Skill only.
///
/// AI-OS owns source ingestion, EvidenceBundle construction, provenance,
/// quarantine and profile authority. Finding Nuwa here does NOT authorize
/// autonomous web research or direct profile activation.
fn nuwa_candidates() -> Vec<PathBuf> {
    let mut candidates = std::env::var_os("AI_OS_NUWA_SKILL_DIR")
        .map(|path| vec![PathBuf::from(path)])
        .unwrap_or_default();

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.extend([
            home.join(".openclaw/workspace-ai-os-files/skills/nuwa-skill"),
            home.join(".openclaw/workspace/skills/nuwa-skill"),
            home.join(".openclaw/skills/nuwa-skill"),
            home.join(".agents/skills/nuwa-skill"),
        ]);
    }

    candidates
}

fn nuwa_installation_is_usable(directory: &Path) -> bool {
    let required = [
        directory.join("SKILL.md"),
        directory.join("LICENSE"),
        directory.join("references/extraction-framework.md"),
        directory.join("references/fidelity-scorecard.md"),
        directory.join("references/skill-template.md"),
    ];

    if !required.iter().all(|path| path.is_file()) {
        return false;
    }

    let skill = fs::read_to_string(&required[0]).unwrap_or_default();
    let framework = fs::read_to_string(&required[2]).unwrap_or_default();

    skill.contains("name: huashu-nuwa")
        && skill.contains("本地语料")
        && skill.contains("心智模型")
        && skill.contains("决策启发式")
        && framework.contains("跨域复现")
        && framework.contains("有生成力")
        && framework.contains("有排他性")
}

fn probe_nuwa() -> AdapterStatus {
    probe_nuwa_candidates(nuwa_candidates())
}

/// Resolve the exact local Nuwa installation accepted by the capability probe.
/// Execution must use the same installation readiness semantics as status probing.
pub(crate) fn resolve_nuwa_directory() -> Option<PathBuf> {
    nuwa_candidates()
        .into_iter()
        .find(|directory| nuwa_installation_is_usable(directory))
}

fn probe_nuwa_candidates(candidates: Vec<PathBuf>) -> AdapterStatus {
    for directory in candidates {
        if nuwa_installation_is_usable(&directory) {
            return AdapterStatus {
                id: CreatorAdapterId::Nuwa,
                availability: AdapterAvailability::Ready,
                capability_probe:
                    "installed-skill:local-corpus+cognitive-framework+fidelity-validation"
                        .to_owned(),
                reason: None,
            };
        }
    }

    AdapterStatus {
        id: CreatorAdapterId::Nuwa,
        availability: AdapterAvailability::Unavailable,
        capability_probe: "installed-skill:local-corpus+cognitive-framework+fidelity-validation"
            .to_owned(),
        reason: Some(
            "A capability-compatible local Nuwa Skill installation was not found.".to_owned(),
        ),
    }
}

fn command_status(id: &str, command: &str, arguments: &[&str], probe: &str) -> ExtractorStatus {
    let ready = Command::new(command)
        .args(arguments)
        .env_remove("OPENAI_API_KEY")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    ExtractorStatus {
        id: id.to_owned(),
        availability: if ready { AdapterAvailability::Ready } else { AdapterAvailability::Unavailable },
        local: true,
        capability_probe: probe.to_owned(),
        reason: (!ready).then(|| format!("{id} capability probe did not pass; AI-OS will not download a runtime or model automatically.")),
    }
}

pub(crate) fn probe_asr_adapters() -> Vec<ExtractorStatus> {
    vec![
        command_status(
            "whisper.cpp",
            "whisper-cli",
            &["--help"],
            "timestamped local transcription",
        ),
        command_status(
            "mlx-whisper",
            "mlx_whisper",
            &["--help"],
            "timestamped local transcription",
        ),
    ]
}

pub(crate) fn probe_ffmpeg() -> ExtractorStatus {
    command_status(
        "ffmpeg",
        "ffmpeg",
        &["-version"],
        "audio demux and bounded frame extraction",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nuwa_compatibility_uses_cognitive_capabilities_not_version() {
        let root = tempfile::tempdir().unwrap();
        let references = root.path().join("references");
        fs::create_dir(&references).unwrap();

        fs::write(
            root.path().join("SKILL.md"),
            "name: huashu-nuwa\n本地语料\n心智模型\n决策启发式\n",
        )
        .unwrap();

        fs::write(root.path().join("LICENSE"), "MIT").unwrap();

        fs::write(
            references.join("extraction-framework.md"),
            "跨域复现\n有生成力\n有排他性\n",
        )
        .unwrap();

        fs::write(
            references.join("fidelity-scorecard.md"),
            "fidelity scorecard",
        )
        .unwrap();

        fs::write(references.join("skill-template.md"), "skill template").unwrap();

        let status = probe_nuwa_candidates(vec![root.path().to_path_buf()]);

        assert_eq!(status.id, CreatorAdapterId::Nuwa);
        assert_eq!(status.availability, AdapterAvailability::Ready);
        assert!(!status.capability_probe.contains("fe037468"));
    }

    #[test]
    fn distilly_compatibility_uses_capabilities_not_version() {
        let root = tempfile::tempdir().unwrap();
        let tools = root.path().join("tools");
        fs::create_dir(&tools).unwrap();
        fs::write(root.path().join("SKILL.md"), "create update rollback").unwrap();
        fs::write(tools.join("skill_writer.py"), "writer").unwrap();
        fs::write(tools.join("version_manager.py"), "manager").unwrap();

        let status = probe_distilly_candidates(vec![root.path().to_path_buf()]);
        assert_eq!(status.availability, AdapterAvailability::Ready);
        assert!(!status.capability_probe.contains("1.0"));
    }
}
