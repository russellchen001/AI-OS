//! What a person's machine needs before it can distil a given kind of file.
//!
//! Three facts shape this module.
//!
//! **Most people download nothing.** Distilling chat logs, documents and
//! screenshots needs no model at all. A download is only ever proposed when
//! someone actually hands over the kind of file that requires one, so a user who
//! never touches audio never sees a gigabyte of anything.
//!
//! **The machine decides the size, not the developer.** A laptop with 8 GB of
//! memory gets the compact models; a larger machine gets the accurate ones. The
//! difference is roughly 1.5 GB against 2.6 GB for the full set.
//!
//! **Installed is not Ready.** An asset counts as present only once its sha256
//! matches, and a tier counts as usable only once its engine has actually
//! produced output. That is the same rule GM-3 already applies to ComfyUI, and it
//! is what stops a half-installed toolchain from being reported as working.

use crate::managed_assets::{ensure_asset, file_matches_spec, ManagedAssetSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What a file needs before it can be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MediaTier {
    /// Text, documents, chat logs. Nothing to install.
    Text,
    /// Screenshots and subtitled video: bundled binaries only, no download.
    ScreenText,
    /// Anything with speech.
    Speech,
    /// A picture with no legible text in it.
    Picture,
}

impl MediaTier {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::ScreenText => "screen text",
            Self::Speech => "speech",
            Self::Picture => "pictures",
        }
    }
}

/// Which size of model this machine should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ModelProfile {
    Compact,
    Accurate,
}

impl ModelProfile {
    /// Chosen from installed memory. The models are memory-bound in practice, and
    /// a 2 GB vision model on an 8 GB machine competes with everything the person
    /// is actually doing.
    pub(crate) fn for_this_machine() -> Self {
        let memory = sysinfo::System::new_all().total_memory();
        if memory >= 24 * 1024 * 1024 * 1024 {
            Self::Accurate
        } else {
            Self::Compact
        }
    }
}

const WHISPER_HOST: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const VLM_HOST: &str = "https://huggingface.co/ggml-org/InternVL3-2B-Instruct-GGUF/resolve/main";

