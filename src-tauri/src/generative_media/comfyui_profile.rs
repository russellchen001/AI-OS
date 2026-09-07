use super::provider::LocalMediaReadinessEvidence;
use reqwest::{blocking::Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path},
    time::Duration,
};

pub(crate) const CORE_TEXT_TO_IMAGE_PROFILE_ID: &str = "comfyui-checkpoint-t2i-v1";
pub(crate) const CORE_TEXT_TO_IMAGE_PROFILE_VERSION: u32 = 1;

const REQUIRED_CORE_NODES: &[&str] = &[
    "CheckpointLoaderSimple",
    "CLIPTextEncode",
    "EmptyLatentImage",
    "KSampler",
    "VAEDecode",
    "SaveImage",
];

const REQUIRED_CUSTOM_NODES: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManagedAssetRecord {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManagedProfileManifest {
    pub profile_id: String,
    pub profile_version: u32,
    pub checkpoint: ManagedAssetRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiProfileReadiness {
    pub profile_id: String,
    pub workflow_ready: bool,
    pub required_assets_ready: bool,
    pub custom_nodes_ready: bool,
    pub integrity_ok: bool,
    pub selected_checkpoint: Option<String>,
    pub diagnostics: Vec<String>,
}

impl ComfyUiProfileReadiness {
    pub(crate) fn apply_to_evidence(
        &self,
        base: &LocalMediaReadinessEvidence,
    ) -> LocalMediaReadinessEvidence {
        LocalMediaReadinessEvidence {
            engine_installed: base.engine_installed,
            engine_startable: base.engine_startable,
            api_reachable: base.api_reachable,
            workflow_ready: self.workflow_ready,
            required_assets_ready: self.required_assets_ready,
            custom_nodes_ready: self.custom_nodes_ready,
            integrity_ok: self.integrity_ok,
            smoke_generation_ok: base.smoke_generation_ok,
            output_retrieval_ok: base.output_retrieval_ok,
        }
    }
}

/// Provider-owned API-format workflow for the first AI-OS local generation profile.
///
/// It deliberately uses only core ComfyUI nodes. GM-3 supplies the managed
/// checkpoint. GM-2C will replace the stable placeholders and submit this graph.
pub(crate) fn core_text_to_image_workflow(checkpoint_name: &str, prompt: &str) -> Value {
    json!({
        "1": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {
                "ckpt_name": checkpoint_name
            }
        },
        "2": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": prompt,
                "clip": ["1", 1]
            }
        },
        "3": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": "",
                "clip": ["1", 1]
            }
        },
        "4": {
            "class_type": "EmptyLatentImage",
            "inputs": {
                "width": 512,
                "height": 512,
                "batch_size": 1
            }
        },
        "5": {
            "class_type": "KSampler",
            "inputs": {
                "seed": 0,
                "steps": 20,
                "cfg": 7.0,
                "sampler_name": "euler",
                "scheduler": "normal",
                "denoise": 1.0,
                "model": ["1", 0],
                "positive": ["2", 0],
                "negative": ["3", 0],
                "latent_image": ["4", 0]
            }
        },
        "6": {
            "class_type": "VAEDecode",
            "inputs": {
                "samples": ["5", 0],
                "vae": ["1", 2]
            }
        },
        "7": {
            "class_type": "SaveImage",
            "inputs": {
                "filename_prefix": "AI-OS",
                "images": ["6", 0]
            }
        }
    })
}

