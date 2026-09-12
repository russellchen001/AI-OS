#[cfg(target_os = "macos")]
use super::{
    comfyui_macos::{
        active_input_directory, select_usable_desktop_installation, start_comfyui_backend_and_wait,
    },
    comfyui_reference::{
        image_setup_smoke_fixture, readiness_record_path, run_image_setup_smoke,
        validate_node_schemas, ReferenceVlmReadinessRecord, COMFYUI_VLM_ADAPTER_ID,
        COMFYUI_VLM_MODEL_ID, COMFYUI_VLM_UPSTREAM_REVISION, REFERENCE_READY_VERSION,
    },
    domain::{MediaKind, MediaReference},
    reference_analysis::{normalize_reference_response, ReferenceAnalyzerIdentity},
};
use serde::Serialize;
#[cfg(target_os = "macos")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};
#[cfg(target_os = "macos")]
use uuid::Uuid;

const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 5_000;

#[cfg(target_os = "macos")]
fn normalize_setup_smoke(
    instance_id: &str,
    smoke: &str,
) -> Result<super::domain::ReferenceSpec, String> {
    let fixture = image_setup_smoke_fixture().map_err(|error| error.message)?;
    normalize_reference_response(
        &MediaReference {
            id: "reference-vlm-setup-smoke".to_owned(),
            kind: MediaKind::Image,
            handle: "ephemeral://reference-vlm-setup-smoke".to_owned(),
        },
        &ReferenceAnalyzerIdentity {
            provider_id: COMFYUI_VLM_ADAPTER_ID.to_owned(),
            provider_instance_id: Some(instance_id.to_owned()),
        },
        &fixture,
        smoke,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceVlmSetupResult {
    pub provider_instance_id: String,
    pub upstream_revision: String,
    pub model_id: String,
    pub source_action: String,
    pub backup_path: Option<String>,
    pub dependencies_ready: bool,
    pub modern_vlm_schema_ok: bool,
    pub video_reasoner_schema_ok: bool,
    pub image_smoke_ok: bool,
    pub model_snapshot_path: String,
}

#[cfg(target_os = "macos")]
fn archive_url() -> String {
    format!(
        "https://github.com/gokayfem/ComfyUI_VLM_nodes/archive/{COMFYUI_VLM_UPSTREAM_REVISION}.tar.gz"
    )
}

#[cfg(target_os = "macos")]
fn download_archive() -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| format!("Reference VLM download client failed: {error}"))?;
    let response = client
        .get(archive_url())
        .send()
        .map_err(|error| format!("Reference VLM source download failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Reference VLM source download returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err("Reference VLM source archive exceeds the download bound".to_owned());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Reference VLM source archive could not be read: {error}"))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err("Reference VLM source archive is empty or too large".to_owned());
    }
    Ok(bytes)
}

#[cfg(target_os = "macos")]
fn safe_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut components = path.components();
    let archive_root = components
        .next()
        .ok_or_else(|| "Reference VLM archive entry has no root".to_owned())?;
    if !matches!(archive_root, Component::Normal(_)) {
        return Err("Reference VLM archive root is unsafe".to_owned());
    }
    let mut relative = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err("Reference VLM archive contains an unsafe path".to_owned()),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err("Reference VLM archive entry is empty".to_owned());
    }
    Ok(relative)
}

#[cfg(target_os = "macos")]
fn extract_archive(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0_u64;
    let mut count = 0_usize;
    for entry in archive
        .entries()
        .map_err(|error| format!("Reference VLM archive is invalid: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Reference VLM entry is invalid: {error}"))?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err("Reference VLM archive contains too many entries".to_owned());
        }
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
            || kind.is_gnu_longname()
            || kind.is_gnu_longlink()
        {
            continue;
        }
        if !kind.is_file() && !kind.is_dir() {
            return Err("Reference VLM archive contains a link or special file".to_owned());
        }
        let archived_path = entry
            .path()
            .map_err(|_| "Reference VLM archive path is invalid".to_owned())?;
        if kind.is_dir() && archived_path.components().count() == 1 {
            continue;
        }
        let relative = safe_relative_path(&archived_path)?;
        let output = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(&output).map_err(|error| {
                format!("Reference VLM directory could not be created: {error}")
            })?;
            continue;
        }
        let size = entry
            .header()
            .size()
            .map_err(|_| "Reference VLM entry size is invalid".to_owned())?;
        total = total.saturating_add(size);
        if total > MAX_EXTRACTED_BYTES {
            return Err("Reference VLM archive exceeds the extraction bound".to_owned());
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Reference VLM parent could not be created: {error}"))?;
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .map_err(|error| format!("Reference VLM file could not be created: {error}"))?;
        std::io::copy(&mut entry.take(size + 1), &mut file)
            .map_err(|error| format!("Reference VLM file could not be extracted: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Reference VLM file could not be synced: {error}"))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn managed_revision(destination: &Path) -> Option<String> {
    let bytes = fs::read(destination.join(".ai-os-managed.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("revision")?.as_str().map(str::to_owned)
}

#[cfg(target_os = "macos")]
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Managed JSON destination has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Managed JSON directory could not be created: {error}"))?;
    let temporary = parent.join(format!(".ai-os-{}.tmp", Uuid::new_v4().simple()));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "Managed JSON could not be serialized".to_owned())?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("Managed JSON temporary file could not be created: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Managed JSON temporary file could not be written: {error}"))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("Managed JSON could not be installed atomically: {error}"))
}

