use super::{
    comfyui_profile::{
        ManagedAssetRecord, ManagedProfileManifest, CORE_TEXT_TO_IMAGE_PROFILE_ID,
        CORE_TEXT_TO_IMAGE_PROFILE_VERSION,
    },
    provider::LocalMediaReadiness,
};
use reqwest::{
    blocking::{Client, Response},
    header::{CONTENT_RANGE, RANGE},
    StatusCode,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    thread,
    time::Duration,
};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use super::comfyui_macos::{
    managed_profile_manifest_path, probe_comfyui_installation_profile_readiness,
    resolve_desktop_model_root, select_usable_desktop_installation,
};

const BOOTSTRAP_FILE: &str = "v1-5-pruned-emaonly-fp16.safetensors";
const BOOTSTRAP_SIZE: u64 = 2_132_696_762;
const BOOTSTRAP_SHA256: &str = "e9476a13728cd75d8279f6ec8bad753a66a1957ca375a1464dc63b37db6e3916";
const BOOTSTRAP_LICENSE: &str = "creativeml-openrail-m";

#[derive(Debug, Clone)]
struct ManagedAssetSpec {
    source_url: String,
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

impl ManagedAssetSpec {
    fn bootstrap() -> Self {
        let host = "huggingface.co";

        Self {
            source_url: format!(
                "https://{host}/Comfy-Org/stable-diffusion-v1-5-archive/resolve/main/{BOOTSTRAP_FILE}?download=true"
            ),
            relative_path: format!("checkpoints/{BOOTSTRAP_FILE}"),
            size_bytes: BOOTSTRAP_SIZE,
            sha256: BOOTSTRAP_SHA256.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExistingAssetState {
    Missing,
    ExactUnmanaged,
    ExactManaged,
    ManagedNeedsRepair,
}

#[derive(Debug)]
struct InstalledAsset {
    action: String,
    final_path: PathBuf,
    backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedProfileSetupResult {
    pub profile_id: String,
    pub action: String,
    pub checkpoint_file: String,
    pub checkpoint_path: String,
    pub backup_path: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    pub license: String,
    pub workflow_ready: bool,
    pub required_assets_ready: bool,
    pub custom_nodes_ready: bool,
    pub integrity_ok: bool,
    pub smoke_generation_checked: bool,
    pub output_retrieval_checked: bool,
    pub readiness: LocalMediaReadiness,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|_| "Managed checkpoint could not be opened".to_owned())?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "Managed checkpoint could not be read".to_owned())?;

        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_relative_asset_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("Managed checkpoint path is invalid".to_owned());
    }

    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Managed checkpoint path may not traverse directories".to_owned());
    }

    if !matches!(
        path.components().next(),
        Some(Component::Normal(value))
            if value == std::ffi::OsStr::new("checkpoints")
    ) {
        return Err("Managed checkpoint must be stored under checkpoints/".to_owned());
    }

    Ok(())
}

fn validate_destination(
    models_root: &Path,
    spec: &ManagedAssetSpec,
) -> Result<(PathBuf, PathBuf), String> {
    let relative = Path::new(&spec.relative_path);
    validate_relative_asset_path(relative)?;

    if !models_root.is_dir() {
        return Err("ComfyUI models root is unavailable".to_owned());
    }

    let canonical_root = models_root
        .canonicalize()
        .map_err(|_| "ComfyUI models root could not be resolved".to_owned())?;

    let final_path = models_root.join(relative);
    let parent = final_path
        .parent()
        .ok_or_else(|| "Managed checkpoint parent is unavailable".to_owned())?;

    fs::create_dir_all(parent)
        .map_err(|_| "Managed checkpoint directory could not be created".to_owned())?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| "Managed checkpoint directory could not be resolved".to_owned())?;

    if !canonical_parent.starts_with(&canonical_root) {
        return Err("Managed checkpoint path escaped the ComfyUI models root".to_owned());
    }

    let part_path = parent.join(format!(".{BOOTSTRAP_FILE}.ai-os.part"));

    Ok((final_path, part_path))
}

fn file_matches_spec(path: &Path, spec: &ManagedAssetSpec) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("Managed checkpoint metadata could not be read".to_owned()),
    };

    if metadata.file_type().is_symlink() {
        return Err("Managed checkpoint symlinks are refused".to_owned());
    }

    if !metadata.file_type().is_file() {
        return Err("Managed checkpoint path is not a regular file".to_owned());
    }

    if metadata.len() != spec.size_bytes {
        return Ok(false);
    }

    Ok(sha256_file(path)? == spec.sha256)
}

