use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CreatorAdapterId {
    Distilly,
    HumanDistill,
    AnyoneStyle,
    DistillBlog,
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
            statuses: vec![
                probe_distilly(),
                probe_skill_dir(
                    CreatorAdapterId::HumanDistill,
                    "AI_OS_HUMAN_DISTILL_SKILL_DIR",
                    "licensed Skill directory with SKILL.md",
                ),
                AdapterStatus {
                    id: CreatorAdapterId::AnyoneStyle,
                    availability: AdapterAvailability::ReferenceOnly,
                    capability_probe: "repository-license".to_owned(),
                    reason: Some(
                        "Upstream repository has no valid LICENSE file; workflow reference only."
                            .to_owned(),
                    ),
                },
                AdapterStatus {
                    id: CreatorAdapterId::DistillBlog,
                    availability: AdapterAvailability::ReferenceOnly,
                    capability_probe: "repository-license".to_owned(),
                    reason: Some(
                        "Upstream repository has no valid LICENSE file; workflow reference only."
                            .to_owned(),
                    ),
                },
            ],
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

fn probe_distilly() -> AdapterStatus {
    let mut candidates = std::env::var_os("AI_OS_DISTILLY_SKILL_DIR")
        .map(|path| vec![PathBuf::from(path)])
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.extend([
            home.join(".openclaw/workspace/skills/distilly"),
            home.join(".openclaw/skills/distilly"),
            home.join(".agents/skills/distilly"),
        ]);
    }
    probe_distilly_candidates(candidates)
}

fn probe_distilly_candidates(candidates: Vec<PathBuf>) -> AdapterStatus {
    for directory in candidates {
        let required = [
            directory.join("SKILL.md"),
            directory.join("tools/skill_writer.py"),
            directory.join("tools/version_manager.py"),
        ];
        let skill_contract = fs::read_to_string(&required[0]).unwrap_or_default();
        if required.iter().all(|path| path.is_file())
            && ["create", "update", "rollback"]
                .iter()
                .all(|word| skill_contract.to_ascii_lowercase().contains(word))
        {
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

fn probe_skill_dir(id: CreatorAdapterId, variable: &str, probe: &str) -> AdapterStatus {
    let ready = std::env::var_os(variable)
        .map(PathBuf::from)
        .map(|path| path.join("SKILL.md").is_file())
        .unwrap_or(false);
    AdapterStatus {
        id,
        availability: if ready {
            AdapterAvailability::Ready
        } else {
            AdapterAvailability::Unavailable
        },
        capability_probe: probe.to_owned(),
        reason: (!ready)
            .then(|| "No configured, verified local Skill directory was found.".to_owned()),
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

    #[test]
    fn unlicensed_adapters_are_reference_only() {
        let catalog = AdapterCatalog::probe();
        assert_eq!(
            catalog.availability(CreatorAdapterId::AnyoneStyle),
            AdapterAvailability::ReferenceOnly
        );
        assert_eq!(
            catalog.availability(CreatorAdapterId::DistillBlog),
            AdapterAvailability::ReferenceOnly
        );
    }
}