pub(crate) fn fetch_comfyui_object_info(endpoint: &str) -> Result<Value, String> {
    let mut base = Url::parse(endpoint).map_err(|_| "ComfyUI endpoint is invalid".to_owned())?;

    if !matches!(base.scheme(), "http" | "https") {
        return Err("ComfyUI endpoint must use http or https".to_owned());
    }

    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }

    let url = base
        .join("object_info")
        .map_err(|_| "ComfyUI object_info URL is invalid".to_owned())?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Unable to create ComfyUI profile HTTP client".to_owned())?;

    let response = client
        .get(url)
        .send()
        .map_err(|_| "Unable to reach ComfyUI object_info".to_owned())?;

    if !response.status().is_success() {
        return Err("ComfyUI object_info returned an unsuccessful status".to_owned());
    }

    let value: Value = response
        .json()
        .map_err(|_| "ComfyUI object_info returned invalid JSON".to_owned())?;

    if !value.is_object() {
        return Err("ComfyUI object_info returned an invalid object".to_owned());
    }

    Ok(value)
}

fn node<'a>(object_info: &'a Value, class_type: &str) -> Option<&'a Value> {
    object_info.as_object()?.get(class_type)
}

fn required_input_schema<'a>(
    object_info: &'a Value,
    class_type: &str,
    input: &str,
) -> Option<&'a Value> {
    node(object_info, class_type)?
        .get("input")?
        .get("required")?
        .get(input)
}

fn choice_strings(schema: &Value) -> Vec<String> {
    schema
        .as_array()
        .and_then(|items| items.first())
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn workflow_class_types(workflow: &Value) -> Vec<String> {
    let mut class_types = workflow
        .as_object()
        .into_iter()
        .flat_map(|nodes| nodes.values())
        .filter_map(|node| node.get("class_type"))
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();

    class_types.sort();
    class_types.dedup();
    class_types
}

fn core_workflow_schema_ready(object_info: &Value) -> bool {
    if !REQUIRED_CORE_NODES
        .iter()
        .all(|class_type| node(object_info, class_type).is_some())
    {
        return false;
    }

    let required_inputs: &[(&str, &[&str])] = &[
        ("CheckpointLoaderSimple", &["ckpt_name"]),
        ("CLIPTextEncode", &["text", "clip"]),
        ("EmptyLatentImage", &["width", "height", "batch_size"]),
        (
            "KSampler",
            &[
                "seed",
                "steps",
                "cfg",
                "sampler_name",
                "scheduler",
                "denoise",
                "model",
                "positive",
                "negative",
                "latent_image",
            ],
        ),
        ("VAEDecode", &["samples", "vae"]),
        ("SaveImage", &["filename_prefix", "images"]),
    ];

    if !required_inputs.iter().all(|(class_type, inputs)| {
        inputs
            .iter()
            .all(|input| required_input_schema(object_info, class_type, input).is_some())
    }) {
        return false;
    }

    let sampler_names = required_input_schema(object_info, "KSampler", "sampler_name")
        .map(choice_strings)
        .unwrap_or_default();

    if !sampler_names.iter().any(|value| value == "euler") {
        return false;
    }

    let schedulers = required_input_schema(object_info, "KSampler", "scheduler")
        .map(choice_strings)
        .unwrap_or_default();

    if !schedulers.iter().any(|value| value == "normal") {
        return false;
    }

    let template = core_text_to_image_workflow("__AI_OS_CHECKPOINT__", "__AI_OS_PROMPT__");
    let class_types = workflow_class_types(&template);

    REQUIRED_CORE_NODES
        .iter()
        .all(|required| class_types.iter().any(|actual| actual == required))
}

fn checkpoint_names(object_info: &Value) -> Vec<String> {
    required_input_schema(object_info, "CheckpointLoaderSimple", "ckpt_name")
        .map(choice_strings)
        .unwrap_or_default()
}

fn safe_checkpoint_relative_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return false;
    }

    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return false;
    }

    matches!(
        path.components().next(),
        Some(Component::Normal(value))
            if value == std::ffi::OsStr::new("checkpoints")
    )
}