#[cfg(target_os = "macos")]
fn validate_requirements(destination: &Path) -> Result<PathBuf, String> {
    let path = destination.join("requirements.txt");
    let contents = fs::read_to_string(&path)
        .map_err(|_| "Reference VLM requirements.txt is unavailable".to_owned())?;
    for line in contents.lines() {
        let package = line
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(character, '<' | '>' | '=' | '!' | '~' | '[' | ';')
            })
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(package.as_str(), "torch" | "torchvision" | "torchaudio") {
            return Err(
                "Reference VLM requirements may not replace ComfyUI torch packages".to_owned(),
            );
        }
    }
    Ok(path)
}

#[cfg(target_os = "macos")]
fn install_source(custom_nodes: &Path) -> Result<(PathBuf, String, Option<String>), String> {
    let destination = custom_nodes.join("AIOS_ComfyUI_VLM_nodes");
    if destination.exists() && managed_revision(&destination).is_none() {
        return Err(
            "An unmanaged Reference VLM installation already uses the AI-OS destination".to_owned(),
        );
    }
    if managed_revision(&destination).as_deref() == Some(COMFYUI_VLM_UPSTREAM_REVISION) {
        return Ok((destination, "already-installed".to_owned(), None));
    }

    fs::create_dir_all(custom_nodes)
        .map_err(|error| format!("ComfyUI custom_nodes is unavailable: {error}"))?;
    let staging = custom_nodes.join(format!(".ai-os-vlm-{}.staging", Uuid::new_v4().simple()));
    fs::create_dir(&staging)
        .map_err(|error| format!("Reference VLM staging directory failed: {error}"))?;
    let result = (|| {
        let bytes = download_archive()?;
        extract_archive(&bytes, &staging)?;
        write_json_atomic(
            &staging.join(".ai-os-managed.json"),
            &serde_json::json!({
                "source":"gokayfem/ComfyUI_VLM_nodes",
                "revision":COMFYUI_VLM_UPSTREAM_REVISION
            }),
        )?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let backup = if destination.exists() {
        let backup = custom_nodes.join(format!(
            "AIOS_ComfyUI_VLM_nodes.backup-{}",
            Uuid::new_v4().simple()
        ));
        fs::rename(&destination, &backup)
            .map_err(|error| format!("Managed Reference VLM backup failed: {error}"))?;
        Some(backup)
    } else {
        None
    };
    fs::rename(&staging, &destination)
        .map_err(|error| format!("Reference VLM source install failed: {error}"))?;
    Ok((
        destination,
        if backup.is_some() {
            "repaired"
        } else {
            "installed"
        }
        .to_owned(),
        backup.map(|path| path.to_string_lossy().into_owned()),
    ))
}

#[cfg(target_os = "macos")]
fn install_dependencies(python: &Path, requirements: &Path) -> Result<(), String> {
    let output = Command::new(python)
        .args(["-m", "pip", "install", "--disable-pip-version-check", "-r"])
        .arg(requirements)
        .output()
        .map_err(|error| format!("Reference VLM dependency installation failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Reference VLM dependency installation failed: {}",
            stderr
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn model_snapshot_path(
    installation: &super::comfyui_macos::ComfyUiMacOsInstallation,
) -> Result<String, String> {
    let runtime_root = installation
        .main_py_path
        .parent()
        .ok_or_else(|| "ComfyUI runtime root is unavailable".to_owned())?;
    let script = "import folder_paths; from pathlib import Path; print(Path(folder_paths.models_dir) / 'LLavacheckpoints' / 'modern-vlm' / 'Qwen--Qwen3-VL-2B-Instruct')";
    let output = Command::new(&installation.python_path)
        .current_dir(runtime_root)
        .args(["-c", script])
        .output()
        .map_err(|error| format!("Reference VLM model cache inspection failed: {error}"))?;
    if !output.status.success() {
        return Err(
            "Reference VLM smoke ran but the complete model snapshot was not found".to_owned(),
        );
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let snapshot = Path::new(&path);
    let has_weights = fs::read_dir(snapshot)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "safetensors")
        });
    if path.is_empty() || !snapshot.join("config.json").is_file() || !has_weights {
        return Err("Reference VLM model snapshot path is invalid".to_owned());
    }
    Ok(path)
}

#[cfg(target_os = "macos")]
fn setup_blocking() -> Result<ReferenceVlmSetupResult, String> {
    let installation = select_usable_desktop_installation()?;
    let runtime_root = installation
        .main_py_path
        .parent()
        .ok_or_else(|| "ComfyUI runtime root is unavailable".to_owned())?;
    let custom_nodes = runtime_root.join("custom_nodes");
    let (destination, source_action, backup_path) = install_source(&custom_nodes)?;
    let requirements = validate_requirements(&destination)?;
    install_dependencies(&installation.python_path, &requirements)?;

    let mut backend = start_comfyui_backend_and_wait(&installation, Duration::from_secs(180))?;
    let result = (|| {
        let (modern_vlm_schema_ok, video_reasoner_schema_ok) =
            validate_node_schemas(&backend.endpoint).map_err(|error| error.message)?;
        let smoke =
            run_image_setup_smoke(&backend.endpoint, &active_input_directory(&installation)?)
                .map_err(|error| error.message)?;
        if smoke.trim().is_empty() {
            return Err("Reference VLM real smoke returned empty output".to_owned());
        }
        let normalized = normalize_setup_smoke(&installation.instance_id, &smoke)?;
        let model_snapshot_path = model_snapshot_path(&installation)?;
        let smoke_output_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(smoke.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let smoke_reference_spec_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(serde_json::to_vec(&normalized).map_err(|error| error.to_string())?);
            format!("{:x}", hasher.finalize())
        };
        let record = ReferenceVlmReadinessRecord {
            contract_version: REFERENCE_READY_VERSION,
            provider_instance_id: installation.instance_id.clone(),
            upstream_revision: COMFYUI_VLM_UPSTREAM_REVISION.to_owned(),
            model_id: COMFYUI_VLM_MODEL_ID.to_owned(),
            model_snapshot_path: model_snapshot_path.clone(),
            modern_vlm_schema_ok,
            video_reasoner_schema_ok,
            image_smoke_ok: true,
            smoke_output_sha256,
            smoke_reference_spec_sha256,
        };
        write_json_atomic(&readiness_record_path()?, &record)?;
        Ok(ReferenceVlmSetupResult {
            provider_instance_id: installation.instance_id.clone(),
            upstream_revision: COMFYUI_VLM_UPSTREAM_REVISION.to_owned(),
            model_id: COMFYUI_VLM_MODEL_ID.to_owned(),
            source_action,
            backup_path,
            dependencies_ready: true,
            modern_vlm_schema_ok,
            video_reasoner_schema_ok,
            image_smoke_ok: true,
            model_snapshot_path,
        })
    })();
    backend.stop();
    result
}

#[tauri::command]
pub async fn setup_comfyui_reference_vlm(
    user_confirmed: bool,
) -> Result<ReferenceVlmSetupResult, String> {
    if !user_confirmed {
        return Err("Reference VLM Setup / Repair requires explicit user confirmation".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        return tokio::task::spawn_blocking(setup_blocking)
            .await
            .map_err(|_| "Reference VLM Setup / Repair worker failed".to_owned())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Reference VLM Setup / Repair is not implemented on this platform".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_reject_traversal_and_absolute_components() {
        #[cfg(target_os = "macos")]
        {
            assert!(safe_relative_path(Path::new("root/../../escape")).is_err());
            assert!(safe_relative_path(Path::new("/absolute/escape")).is_err());
            assert_eq!(
                safe_relative_path(Path::new("root/nodes/modern.py")).unwrap(),
                PathBuf::from("nodes/modern.py")
            );
        }
    }

    #[test]
    fn setup_requires_explicit_confirmation_before_work_starts() {
        let result = tauri::async_runtime::block_on(setup_comfyui_reference_vlm(false));
        assert!(result.is_err());
    }

    #[test]
    fn ready_promotion_requires_smoke_text_normalizable_as_reference_spec() {
        #[cfg(target_os = "macos")]
        {
            assert!(normalize_setup_smoke("local-a", "").is_err());
            let normalized = normalize_setup_smoke(
                "local-a",
                r#"{"summary":"blue square","constraints":["preserve blue"]}"#,
            )
            .unwrap();
            assert_eq!(normalized.summary, "blue square");
            assert_eq!(
                normalized.analyzer_provider_instance_id.as_deref(),
                Some("local-a")
            );
        }
    }

    #[test]
    #[ignore = "downloads the pinned OSS source/model and runs a real local ComfyUI VLM smoke"]
    fn live_reference_vlm_setup_and_smoke() {
        #[cfg(target_os = "macos")]
        {
            let result = setup_blocking().expect("real Reference VLM Setup / Repair");
            assert!(result.dependencies_ready);
            assert!(result.modern_vlm_schema_ok);
            assert!(result.video_reasoner_schema_ok);
            assert!(result.image_smoke_ok);
            assert!(Path::new(&result.model_snapshot_path).is_dir());
            eprintln!(
                "LIVE_GM5_REFERENCE instance={} revision={} model={} action={} snapshot={}",
                result.provider_instance_id,
                result.upstream_revision,
                result.model_id,
                result.source_action,
                result.model_snapshot_path
            );
        }
    }
}