fn manifest_matches_spec(path: &Path, spec: &ManagedAssetSpec) -> bool {
    let Ok(contents) = fs::read(path) else {
        return false;
    };

    let Ok(manifest) = serde_json::from_slice::<ManagedProfileManifest>(&contents) else {
        return false;
    };

    manifest.profile_id == CORE_TEXT_TO_IMAGE_PROFILE_ID
        && manifest.profile_version == CORE_TEXT_TO_IMAGE_PROFILE_VERSION
        && manifest.checkpoint.relative_path == spec.relative_path
        && manifest.checkpoint.size_bytes == spec.size_bytes
        && manifest
            .checkpoint
            .sha256
            .eq_ignore_ascii_case(&spec.sha256)
}

fn inspect_existing_asset(
    final_path: &Path,
    manifest_path: &Path,
    spec: &ManagedAssetSpec,
) -> Result<ExistingAssetState, String> {
    if !final_path.exists() {
        return Ok(ExistingAssetState::Missing);
    }

    let exact = file_matches_spec(final_path, spec)?;

    if exact {
        return Ok(if manifest_matches_spec(manifest_path, spec) {
            ExistingAssetState::ExactManaged
        } else {
            ExistingAssetState::ExactUnmanaged
        });
    }

    // A manifest at AI-OS's private profile path is ownership evidence even
    // if the manifest itself was damaged. Repair still preserves the old
    // checkpoint as a backup before replacing it.
    if manifest_path.is_file() {
        return Ok(ExistingAssetState::ManagedNeedsRepair);
    }

    Err(
        "A different unmanaged file already uses the AI-OS bootstrap checkpoint name. \
         AI-OS will not overwrite it. Move or rename that file and run Setup / Repair again."
            .to_owned(),
    )
}

fn profile_manifest(spec: &ManagedAssetSpec) -> ManagedProfileManifest {
    ManagedProfileManifest {
        profile_id: CORE_TEXT_TO_IMAGE_PROFILE_ID.to_owned(),
        profile_version: CORE_TEXT_TO_IMAGE_PROFILE_VERSION,
        checkpoint: ManagedAssetRecord {
            relative_path: spec.relative_path.clone(),
            size_bytes: spec.size_bytes,
            sha256: spec.sha256.clone(),
        },
    }
}

fn write_manifest_atomic(
    manifest_path: &Path,
    manifest: &ManagedProfileManifest,
) -> Result<(), String> {
    let parent = manifest_path
        .parent()
        .ok_or_else(|| "Managed profile directory is unavailable".to_owned())?;

    fs::create_dir_all(parent)
        .map_err(|_| "Managed profile directory could not be created".to_owned())?;

    let filename = manifest_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("profile.json");

    let temporary = parent.join(format!(".{filename}.{}.tmp", Uuid::new_v4()));

    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|_| "Managed profile manifest could not be encoded".to_owned())?;

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "Managed profile temporary manifest could not be created".to_owned())?;

    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Managed profile temporary manifest could not be written: {error}"
        ));
    }

    drop(file);

    if let Err(error) = fs::rename(&temporary, manifest_path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Managed profile manifest could not be installed atomically: {error}"
        ));
    }

    Ok(())
}

fn response_range_starts_at(response: &Response, offset: u64) -> bool {
    let Some(value) = response.headers().get(CONTENT_RANGE) else {
        return false;
    };

    let Ok(value) = value.to_str() else {
        return false;
    };

    value.starts_with(&format!("bytes {offset}-"))
}

fn stream_response(mut response: Response, part_path: &Path, append: bool) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);

    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }

    let mut file = options
        .open(part_path)
        .map_err(|_| "Managed checkpoint partial file could not be opened".to_owned())?;

    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|_| "Managed checkpoint download was interrupted".to_owned())?;

        if read == 0 {
            break;
        }

        file.write_all(&buffer[..read])
            .map_err(|_| "Managed checkpoint partial file could not be written".to_owned())?;
    }

    file.sync_all()
        .map_err(|_| "Managed checkpoint partial file could not be synchronized".to_owned())
}

