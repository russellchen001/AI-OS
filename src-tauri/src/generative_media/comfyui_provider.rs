use super::{
    comfyui_macos::{
        execute_ready_managed_text_to_image, probe_ready_managed_profile,
        ComfyUiManagedProfileReady,
    },
    domain::{
        LawfulContentCompatibility, MediaCapability, MediaError, MediaErrorCode, MediaKind,
        MediaOutput, MediaProgress, MediaProviderSource, MediaRequest, MediaResult,
    },
    provider::{LocalMediaReadiness, MediaProvider, MediaProviderMetadata},
};
use crate::provider_selection::{AuthorizationKind, AuthorizationState, ProviderInterfaceKind};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::Duration,
};
use uuid::Uuid;

pub(crate) const COMFYUI_LOCAL_PROVIDER_ID: &str = "comfyui";

pub(crate) struct ComfyUiLocalProvider {
    metadata: MediaProviderMetadata,
    ready: ComfyUiManagedProfileReady,
}

impl ComfyUiLocalProvider {
    pub(crate) fn discover_ready() -> Result<Self, String> {
        let ready = probe_ready_managed_profile(Duration::from_secs(150))?;

        let metadata = MediaProviderMetadata {
            provider_id: COMFYUI_LOCAL_PROVIDER_ID.to_owned(),
            provider_instance_id: Some(ready.instance_id.clone()),
            source: MediaProviderSource::Local,
            interface_kind: ProviderInterfaceKind::NativeStructured,
            authorization_kind: AuthorizationKind::None,
            authorization_state: AuthorizationState::Connected,
            authorization_ref: None,
            available: true,
            local_readiness: Some(LocalMediaReadiness::Ready),
            priority: 100,
            capabilities: vec![MediaCapability::TextToImage],
            lawful_content_compatibility: LawfulContentCompatibility::Unknown,
            recommended_cloud: false,
            supports_progress: true,
            supports_cancellation: true,
            supports_cost_estimation: false,
        };

        Ok(Self { metadata, ready })
    }
}

impl MediaProvider for ComfyUiLocalProvider {
    fn metadata(&self) -> &MediaProviderMetadata {
        &self.metadata
    }

    fn execute(
        &self,
        request: &MediaRequest,
        report: &mut dyn FnMut(MediaProgress),
    ) -> Result<MediaResult, MediaError> {
        if request.capability != MediaCapability::TextToImage {
            return Err(media_error(
                MediaErrorCode::UnsupportedCapability,
                "The local ComfyUI provider currently supports text-to-image only.",
                false,
            ));
        }

        if request.intent.media_kind != MediaKind::Image {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "Text-to-image requires an image CreativeIntent.",
                false,
            ));
        }

        let prompt = request.intent.request.trim();

        if prompt.is_empty() {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "The image request is empty.",
                false,
            ));
        }

        if request
            .options
            .profile_id
            .as_deref()
            .is_some_and(|profile| profile != self.ready.profile_id)
        {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "The selected local generation profile is not the Ready ComfyUI profile.",
                false,
            ));
        }

        if request
            .options
            .model_id
            .as_deref()
            .is_some_and(|model| model != self.ready.checkpoint_name)
        {
            return Err(media_error(
                MediaErrorCode::InvalidRequest,
                "The selected local model is not the checkpoint proven Ready for this provider instance.",
                false,
            ));
        }

        report(MediaProgress {
            phase: "starting".to_owned(),
            completed_units: Some(0),
            total_units: Some(3),
            message: "Starting the local Generative Media provider.".to_owned(),
        });

        report(MediaProgress {
            phase: "generating".to_owned(),
            completed_units: Some(1),
            total_units: Some(3),
            message: "Generating the image with local ComfyUI.".to_owned(),
        });

        let generated = execute_ready_managed_text_to_image(
            prompt,
            Duration::from_secs(150),
            Duration::from_secs(300),
        )
        .map_err(|message| {
            let lower = message.to_lowercase();

            let code = if lower.contains("no longer")
                || lower.contains("not ready")
                || lower.contains("unavailable")
                || lower.contains("stale")
                || lower.contains("version")
            {
                MediaErrorCode::NotReady
            } else {
                MediaErrorCode::ProviderError
            };

            media_error(code, &message, code == MediaErrorCode::ProviderError)
        })?;

        if generated.ready.instance_id
            != self
                .metadata
                .provider_instance_id
                .as_deref()
                .unwrap_or_default()
        {
            return Err(media_error(
                MediaErrorCode::NotReady,
                "The active ComfyUI installation changed after provider discovery.",
                false,
            ));
        }

        if generated.ready.profile_id != self.ready.profile_id
            || generated.ready.profile_version != self.ready.profile_version
            || generated.ready.checkpoint_sha256 != self.ready.checkpoint_sha256
            || generated.ready.comfyui_version != self.ready.comfyui_version
        {
            return Err(media_error(
                MediaErrorCode::NotReady,
                "The managed ComfyUI Ready identity changed before execution.",
                false,
            ));
        }

        let stored = store_local_asset(&generated.generated.bytes, &generated.generated.mime_type)
            .map_err(|message| media_error(MediaErrorCode::ProviderError, &message, true))?;

        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(&generated.generated.bytes);
            format!("{:x}", hasher.finalize())
        };

        report(MediaProgress {
            phase: "completed".to_owned(),
            completed_units: Some(3),
            total_units: Some(3),
            message: "Local image generation completed.".to_owned(),
        });

        Ok(MediaResult {
            provider_id: COMFYUI_LOCAL_PROVIDER_ID.to_owned(),
            provider_instance_id: self.metadata.provider_instance_id.clone(),
            outputs: vec![MediaOutput {
                id: stored.asset_id.clone(),
                kind: MediaKind::Image,
                mime_type: generated.generated.mime_type.clone(),
                handle: stored.handle.clone(),
            }],
            metadata: serde_json::json!({
                "source": "local",
                "engine": "comfyui",
                "profileId": generated.ready.profile_id,
                "profileVersion": generated.ready.profile_version,
                "checkpointSha256": generated.ready.checkpoint_sha256,
                "comfyuiVersion": generated.ready.comfyui_version,
                "promptId": generated.generated.prompt_id,
                "outputBytes": generated.generated.bytes.len(),
                "outputSha256": digest,
                "assetId": stored.asset_id,
            }),
        })
    }
}