fn checkpoint_name_from_relative_path(path: &Path) -> Option<String> {
    let relative = path.strip_prefix("checkpoints").ok()?;

    if relative.as_os_str().is_empty() {
        return None;
    }

    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "Unable to open managed checkpoint".to_owned())?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "Unable to read managed checkpoint".to_owned())?;

        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn load_and_validate_manifest(path: &Path) -> Result<ManagedProfileManifest, String> {
    let contents =
        fs::read_to_string(path).map_err(|_| "managed-profile-manifest-unreadable".to_owned())?;

    let manifest: ManagedProfileManifest = serde_json::from_str(&contents)
        .map_err(|_| "managed-profile-manifest-invalid".to_owned())?;

    if manifest.profile_id != CORE_TEXT_TO_IMAGE_PROFILE_ID
        || manifest.profile_version != CORE_TEXT_TO_IMAGE_PROFILE_VERSION
    {
        return Err("managed-profile-version-mismatch".to_owned());
    }

    if manifest.checkpoint.size_bytes == 0 {
        return Err("managed-checkpoint-size-invalid".to_owned());
    }

    if !valid_sha256(&manifest.checkpoint.sha256) {
        return Err("managed-checkpoint-sha256-invalid".to_owned());
    }

    let relative = Path::new(&manifest.checkpoint.relative_path);

    if !safe_checkpoint_relative_path(relative)
        || checkpoint_name_from_relative_path(relative).is_none()
    {
        return Err("managed-checkpoint-path-invalid".to_owned());
    }

    Ok(manifest)
}

pub(crate) fn evaluate_core_text_to_image_profile(
    object_info: &Value,
    models_root: Option<&Path>,
    manifest_path: &Path,
) -> ComfyUiProfileReadiness {
    let workflow_ready = core_workflow_schema_ready(object_info);

    let custom_nodes_ready = REQUIRED_CUSTOM_NODES
        .iter()
        .all(|class_type| node(object_info, class_type).is_some());

    let mut readiness = ComfyUiProfileReadiness {
        profile_id: CORE_TEXT_TO_IMAGE_PROFILE_ID.to_owned(),
        workflow_ready,
        required_assets_ready: false,
        custom_nodes_ready,
        integrity_ok: false,
        selected_checkpoint: None,
        diagnostics: Vec::new(),
    };

    if !workflow_ready {
        readiness
            .diagnostics
            .push("core-workflow-incompatible".to_owned());
    }

    if !custom_nodes_ready {
        readiness
            .diagnostics
            .push("required-custom-node-missing".to_owned());
    }

    if !manifest_path.is_file() {
        readiness
            .diagnostics
            .push("managed-profile-manifest-missing".to_owned());
        return readiness;
    }

    let manifest = match load_and_validate_manifest(manifest_path) {
        Ok(manifest) => manifest,
        Err(code) => {
            readiness.diagnostics.push(code);
            return readiness;
        }
    };

    let relative = Path::new(&manifest.checkpoint.relative_path);
    let checkpoint_name = match checkpoint_name_from_relative_path(relative) {
        Some(name) => name,
        None => {
            readiness
                .diagnostics
                .push("managed-checkpoint-path-invalid".to_owned());
            return readiness;
        }
    };

    readiness.selected_checkpoint = Some(checkpoint_name.clone());

    let Some(models_root) = models_root else {
        readiness
            .diagnostics
            .push("desktop-model-root-unavailable".to_owned());
        return readiness;
    };

    let asset_path = models_root.join(relative);

    let metadata = match fs::symlink_metadata(&asset_path) {
        Ok(metadata) => metadata,
        Err(_) => {
            readiness
                .diagnostics
                .push("managed-checkpoint-missing".to_owned());
            return readiness;
        }
    };

    if metadata.file_type().is_symlink() {
        readiness
            .diagnostics
            .push("managed-checkpoint-symlink-refused".to_owned());
        return readiness;
    }

    if !metadata.file_type().is_file() {
        readiness
            .diagnostics
            .push("managed-checkpoint-not-file".to_owned());
        return readiness;
    }

    let canonical_root = match models_root.canonicalize() {
        Ok(root) => root,
        Err(_) => {
            readiness
                .diagnostics
                .push("desktop-model-root-unavailable".to_owned());
            return readiness;
        }
    };

    let canonical_asset = match asset_path.canonicalize() {
        Ok(asset) => asset,
        Err(_) => {
            readiness
                .diagnostics
                .push("managed-checkpoint-missing".to_owned());
            return readiness;
        }
    };

    if !canonical_asset.starts_with(&canonical_root) {
        readiness
            .diagnostics
            .push("managed-checkpoint-path-escape".to_owned());
        return readiness;
    }

    let advertised = checkpoint_names(object_info)
        .iter()
        .any(|available| available == &checkpoint_name);

    if !advertised {
        readiness
            .diagnostics
            .push("managed-checkpoint-not-advertised".to_owned());
        return readiness;
    }

    readiness.required_assets_ready = true;

    if metadata.len() != manifest.checkpoint.size_bytes {
        readiness
            .diagnostics
            .push("managed-checkpoint-size-mismatch".to_owned());
        return readiness;
    }

    let actual_hash = match sha256_file(&canonical_asset) {
        Ok(hash) => hash,
        Err(_) => {
            readiness
                .diagnostics
                .push("managed-checkpoint-hash-unavailable".to_owned());
            return readiness;
        }
    };

    if actual_hash != manifest.checkpoint.sha256.to_ascii_lowercase() {
        readiness
            .diagnostics
            .push("managed-checkpoint-sha256-mismatch".to_owned());
        return readiness;
    }

    readiness.integrity_ok = true;
    readiness
}