fn download_with_resume(
    client: &Client,
    spec: &ManagedAssetSpec,
    part_path: &Path,
) -> Result<(), String> {
    let mut last_error = "Managed checkpoint download failed".to_owned();

    for attempt in 1..=3 {
        let offset = fs::metadata(part_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        if offset > spec.size_bytes {
            let _ = fs::remove_file(part_path);
            last_error = "Partial checkpoint was larger than the expected asset".to_owned();
            continue;
        }

        if offset == spec.size_bytes {
            if file_matches_spec(part_path, spec)? {
                return Ok(());
            }

            fs::remove_file(part_path).map_err(|_| {
                "Corrupt completed partial checkpoint could not be removed".to_owned()
            })?;

            last_error = "Completed partial checkpoint failed integrity verification and was reset"
                .to_owned();

            continue;
        }

        let mut request = client.get(&spec.source_url);

        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                last_error =
                    format!("Managed checkpoint request attempt {attempt} failed: {error}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        let status = response.status();

        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            if offset == spec.size_bytes {
                return Ok(());
            }

            let _ = fs::remove_file(part_path);
            last_error = "Remote server rejected the partial checkpoint range".to_owned();
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        if !status.is_success() {
            last_error = format!(
                "Managed checkpoint server returned HTTP {}",
                status.as_u16()
            );
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        let append = if offset > 0 && status == StatusCode::PARTIAL_CONTENT {
            if !response_range_starts_at(&response, offset) {
                let _ = fs::remove_file(part_path);
                last_error = "Remote checkpoint range did not match the partial file".to_owned();
                thread::sleep(Duration::from_secs(1));
                continue;
            }

            true
        } else {
            false
        };

        if let Err(error) = stream_response(response, part_path, append) {
            last_error = error;
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        let length = fs::metadata(part_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        if length == spec.size_bytes {
            if file_matches_spec(part_path, spec)? {
                return Ok(());
            }

            fs::remove_file(part_path)
                .map_err(|_| "Corrupt downloaded checkpoint could not be removed".to_owned())?;

            last_error = "Downloaded checkpoint failed pinned integrity verification and was reset"
                .to_owned();

            thread::sleep(Duration::from_secs(1));
            continue;
        }

        if length > spec.size_bytes {
            let _ = fs::remove_file(part_path);
            last_error = "Downloaded checkpoint exceeded the expected size".to_owned();
        } else {
            last_error = format!(
                "Checkpoint download is incomplete ({length}/{})",
                spec.size_bytes
            );
        }

        thread::sleep(Duration::from_secs(1));
    }

    Err(last_error)
}

fn backup_managed_corrupt_asset(final_path: &Path) -> Result<PathBuf, String> {
    let parent = final_path
        .parent()
        .ok_or_else(|| "Managed checkpoint directory is unavailable".to_owned())?;

    let stem = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("checkpoint");

    let backup = parent.join(format!(".{stem}.ai-os-backup-{}", Uuid::new_v4()));

    fs::rename(final_path, &backup)
        .map_err(|_| "Damaged managed checkpoint could not be backed up".to_owned())?;

    Ok(backup)
}

fn install_or_repair_asset(
    models_root: &Path,
    manifest_path: &Path,
    spec: &ManagedAssetSpec,
) -> Result<InstalledAsset, String> {
    let (final_path, part_path) = validate_destination(models_root, spec)?;

    let state = inspect_existing_asset(&final_path, manifest_path, spec)?;

    match state {
        ExistingAssetState::ExactManaged => {
            write_manifest_atomic(manifest_path, &profile_manifest(spec))?;

            return Ok(InstalledAsset {
                action: "already-installed".to_owned(),
                final_path,
                backup_path: None,
            });
        }
        ExistingAssetState::ExactUnmanaged => {
            // This is not silent promotion: the user explicitly invoked
            // Setup / Repair and the bytes exactly match the pinned asset.
            write_manifest_atomic(manifest_path, &profile_manifest(spec))?;

            return Ok(InstalledAsset {
                action: "adopted".to_owned(),
                final_path,
                backup_path: None,
            });
        }
        ExistingAssetState::Missing | ExistingAssetState::ManagedNeedsRepair => {}
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(2 * 60 * 60))
        .user_agent("AI-OS/1.0 Generative-Media-Setup")
        .build()
        .map_err(|_| "Managed checkpoint HTTP client could not be created".to_owned())?;

    download_with_resume(&client, spec, &part_path)?;

    if !file_matches_spec(&part_path, spec)? {
        return Err(
            "Downloaded checkpoint failed pinned size/SHA-256 verification; \
             the partial file was kept for diagnosis or retry."
                .to_owned(),
        );
    }

    let mut backup_path = None;

    if state == ExistingAssetState::ManagedNeedsRepair {
        backup_path = Some(backup_managed_corrupt_asset(&final_path)?);
    }

    if let Err(error) = fs::rename(&part_path, &final_path) {
        if let Some(backup) = backup_path.as_ref() {
            let _ = fs::rename(backup, &final_path);
        }

        return Err(format!(
            "Verified checkpoint could not be installed atomically: {error}"
        ));
    }

    if !file_matches_spec(&final_path, spec)? {
        if let Some(backup) = backup_path.as_ref() {
            let _ = fs::remove_file(&final_path);
            let _ = fs::rename(backup, &final_path);
        }

        return Err("Installed checkpoint failed post-install integrity verification".to_owned());
    }

    if let Err(error) = write_manifest_atomic(manifest_path, &profile_manifest(spec)) {
        return Err(format!(
            "Checkpoint was installed but its managed profile manifest failed: {error}"
        ));
    }

    Ok(InstalledAsset {
        action: if state == ExistingAssetState::ManagedNeedsRepair {
            "repaired".to_owned()
        } else {
            "installed".to_owned()
        },
        final_path,
        backup_path,
    })
}

#[cfg(target_os = "macos")]
fn setup_comfyui_managed_profile_blocking() -> Result<ManagedProfileSetupResult, String> {
    let installation = select_usable_desktop_installation()?;

    let models_root = resolve_desktop_model_root(&installation.instance_id)?
        .ok_or_else(|| "Comfy Desktop models root is not configured".to_owned())?;

    let manifest_path = managed_profile_manifest_path()?;
    let spec = ManagedAssetSpec::bootstrap();

    let installed = install_or_repair_asset(&models_root, &manifest_path, &spec)?;

    let report =
        probe_comfyui_installation_profile_readiness(&installation, Duration::from_secs(150));

    if !report.evidence.workflow_ready {
        return Err(format!(
            "Managed checkpoint installed, but the ComfyUI workflow is not compatible: {:?}",
            report.diagnostics
        ));
    }

    if !report.evidence.required_assets_ready {
        return Err(format!(
            "Managed checkpoint installed, but running ComfyUI does not advertise it: {:?}",
            report.diagnostics
        ));
    }

    if !report.evidence.custom_nodes_ready {
        return Err(format!(
            "Managed checkpoint installed, but required nodes are unavailable: {:?}",
            report.diagnostics
        ));
    }

    if !report.evidence.integrity_ok {
        return Err(format!(
            "Managed checkpoint installed, but profile integrity verification failed: {:?}",
            report.diagnostics
        ));
    }

    Ok(ManagedProfileSetupResult {
        profile_id: CORE_TEXT_TO_IMAGE_PROFILE_ID.to_owned(),
        action: installed.action,
        checkpoint_file: BOOTSTRAP_FILE.to_owned(),
        checkpoint_path: installed.final_path.to_string_lossy().into_owned(),
        backup_path: installed
            .backup_path
            .map(|path| path.to_string_lossy().into_owned()),
        size_bytes: BOOTSTRAP_SIZE,
        sha256: BOOTSTRAP_SHA256.to_owned(),
        license: BOOTSTRAP_LICENSE.to_owned(),
        workflow_ready: report.evidence.workflow_ready,
        required_assets_ready: report.evidence.required_assets_ready,
        custom_nodes_ready: report.evidence.custom_nodes_ready,
        integrity_ok: report.evidence.integrity_ok,
        smoke_generation_checked: report.smoke_generation_checked,
        output_retrieval_checked: report.output_retrieval_checked,
        readiness: report.readiness(),
    })
}

#[tauri::command]
pub async fn setup_comfyui_managed_profile(
    confirmed: bool,
) -> Result<ManagedProfileSetupResult, String> {
    if !confirmed {
        return Err("Local image-generation setup requires explicit user confirmation".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        return tokio::task::spawn_blocking(setup_comfyui_managed_profile_blocking)
            .await
            .map_err(|_| "ComfyUI Setup / Repair worker failed".to_owned())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("ComfyUI managed profile setup is not implemented on this platform yet".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
    };

    fn fixture_spec(url: String, payload: &[u8]) -> ManagedAssetSpec {
        ManagedAssetSpec {
            source_url: url,
            relative_path: "checkpoints/fixture.safetensors".to_owned(),
            size_bytes: payload.len() as u64,
            sha256: sha256_bytes(payload),
        }
    }

    #[test]
    fn exact_explicit_asset_can_be_adopted_but_wrong_unmanaged_file_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        let checkpoints = models.join("checkpoints");
        fs::create_dir_all(&checkpoints).unwrap();

        let payload = b"managed fixture";
        let spec = fixture_spec("http://127.0.0.1/unused".to_owned(), payload);
        let final_path = models.join(&spec.relative_path);
        let manifest = root.path().join("profile.json");

        fs::write(&final_path, payload).unwrap();

        assert_eq!(
            inspect_existing_asset(&final_path, &manifest, &spec).unwrap(),
            ExistingAssetState::ExactUnmanaged
        );

        fs::write(&final_path, b"different").unwrap();

        assert!(inspect_existing_asset(&final_path, &manifest, &spec)
            .unwrap_err()
            .contains("will not overwrite"));
    }

    #[test]
    fn managed_corruption_is_repairable_and_manifest_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        let checkpoints = models.join("checkpoints");
        fs::create_dir_all(&checkpoints).unwrap();

        let payload = b"expected payload";
        let spec = fixture_spec("http://127.0.0.1/unused".to_owned(), payload);
        let final_path = models.join(&spec.relative_path);
        let manifest_path = root.path().join("profile.json");

        fs::write(&final_path, b"corrupt").unwrap();
        write_manifest_atomic(&manifest_path, &profile_manifest(&spec)).unwrap();

        assert_eq!(
            inspect_existing_asset(&final_path, &manifest_path, &spec).unwrap(),
            ExistingAssetState::ManagedNeedsRepair
        );

        let parsed: ManagedProfileManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();

        assert_eq!(parsed.checkpoint.relative_path, spec.relative_path);

        let temporary_manifest_count = fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count();

        assert_eq!(temporary_manifest_count, 0);
    }

    #[test]
    fn interrupted_partial_download_resumes_with_http_range() {
        let payload = b"0123456789abcdef".to_vec();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let root = tempfile::tempdir().unwrap();
        let part = root.path().join("fixture.part");

        fs::write(&part, &payload[..6]).unwrap();

        let server_payload = payload.clone();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];

            loop {
                let read = stream.read(&mut buffer).unwrap();

                if read == 0 {
                    break;
                }

                request.extend_from_slice(&buffer[..read]);

                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }

            let request_text = String::from_utf8_lossy(&request).to_ascii_lowercase();

            assert!(
                request_text.contains("range: bytes=6-"),
                "resume request did not contain the expected Range header"
            );

            let remaining = &server_payload[6..];

            let headers = format!(
                "HTTP/1.1 206 Partial Content\r\n\
                 Content-Length: {}\r\n\
                 Content-Range: bytes 6-15/16\r\n\
                 Connection: close\r\n\
                 \r\n",
                remaining.len()
            );

            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(remaining).unwrap();
            stream.flush().unwrap();
        });

        let spec = fixture_spec(format!("http://{address}/fixture"), &payload);

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        download_with_resume(&client, &spec, &part).unwrap();

        server.join().unwrap();

        assert_eq!(fs::read(&part).unwrap(), payload);
        assert!(file_matches_spec(&part, &spec).unwrap());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn configured_profile_pending_execution_is_not_broken() {
        use crate::generative_media::{
            comfyui_macos::ComfyUiMacOsProfileReadinessReport,
            provider::LocalMediaReadinessEvidence,
        };

        let report = ComfyUiMacOsProfileReadinessReport {
            instance_id: Some("test".to_owned()),
            profile_id: CORE_TEXT_TO_IMAGE_PROFILE_ID.to_owned(),
            evidence: LocalMediaReadinessEvidence {
                engine_installed: true,
                engine_startable: true,
                api_reachable: true,
                workflow_ready: true,
                required_assets_ready: true,
                custom_nodes_ready: true,
                integrity_ok: true,
                smoke_generation_ok: false,
                output_retrieval_ok: false,
            },
            smoke_generation_checked: false,
            output_retrieval_checked: false,
            diagnostics: Vec::new(),
        };

        assert_eq!(
            report.readiness(),
            LocalMediaReadiness::InstalledNotConfigured
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "downloads/verifies the real managed ComfyUI bootstrap checkpoint"]
    fn live_setup_or_repair_configures_real_profile() {
        let result = setup_comfyui_managed_profile_blocking().expect("real GM-3 Setup / Repair");

        assert!(result.workflow_ready);
        assert!(result.required_assets_ready);
        assert!(result.custom_nodes_ready);
        assert!(result.integrity_ok);

        assert!(!result.smoke_generation_checked);
        assert!(!result.output_retrieval_checked);

        assert_eq!(
            result.readiness,
            LocalMediaReadiness::InstalledNotConfigured
        );

        eprintln!(
            "LIVE_GM3_SETUP action={} profile={} workflow={} assets={} custom_nodes={} integrity={} smoke_checked={} output_checked={} state={:?}",
            result.action,
            result.profile_id,
            result.workflow_ready,
            result.required_assets_ready,
            result.custom_nodes_ready,
            result.integrity_ok,
            result.smoke_generation_checked,
            result.output_retrieval_checked,
            result.readiness,
        );
    }
}
