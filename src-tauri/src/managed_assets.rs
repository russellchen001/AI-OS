//! Downloading and verifying one large managed asset.
//!
//! Extracted from the ComfyUI installer so Cognitive Distillation's speech and
//! vision models take the SAME proven path rather than a second downloader
//! written from scratch. The awkward parts are what make it worth sharing:
//! resuming an interrupted multi-gigabyte download over HTTP Range, refusing a
//! partial file larger than the asset, resetting one the server will not resume,
//! retrying, and verifying sha256 before anything is moved into place.
//!
//! A file that downloads completely but hashes wrong is a corrupt file. Treating
//! it as usable is how a silent failure reaches a person weeks later, so it never
//! reaches its destination.

use reqwest::blocking::{Client, Response};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    thread,
    time::Duration,
};

const BOOTSTRAP_FILE: &str = "v1-5-pruned-emaonly-fp16.safetensors";
const BOOTSTRAP_SIZE: u64 = 2_132_696_762;
const BOOTSTRAP_SHA256: &str = "e9476a13728cd75d8279f6ec8bad753a66a1957ca375a1464dc63b37db6e3916";

#[derive(Debug, Clone)]
pub(crate) struct ManagedAssetSpec {
    pub source_url: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

impl ManagedAssetSpec {
    pub(crate) fn bootstrap() -> Self {
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

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
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

pub(crate) fn file_matches_spec(path: &Path, spec: &ManagedAssetSpec) -> Result<bool, String> {
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

pub(crate) fn download_with_resume(
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

/// Make sure one asset is present and intact at `destination`.
///
/// Returns without touching the network when the file is already there and its
/// hash matches. Otherwise it resumes into a `.part` beside the destination,
/// verifies, and only then moves it into place — so an interrupted or corrupt
/// download can never be mistaken for an installed asset.
pub(crate) fn ensure_asset(
    client: &Client,
    spec: &ManagedAssetSpec,
    destination: &Path,
) -> Result<(), String> {
    if file_matches_spec(destination, spec)? {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "Managed asset directory could not be created".to_owned())?;
    }

    let part_path = destination.with_extension("part");
    download_with_resume(client, spec, &part_path)?;

    if !file_matches_spec(&part_path, spec)? {
        // Kept rather than deleted: a file that downloaded fully and hashed wrong
        // is worth inspecting, and deleting it invites an identical retry.
        return Err(
            "Managed asset failed integrity verification; the partial file was kept for diagnosis"
                .to_owned(),
        );
    }

    fs::rename(&part_path, destination)
        .map_err(|_| "Verified managed asset could not be moved into place".to_owned())
}

#[cfg(test)]
mod ensure_tests {
    use super::*;

    fn spec_for(payload: &[u8], url: String) -> ManagedAssetSpec {
        ManagedAssetSpec {
            source_url: url,
            relative_path: "asset.bin".to_owned(),
            size_bytes: payload.len() as u64,
            sha256: sha256_bytes(payload),
        }
    }

    /// An asset already present and correct must not be downloaded again. The URL
    /// is deliberately unreachable: if it were contacted, this would fail.
    #[test]
    fn an_intact_asset_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("asset.bin");
        let payload = b"already here";
        fs::write(&destination, payload).unwrap();

        ensure_asset(
            &Client::new(),
            &spec_for(payload, "http://127.0.0.1:1/never".to_owned()),
            &destination,
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), payload);
    }

    /// A file of the right size whose contents are wrong is NOT the asset, and
    /// must not be adopted just because it is the right length.
    #[test]
    fn a_wrong_file_of_the_right_size_is_not_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("asset.bin");
        fs::write(&destination, b"XXXXXXXXXXXX").unwrap();

        let error = ensure_asset(
            &Client::new(),
            &spec_for(b"already here", "http://127.0.0.1:1/never".to_owned()),
            &destination,
        )
        .unwrap_err();
        // It tried to download rather than accepting the impostor.
        assert!(!error.is_empty());
        assert_eq!(fs::read(&destination).unwrap(), b"XXXXXXXXXXXX");
    }
}