pub(crate) fn probe_core_text_to_image_profile(
    endpoint: &str,
    models_root: Option<&Path>,
    manifest_path: &Path,
) -> Result<ComfyUiProfileReadiness, String> {
    let object_info = fetch_comfyui_object_info(endpoint)?;

    Ok(evaluate_core_text_to_image_profile(
        &object_info,
        models_root,
        manifest_path,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, path::PathBuf};

    fn object_info(checkpoints: &[&str]) -> Value {
        json!({
            "CheckpointLoaderSimple": {
                "input": {
                    "required": {
                        "ckpt_name": [checkpoints, {}]
                    }
                }
            },
            "CLIPTextEncode": {
                "input": {
                    "required": {
                        "text": ["STRING", {}],
                        "clip": ["CLIP", {}]
                    }
                }
            },
            "EmptyLatentImage": {
                "input": {
                    "required": {
                        "width": ["INT", {}],
                        "height": ["INT", {}],
                        "batch_size": ["INT", {}]
                    }
                }
            },
            "KSampler": {
                "input": {
                    "required": {
                        "seed": ["INT", {}],
                        "steps": ["INT", {}],
                        "cfg": ["FLOAT", {}],
                        "sampler_name": [["euler", "dpmpp_2m"], {}],
                        "scheduler": [["normal", "karras"], {}],
                        "denoise": ["FLOAT", {}],
                        "model": ["MODEL", {}],
                        "positive": ["CONDITIONING", {}],
                        "negative": ["CONDITIONING", {}],
                        "latent_image": ["LATENT", {}]
                    }
                }
            },
            "VAEDecode": {
                "input": {
                    "required": {
                        "samples": ["LATENT", {}],
                        "vae": ["VAE", {}]
                    }
                }
            },
            "SaveImage": {
                "input": {
                    "required": {
                        "filename_prefix": ["STRING", {}],
                        "images": ["IMAGE", {}]
                    }
                }
            }
        })
    }

    fn write_manifest(path: &Path, relative_path: &str, size_bytes: u64, sha256: &str) {
        let manifest = ManagedProfileManifest {
            profile_id: CORE_TEXT_TO_IMAGE_PROFILE_ID.to_owned(),
            profile_version: CORE_TEXT_TO_IMAGE_PROFILE_VERSION,
            checkpoint: ManagedAssetRecord {
                relative_path: relative_path.to_owned(),
                size_bytes,
                sha256: sha256.to_owned(),
            },
        };

        fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn core_workflow_uses_only_declared_core_nodes() {
        let workflow = core_text_to_image_workflow("managed.safetensors", "a lighthouse");

        let actual = workflow_class_types(&workflow)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let expected = REQUIRED_CORE_NODES
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<BTreeSet<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn missing_manifest_keeps_profile_not_configured() {
        let root = tempfile::tempdir().unwrap();
        let models_root = root.path().join("models");
        fs::create_dir_all(models_root.join("checkpoints")).unwrap();

        let readiness = evaluate_core_text_to_image_profile(
            &object_info(&[]),
            Some(&models_root),
            &root.path().join("missing-profile.json"),
        );

        assert!(readiness.workflow_ready);
        assert!(!readiness.required_assets_ready);
        assert!(readiness.custom_nodes_ready);
        assert!(!readiness.integrity_ok);
        assert!(readiness.selected_checkpoint.is_none());
        assert!(readiness
            .diagnostics
            .contains(&"managed-profile-manifest-missing".to_owned()));
    }

    #[test]
    fn managed_checkpoint_must_match_advertisement_size_and_hash() {
        let root = tempfile::tempdir().unwrap();
        let models_root = root.path().join("models");
        let checkpoint_dir = models_root.join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();

        let checkpoint = checkpoint_dir.join("managed.safetensors");
        fs::write(&checkpoint, b"ai-os-managed-checkpoint-fixture").unwrap();

        let hash = sha256_file(&checkpoint).unwrap();
        let size = fs::metadata(&checkpoint).unwrap().len();
        let manifest_path = root.path().join("profile.json");

        write_manifest(
            &manifest_path,
            "checkpoints/managed.safetensors",
            size,
            &hash,
        );

        let readiness = evaluate_core_text_to_image_profile(
            &object_info(&["managed.safetensors"]),
            Some(&models_root),
            &manifest_path,
        );

        assert!(readiness.workflow_ready);
        assert!(readiness.required_assets_ready);
        assert!(readiness.custom_nodes_ready);
        assert!(readiness.integrity_ok);
        assert_eq!(
            readiness.selected_checkpoint.as_deref(),
            Some("managed.safetensors")
        );

        fs::write(&checkpoint, b"changed").unwrap();

        let changed = evaluate_core_text_to_image_profile(
            &object_info(&["managed.safetensors"]),
            Some(&models_root),
            &manifest_path,
        );

        assert!(changed.required_assets_ready);
        assert!(!changed.integrity_ok);
    }

    #[test]
    fn unsafe_managed_checkpoint_path_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let models_root = root.path().join("models");
        fs::create_dir_all(&models_root).unwrap();

        let manifest_path = root.path().join("profile.json");

        write_manifest(&manifest_path, "../outside.safetensors", 1, &"0".repeat(64));

        let readiness = evaluate_core_text_to_image_profile(
            &object_info(&["outside.safetensors"]),
            Some(&models_root),
            &manifest_path,
        );

        assert!(!readiness.required_assets_ready);
        assert!(!readiness.integrity_ok);
        assert!(readiness
            .diagnostics
            .contains(&"managed-checkpoint-path-invalid".to_owned()));
    }

    #[test]
    fn missing_required_core_node_makes_workflow_incompatible() {
        let mut info = object_info(&[]);
        info.as_object_mut().unwrap().remove("VAEDecode");

        let root = tempfile::tempdir().unwrap();

        let readiness = evaluate_core_text_to_image_profile(
            &info,
            Some(&PathBuf::from(root.path())),
            &root.path().join("missing.json"),
        );

        assert!(!readiness.workflow_ready);
        assert!(readiness.custom_nodes_ready);
    }
}