fn media_error(code: MediaErrorCode, message: &str, retryable: bool) -> MediaError {
    MediaError {
        code,
        message: message.to_owned(),
        retryable,
    }
}

pub(crate) struct StoredAsset {
    pub(crate) asset_id: String,
    pub(crate) handle: String,
}

fn local_asset_root() -> Result<PathBuf, String> {
    let data_root =
        dirs::data_dir().ok_or_else(|| "Application data directory is unavailable".to_owned())?;

    Ok(data_root
        .join("AI-OS")
        .join("generative-media")
        .join("assets"))
}

fn extension_for_mime(mime_type: &str) -> Result<&'static str, String> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        "video/mp4" => Ok("mp4"),
        _ => Err("Generated media MIME type is unsupported".to_owned()),
    }
}

pub(crate) fn store_local_asset(bytes: &[u8], mime_type: &str) -> Result<StoredAsset, String> {
    let root = local_asset_root()?;
    store_local_asset_at(&root, bytes, mime_type)
}

pub(crate) fn store_local_asset_at(
    root: &std::path::Path,
    bytes: &[u8],
    mime_type: &str,
) -> Result<StoredAsset, String> {
    if bytes.is_empty() {
        return Err("Generated image asset is empty".to_owned());
    }

    let extension = extension_for_mime(mime_type)?;

    fs::create_dir_all(root)
        .map_err(|_| "Generative Media asset directory could not be created".to_owned())?;

    let asset_id = Uuid::new_v4().simple().to_string();
    let filename = format!("{asset_id}.{extension}");
    let final_path = root.join(&filename);
    let temporary = root.join(format!(".{filename}.{}.tmp", Uuid::new_v4().simple()));

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "Generative Media temporary asset could not be created".to_owned())?;

    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);

        return Err(format!(
            "Generative Media temporary asset could not be written: {error}"
        ));
    }

    if let Err(error) = fs::rename(&temporary, &final_path) {
        let _ = fs::remove_file(&temporary);

        return Err(format!(
            "Generative Media asset could not be committed atomically: {error}"
        ));
    }

    Ok(StoredAsset {
        asset_id,
        handle: format!("asset://generative-media/{filename}"),
    })
}

pub(crate) fn resolve_local_asset_handle(handle: &str) -> Result<PathBuf, String> {
    let filename = handle
        .strip_prefix("asset://generative-media/")
        .filter(|value| {
            !value.is_empty()
                && !value.contains('/')
                && !value.contains('\\')
                && !value.contains("..")
        })
        .ok_or_else(|| "Generative Media asset handle is invalid".to_owned())?;

    Ok(local_asset_root()?.join(filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_asset_handle_is_opaque_and_resolves_inside_ai_os_storage() {
        let root = tempfile::tempdir().unwrap();

        let stored =
            store_local_asset_at(root.path(), b"\x89PNG\r\n\x1a\nfixture", "image/png").unwrap();

        assert!(stored.handle.starts_with("asset://generative-media/"));

        assert!(!stored.handle.contains("/Users/"));
        assert!(!stored
            .handle
            .contains(root.path().to_string_lossy().as_ref()));

        let filename = stored
            .handle
            .strip_prefix("asset://generative-media/")
            .unwrap();

        let path = root.path().join(filename);

        assert!(path.is_file());
        assert_eq!(fs::read(path).unwrap(), b"\x89PNG\r\n\x1a\nfixture");
    }

    #[test]
    fn invalid_asset_handles_cannot_escape_managed_storage() {
        for handle in [
            "asset://generative-media/../secret",
            "asset://generative-media/a/b.png",
            "asset://generative-media/../../x.png",
            "file:///tmp/x.png",
        ] {
            assert!(resolve_local_asset_handle(handle).is_err());
        }
    }
}