/// Every managed asset a tier needs.
///
/// Sizes and hashes come from the publishers' own file metadata. They are pinned
/// rather than fetched, so an asset that changes upstream fails verification
/// loudly instead of being installed silently.
pub(crate) fn assets_for(tier: MediaTier, profile: ModelProfile) -> Vec<ManagedAssetSpec> {
    match (tier, profile) {
        (MediaTier::Text | MediaTier::ScreenText, _) => Vec::new(),

        (MediaTier::Speech, ModelProfile::Compact) => vec![spec(
            WHISPER_HOST,
            "ggml-small-q5_1.bin",
            "whisper",
            190_085_487,
            "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        )],
        (MediaTier::Speech, ModelProfile::Accurate) => vec![spec(
            WHISPER_HOST,
            "ggml-large-v3-turbo-q5_0.bin",
            "whisper",
            574_041_195,
            "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        )],

        (MediaTier::Picture, ModelProfile::Compact) => vec![
            spec(
                VLM_HOST,
                "InternVL3-2B-Instruct-Q4_K_M.gguf",
                "vlm",
                1_116_758_816,
                "dc36eddc05ff1db5e11e0aa38efe7a5063b045aa5350c3b1a2510f3ff9107179",
            ),
            spec(
                VLM_HOST,
                "mmproj-InternVL3-2B-Instruct-Q8_0.gguf",
                "vlm",
                337_012_000,
                "a91c525291f65b0469f82544e82f202c4df1c093268f0ce27deec10664078a0c",
            ),
        ],
        (MediaTier::Picture, ModelProfile::Accurate) => vec![
            spec(
                VLM_HOST,
                "InternVL3-2B-Instruct-Q8_0.gguf",
                "vlm",
                1_893_671_520,
                "b09d7858d8b111f103a38b4ac333c8d41ca6faf34da7d4c3f76c646318004410",
            ),
            spec(
                VLM_HOST,
                "mmproj-InternVL3-2B-Instruct-Q8_0.gguf",
                "vlm",
                337_012_000,
                "a91c525291f65b0469f82544e82f202c4df1c093268f0ce27deec10664078a0c",
            ),
        ],
    }
}

fn spec(host: &str, file: &str, folder: &str, size_bytes: u64, sha256: &str) -> ManagedAssetSpec {
    ManagedAssetSpec {
        source_url: format!("{host}/{file}?download=true"),
        relative_path: format!("{folder}/{file}"),
        size_bytes,
        sha256: sha256.to_owned(),
    }
}

/// Where managed assets live.
///
/// The platform cache directory, which is `~/Library/Caches` on macOS and
/// `~/.cache` elsewhere. Anything that downloads these assets from outside the
/// app MUST resolve the same way: fetching 2.5 GB into `~/.cache` on a Mac
/// produces files the app then cannot find, and a person left wondering why
/// nothing happened.
pub(crate) fn asset_root() -> Option<PathBuf> {
    dirs::cache_dir().map(|cache| cache.join("ai-os"))
}

/// What is still missing before a tier can be used, in terms a person can act on.
///
/// Deliberately NOT a developer instruction. "Install it with `brew install
/// whisper-cpp`" is a sentence for whoever is building AI-OS, not for someone who
/// has just dragged a voice memo into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TierRequirement {
    pub tier: MediaTier,
    pub ready: bool,
    /// Bytes still to fetch. Zero when everything is already present.
    pub download_bytes: u64,
    pub profile: ModelProfile,
    /// One sentence for the person, or none when nothing is needed.
    pub message: Option<String>,
}

pub(crate) fn requirement(tier: MediaTier, profile: ModelProfile) -> TierRequirement {
    let root = asset_root();
    let mut outstanding = 0_u64;

    for asset in assets_for(tier, profile) {
        let present = root
            .as_ref()
            .map(|root| root.join(&asset.relative_path))
            .is_some_and(|path| file_matches_spec(&path, &asset).unwrap_or(false));
        if !present {
            outstanding = outstanding.saturating_add(asset.size_bytes);
        }
    }

    let ready = outstanding == 0;
    TierRequirement {
        tier,
        ready,
        download_bytes: outstanding,
        profile,
        message: (!ready).then(|| {
            format!(
                "Reading {} needs a one-time {} download. It runs entirely on this machine and nothing is sent anywhere.",
                tier.label(),
                human_size(outstanding)
            )
        }),
    }
}

/// Fetch whatever a tier is missing. Called only after a person has agreed.
pub(crate) fn install(tier: MediaTier, profile: ModelProfile) -> Result<(), String> {
    let root = asset_root().ok_or_else(|| "No cache directory is available.".to_owned())?;
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|_| "A download client could not be created.".to_owned())?;

    for asset in assets_for(tier, profile) {
        let destination = root.join(&asset.relative_path);
        ensure_asset(&client, &asset, &destination)?;
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mib = bytes as f64 / MIB;
    if mib >= 1024.0 {
        format!("{:.1} GB", mib / 1024.0)
    } else {
        format!("{mib:.0} MB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the whole tiering scheme: someone distilling chat logs and
    /// screenshots is never asked to download anything.
    #[test]
    fn text_and_screenshots_need_no_download_on_any_machine() {
        for profile in [ModelProfile::Compact, ModelProfile::Accurate] {
            for tier in [MediaTier::Text, MediaTier::ScreenText] {
                assert!(assets_for(tier, profile).is_empty());
                let requirement = requirement(tier, profile);
                assert!(requirement.ready);
                assert_eq!(requirement.download_bytes, 0);
                assert!(requirement.message.is_none());
            }
        }
    }

    /// A smaller machine must be offered materially less to download, or the
    /// hardware profile is decoration.
    #[test]
    fn a_smaller_machine_is_asked_for_materially_less() {
        let total = |profile| {
            [MediaTier::Speech, MediaTier::Picture]
                .into_iter()
                .flat_map(|tier| assets_for(tier, profile))
                .map(|asset| asset.size_bytes)
                .sum::<u64>()
        };
        let compact = total(ModelProfile::Compact);
        let accurate = total(ModelProfile::Accurate);

        assert!(compact < accurate);
        // Roughly 1.5 GB against 2.6 GB; anything less than a third saved would
        // not be worth splitting the profiles over.
        assert!(
            accurate - compact > 700 * 1024 * 1024,
            "{compact} vs {accurate}"
        );
    }

    /// Every managed asset must carry a real pinned hash and a plausible size, or
    /// verification is theatre.
    #[test]
    fn every_asset_is_pinned_by_hash_and_size() {
        for profile in [ModelProfile::Compact, ModelProfile::Accurate] {
            for tier in [MediaTier::Speech, MediaTier::Picture] {
                for asset in assets_for(tier, profile) {
                    assert_eq!(asset.sha256.len(), 64, "{}", asset.relative_path);
                    assert!(asset.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                    assert!(asset.size_bytes > 1024 * 1024);
                    assert!(asset.source_url.starts_with("https://"));
                }
            }
        }
    }

    /// The projector is shared between profiles, so a person who switches profile
    /// does not re-download the part that did not change.
    #[test]
    fn the_shared_projector_is_the_same_asset_in_both_profiles() {
        let path_of = |profile| {
            assets_for(MediaTier::Picture, profile)
                .into_iter()
                .find(|asset| asset.relative_path.contains("mmproj"))
                .map(|asset| (asset.relative_path, asset.sha256))
                .unwrap()
        };
        assert_eq!(
            path_of(ModelProfile::Compact),
            path_of(ModelProfile::Accurate)
        );
    }

    /// What a person is shown must describe an action and a size, never a
    /// developer's install command.
    #[test]
    fn the_message_is_written_for_a_person_not_a_developer() {
        let message = TierRequirement {
            tier: MediaTier::Speech,
            ready: false,
            download_bytes: 574_041_195,
            profile: ModelProfile::Accurate,
            message: requirement(MediaTier::Speech, ModelProfile::Accurate).message,
        };
        let text = message.message.unwrap_or_default();
        assert!(
            !text.contains("brew"),
            "developer instructions leaked: {text}"
        );
        assert!(!text.contains("cargo"));
        assert!(text.contains("this machine"));
    }

    /// The setup script resolves this path independently, so the two can drift.
    /// Pinning the platform rule here means a drift shows up as a failing test
    /// rather than as a wasted multi-gigabyte download.
    #[test]
    fn assets_live_under_the_platform_cache_directory() {
        let root = asset_root().expect("a cache directory");
        assert!(root.ends_with("ai-os"));
        let cache = dirs::cache_dir().unwrap();
        assert!(root.starts_with(&cache));

        if cfg!(target_os = "macos") {
            assert!(
                cache.ends_with("Library/Caches"),
                "macOS caches live in ~/Library/Caches, not ~/.cache: {cache:?}"
            );
        }
    }

    #[test]
    fn sizes_are_reported_in_units_a_person_reads() {
        assert_eq!(human_size(190_085_487), "181 MB");
        assert_eq!(human_size(1_893_671_520 + 337_012_000), "2.1 GB");
    }
}

#[cfg(test)]
mod no_developer_instructions_anywhere {
    /// A person who drags in a voice memo must never be shown a build command.
    /// This checks the whole media path, not one message, because the leak is
    /// easy to reintroduce one string at a time.
    #[test]
    fn no_media_path_tells_a_person_to_run_a_build_command() {
        let sources = [
            include_str!("transcription.rs"),
            include_str!("visual.rs"),
            include_str!("toolchain.rs"),
        ];
        for source in sources {
            for line in source.lines() {
                let trimmed = line.trim_start();
                // Comments explain the code to developers; only user-facing string
                // literals matter here.
                if trimmed.starts_with("//") {
                    continue;
                }
                // Built at run time so the needles themselves do not appear in
                // this file and match it.
                for forbidden in [
                    ["brew", "install"].join(" "),
                    ["cargo", "test"].join(" "),
                    ["cargo", "run"].join(" "),
                    ["verify", ""].join("/"),
                ] {
                    assert!(
                        !line.contains(&forbidden),
                        "a developer instruction reached a user-facing string: {line}"
                    );
                }
            }
        }
    }
}
