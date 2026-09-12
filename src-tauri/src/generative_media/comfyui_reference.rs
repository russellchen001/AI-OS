use super::{
    comfyui_macos::{
        active_input_directory, select_usable_desktop_installation, start_comfyui_backend_and_wait,
        ComfyUiManagedProfileReady,
    },
    comfyui_provider::resolve_local_asset_handle,
    domain::{MediaError, MediaErrorCode, MediaKind, MediaReference, ReferenceSpec},
    reference_analysis::{
        normalize_reference_response, ReferenceAnalysisAdapter, ReferenceAnalyzerIdentity,
        MAX_REFERENCE_BYTES,
    },
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(crate) const COMFYUI_VLM_ADAPTER_ID: &str = "comfyui-vlm";
pub(crate) const COMFYUI_VLM_UPSTREAM_REVISION: &str = "3f9612774e862f94d1dfbe3a1b36375a52870382";
pub(crate) const COMFYUI_VLM_MODEL: &str = "Qwen 3 VL 2B Instruct";
pub(crate) const COMFYUI_VLM_MODEL_ID: &str = "Qwen/Qwen3-VL-2B-Instruct";
pub(crate) const REFERENCE_READY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceVlmReadinessRecord {
    pub contract_version: u32,
    pub provider_instance_id: String,
    pub upstream_revision: String,
    pub model_id: String,
    pub model_snapshot_path: String,
    pub modern_vlm_schema_ok: bool,
    pub video_reasoner_schema_ok: bool,
    pub image_smoke_ok: bool,
    pub smoke_output_sha256: String,
    pub smoke_reference_spec_sha256: String,
}

pub(crate) struct ComfyUiVlmReferenceAdapter {
    ready: ComfyUiManagedProfileReady,
}

impl ComfyUiVlmReferenceAdapter {
    pub(crate) fn new(ready: ComfyUiManagedProfileReady) -> Self {
        Self { ready }
    }
}

impl ReferenceAnalysisAdapter for ComfyUiVlmReferenceAdapter {
    fn identity(&self) -> ReferenceAnalyzerIdentity {
        ReferenceAnalyzerIdentity {
            provider_id: COMFYUI_VLM_ADAPTER_ID.to_owned(),
            provider_instance_id: Some(self.ready.instance_id.clone()),
        }
    }

    fn analyze(&self, reference: &MediaReference) -> Result<ReferenceSpec, MediaError> {
        analyze_reference_with_identity(&self.ready, reference, &self.identity())
    }
}

pub(crate) fn readiness_record_path() -> Result<PathBuf, String> {
    let root = dirs::data_dir()
        .ok_or_else(|| "Application data directory is unavailable".to_owned())?
        .join("AI-OS")
        .join("generative-media");
    Ok(root.join("reference-vlm-readiness.json"))
}

pub(crate) fn reference_vlm_is_ready(ready: &ComfyUiManagedProfileReady) -> bool {
    let Ok(path) = readiness_record_path() else {
        return false;
    };
    reference_vlm_is_ready_at(&path, ready)
}

fn reference_vlm_is_ready_at(path: &Path, ready: &ComfyUiManagedProfileReady) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(record) = serde_json::from_slice::<ReferenceVlmReadinessRecord>(&bytes) else {
        return false;
    };

    record.contract_version == REFERENCE_READY_VERSION
        && record.provider_instance_id == ready.instance_id
        && record.upstream_revision == COMFYUI_VLM_UPSTREAM_REVISION
        && record.model_id == COMFYUI_VLM_MODEL_ID
        && record.modern_vlm_schema_ok
        && record.video_reasoner_schema_ok
        && record.image_smoke_ok
        && record.smoke_output_sha256.len() == 64
        && record.smoke_reference_spec_sha256.len() == 64
        && model_snapshot_is_complete(Path::new(&record.model_snapshot_path))
}

fn model_snapshot_is_complete(path: &Path) -> bool {
    path.join("config.json").is_file()
        && fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "safetensors")
            })
}

fn analyze_reference_with_identity(
    ready: &ComfyUiManagedProfileReady,
    reference: &MediaReference,
    analyzer: &ReferenceAnalyzerIdentity,
) -> Result<ReferenceSpec, MediaError> {
    if !reference_vlm_is_ready(ready) {
        return Err(error(
            MediaErrorCode::NotReady,
            "Local Reference VLM requires explicit Setup / Repair and real smoke evidence.",
            false,
        ));
    }

    let installation = select_usable_desktop_installation().map_err(not_ready)?;
    if installation.instance_id != ready.instance_id {
        return Err(error(
            MediaErrorCode::NotReady,
            "The selected ComfyUI Provider instance changed after Reference VLM readiness.",
            false,
        ));
    }
    let bytes = reference_bytes(reference)?;
    let extension = reference_extension(reference.kind, &bytes)?;
    let input_root = active_input_directory(&installation).map_err(not_ready)?;
    let temporary = TemporaryReference::create(&input_root, extension, &bytes)
        .map_err(|message| error(MediaErrorCode::ProviderError, &message, false))?;
    let mut backend = start_comfyui_backend_and_wait(&installation, Duration::from_secs(150))
        .map_err(not_ready)?;

    let result = (|| {
        validate_node_schemas(&backend.endpoint)?;
        let response = execute_reference_workflow(
            &backend.endpoint,
            reference.kind,
            temporary.filename(),
            Duration::from_secs(300),
        )?;
        normalize_reference_response(reference, analyzer, &bytes, &response)
            .map_err(|message| error(MediaErrorCode::ProviderError, &message, false))
    })();

    backend.stop();
    result
}

pub(crate) fn validate_node_schemas(endpoint: &str) -> Result<(bool, bool), MediaError> {
    let client = http_client(Duration::from_secs(30))?;
    let object_info = get_json(&client, endpoint, "/object_info")?;
    let modern = object_info
        .get("ModernVLM")
        .and_then(|node| node.pointer("/input/required"))
        .is_some_and(|required| {
            [
                "prompt",
                "model",
                "custom_model_id",
                "memory_mode",
                "max_new_tokens",
            ]
            .iter()
            .all(|field| required.get(field).is_some())
        });
    let video = object_info
        .get("VLMVideoTemporalReasoner")
        .and_then(|node| node.pointer("/input/required"))
        .is_some_and(|required| {
            ["frames", "fps", "task", "question", "model", "max_frames"]
                .iter()
                .all(|field| required.get(field).is_some())
        });

    if !modern || !video {
        return Err(error(
            MediaErrorCode::NotReady,
            "ComfyUI Reference VLM node schemas are missing or incompatible.",
            false,
        ));
    }
    Ok((modern, video))
}

pub(crate) fn execute_reference_workflow(
    endpoint: &str,
    kind: MediaKind,
    filename: &str,
    timeout: Duration,
) -> Result<String, MediaError> {
    let workflow = match kind {
        MediaKind::Image => image_workflow(filename),
        MediaKind::Video => video_workflow(filename),
    };
    let client = http_client(timeout)?;
    let response = client
        .post(endpoint_url(endpoint, "/prompt")?)
        .json(&serde_json::json!({"prompt": workflow}))
        .send()
        .map_err(|_| unavailable("ComfyUI Reference VLM could not be reached"))?;
    let status = response.status();
    if !status.is_success() {
        let detail = response
            .text()
            .unwrap_or_default()
            .chars()
            .take(2_048)
            .collect::<String>();
        return Err(error(
            MediaErrorCode::ProviderError,
            &format!("ComfyUI rejected Reference Analysis with HTTP {status}: {detail}"),
            false,
        ));
    }
    let payload: Value = response.json().map_err(|_| {
        error(
            MediaErrorCode::ProviderError,
            "ComfyUI returned invalid JSON",
            false,
        )
    })?;
    let prompt_id = payload
        .get("prompt_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            error(
                MediaErrorCode::ProviderError,
                "ComfyUI returned no prompt id",
                false,
            )
        })?;
    wait_for_text(&client, endpoint, prompt_id, timeout)
}

pub(crate) fn run_image_setup_smoke(
    endpoint: &str,
    input_root: &Path,
) -> Result<String, MediaError> {
    let bytes = image_setup_smoke_fixture()?;
    let temporary = TemporaryReference::create(input_root, "png", &bytes)
        .map_err(|message| error(MediaErrorCode::ProviderError, &message, false))?;
    execute_reference_workflow(
        endpoint,
        MediaKind::Image,
        temporary.filename(),
        Duration::from_secs(600),
    )
}

pub(crate) fn image_setup_smoke_fixture() -> Result<Vec<u8>, MediaError> {
    BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAYElEQVR4nO3PwQkAIBDAsBPcf2UdwkcQmgnaNXPmZ1sHvGpAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAa0BrQGtAuzD7Af1qJsBlAAAAAElFTkSuQmCC")
        .map_err(|_| error(MediaErrorCode::ProviderError, "Reference smoke fixture is invalid", false))
}

fn image_workflow(filename: &str) -> Value {
    serde_json::json!({
        "1": {"class_type":"LoadImage","inputs":{"image":filename}},
        "2": {"class_type":"ModernVLM","inputs":{
            "prompt": reference_prompt(false), "model":COMFYUI_VLM_MODEL,
            "custom_model_id":"", "memory_mode":"ComfyUI managed (BF16)",
            "max_new_tokens":512, "temperature":0.1, "top_p":0.9,
            "image":["1",0], "system_prompt":"You are a precise visual reference analyst.",
            "attention_mode":"Auto (SDPA)", "enable_thinking":false,
            "unload_after":false, "stream_output":false
        }},
        "3": {"class_type":"ViewText","inputs":{"text":["2",0]}}
    })
}

fn video_workflow(filename: &str) -> Value {
    serde_json::json!({
        "1":{"class_type":"LoadVideo","inputs":{"file":filename}},
        "2":{"class_type":"GetVideoComponents","inputs":{"video":["1",0]}},
        "3":{"class_type":"VLMVideoTemporalReasoner","inputs":{
            "frames":["2",0], "fps":["2",2], "task":"Reference generation analysis",
            "question":reference_prompt(true), "model":COMFYUI_VLM_MODEL,
            "custom_model_id":"", "memory_mode":"ComfyUI managed (BF16)",
            "max_frames":12, "max_events":24, "max_new_tokens":768,
            "strategy":"Hybrid: scene + motion + tracks", "minimum_gap_seconds":0.15,
            "analysis_max_side":448, "attention_mode":"Auto (SDPA)",
            "enable_thinking":false, "strict_output":true, "unload_after":false,
            "stream_output":false
        }},
        "4":{"class_type":"ViewText","inputs":{"text":["3",0]}}
    })
}

fn reference_prompt(video: bool) -> &'static str {
    if video {
        "Return JSON only with camelCase keys: summary string, constraints string array, temporalEvents string array. Describe subject, scene, composition, camera, style, lighting, mood, color, motion and temporal behavior useful for a later generation request."
    } else {
        "Return JSON only with camelCase keys: summary string, constraints string array, temporalEvents empty array. Describe subject, scene, composition, camera, style, lighting, mood and color useful for a later generation request."
    }
}

fn wait_for_text(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
    timeout: Duration,
) -> Result<String, MediaError> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let history = get_json(client, endpoint, &format!("/history/{prompt_id}"))?;
        if let Some(entry) = history.get(prompt_id) {
            if let Some(status) = entry.pointer("/status/status_str").and_then(Value::as_str) {
                if status == "error" {
                    let detail = execution_error_detail(entry)
                        .unwrap_or_else(|| "no execution detail".to_owned());
                    let detail = detail.chars().take(2_048).collect::<String>();
                    return Err(error(
                        MediaErrorCode::ProviderError,
                        &format!("Reference VLM execution failed: {detail}"),
                        false,
                    ));
                }
            }
            if let Some(outputs) = entry.get("outputs") {
                if let Some(text) = first_nonempty_text(outputs) {
                    return Ok(text);
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(unavailable(
        "Reference VLM did not complete before the timeout",
    ))
}

fn execution_error_detail(entry: &Value) -> Option<String> {
    let messages = entry.pointer("/status/messages")?.as_array()?;
    let error = messages.iter().rev().find_map(|message| {
        let parts = message.as_array()?;
        (parts.first()?.as_str()? == "execution_error")
            .then(|| parts.get(1))
            .flatten()
    })?;
    let error_type = error
        .get("exception_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    let message = error
        .get("exception_message")
        .and_then(Value::as_str)
        .unwrap_or("no message");
    let frame = error
        .get("traceback")
        .and_then(Value::as_array)
        .and_then(|traceback| traceback.last())
        .and_then(Value::as_str)
        .unwrap_or("no traceback");
    Some(format!("{error_type}: {message}; {frame}"))
}

fn first_nonempty_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Array(values) => values.iter().find_map(first_nonempty_text),
        Value::Object(values) => ["text", "string", "result", "output"]
            .into_iter()
            .filter_map(|key| values.get(key))
            .find_map(first_nonempty_text)
            .or_else(|| values.values().find_map(first_nonempty_text)),
        _ => None,
    }
}

fn reference_bytes(reference: &MediaReference) -> Result<Vec<u8>, MediaError> {
    let handle = reference.handle.trim();
    let bytes = if handle.starts_with("data:") {
        let encoded = handle
            .split_once(";base64,")
            .map(|(_, encoded)| encoded)
            .ok_or_else(|| {
                error(
                    MediaErrorCode::InvalidRequest,
                    "Reference data URL must be base64",
                    false,
                )
            })?;
        BASE64_STANDARD.decode(encoded).map_err(|_| {
            error(
                MediaErrorCode::InvalidRequest,
                "Reference base64 is invalid",
                false,
            )
        })?
    } else {
        let path = resolve_local_asset_handle(handle)
            .map_err(|message| error(MediaErrorCode::InvalidRequest, &message, false))?;
        fs::read(path).map_err(|_| {
            error(
                MediaErrorCode::InvalidRequest,
                "Reference asset is unavailable",
                false,
            )
        })?
    };
    if bytes.is_empty() || bytes.len() > MAX_REFERENCE_BYTES {
        return Err(error(
            MediaErrorCode::InvalidRequest,
            "Reference asset is empty or too large",
            false,
        ));
    }
    Ok(bytes)
}

fn reference_extension(kind: MediaKind, bytes: &[u8]) -> Result<&'static str, MediaError> {
    match kind {
        MediaKind::Video if bytes.get(4..8) == Some(b"ftyp") => Ok("mp4"),
        MediaKind::Image if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok("png"),
        MediaKind::Image if bytes.starts_with(b"\xff\xd8\xff") => Ok("jpg"),
        MediaKind::Image if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            Ok("webp")
        }
        _ => Err(error(
            MediaErrorCode::InvalidRequest,
            "Reference media signature is unsupported",
            false,
        )),
    }
}

struct TemporaryReference(PathBuf);

impl TemporaryReference {
    fn create(root: &Path, extension: &str, bytes: &[u8]) -> Result<Self, String> {
        fs::create_dir_all(root)
            .map_err(|_| "ComfyUI input directory is unavailable".to_owned())?;
        let path = root.join(format!(
            "ai-os-reference-{}.{}",
            Uuid::new_v4().simple(),
            extension
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|_| "Temporary reference could not be created".to_owned())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Temporary reference could not be written".to_owned())?;
        Ok(Self(path))
    }

    fn filename(&self) -> &str {
        self.0
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    }
}

impl Drop for TemporaryReference {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn http_client(timeout: Duration) -> Result<Client, MediaError> {
    Client::builder().timeout(timeout).build().map_err(|_| {
        error(
            MediaErrorCode::ProviderError,
            "Reference VLM HTTP client failed",
            false,
        )
    })
}

fn endpoint_url(endpoint: &str, route: &str) -> Result<String, MediaError> {
    let base = url::Url::parse(endpoint).map_err(|_| {
        error(
            MediaErrorCode::ProviderError,
            "ComfyUI endpoint is invalid",
            false,
        )
    })?;
    if base.scheme() != "http" || base.host_str() != Some("127.0.0.1") {
        return Err(error(
            MediaErrorCode::PolicyRejected,
            "Reference VLM must use loopback ComfyUI only",
            false,
        ));
    }
    Ok(format!("{}{}", endpoint.trim_end_matches('/'), route))
}

fn get_json(client: &Client, endpoint: &str, route: &str) -> Result<Value, MediaError> {
    let response = client
        .get(endpoint_url(endpoint, route)?)
        .send()
        .map_err(|_| unavailable("ComfyUI Reference VLM could not be reached"))?;
    if !response.status().is_success() {
        return Err(unavailable(
            "ComfyUI Reference VLM returned an unsuccessful response",
        ));
    }
    response.json().map_err(|_| {
        error(
            MediaErrorCode::ProviderError,
            "ComfyUI returned invalid JSON",
            false,
        )
    })
}

fn not_ready(message: String) -> MediaError {
    error(MediaErrorCode::NotReady, &message, false)
}

fn unavailable(message: &str) -> MediaError {
    error(MediaErrorCode::ProviderUnavailable, message, true)
}

fn error(code: MediaErrorCode, message: &str, retryable: bool) -> MediaError {
    MediaError {
        code,
        message: message.to_owned(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generative_media::comfyui_macos::probe_ready_managed_profile;

    #[test]
    fn workflows_use_local_oss_node_ids_and_disable_streaming() {
        let image = image_workflow("safe.png");
        let video = video_workflow("safe.mp4");
        assert_eq!(image["2"]["class_type"], "ModernVLM");
        assert_eq!(image["2"]["inputs"]["stream_output"], false);
        assert_eq!(video["3"]["class_type"], "VLMVideoTemporalReasoner");
        assert_eq!(video["3"]["inputs"]["model"], COMFYUI_VLM_MODEL);
    }

    #[test]
    fn text_extraction_ignores_empty_values() {
        let value = serde_json::json!({"node":{"text":["", "summary"]}});
        assert_eq!(first_nonempty_text(&value).as_deref(), Some("summary"));
    }

    #[test]
    fn reference_signature_validation_is_typed() {
        assert_eq!(
            reference_extension(MediaKind::Image, b"\x89PNG\r\n\x1a\n").unwrap(),
            "png"
        );
        assert!(reference_extension(MediaKind::Video, b"not-video").is_err());
    }

    #[test]
    fn setup_smoke_fixture_is_64_by_64_rgb_for_current_comfyui_pyav() {
        let bytes = image_setup_smoke_fixture().unwrap();
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 64);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 64);
        assert_eq!(bytes[25], 2, "PNG color type must remain truecolor RGB");
    }

    #[test]
    fn non_loopback_endpoint_is_rejected() {
        assert!(endpoint_url("https://example.com", "/object_info").is_err());
    }

    #[test]
    fn ready_requires_matching_real_smoke_evidence_and_model_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let snapshot = root.path().join("model-snapshot");
        fs::create_dir(&snapshot).unwrap();
        fs::write(snapshot.join("config.json"), b"{}").unwrap();
        fs::write(snapshot.join("model.safetensors"), b"weights").unwrap();
        let evidence = root.path().join("ready.json");
        let ready = ComfyUiManagedProfileReady {
            instance_id: "local-a".to_owned(),
            profile_id: "profile".to_owned(),
            profile_version: 1,
            checkpoint_name: "checkpoint".to_owned(),
            checkpoint_sha256: "b".repeat(64),
            comfyui_version: "1".to_owned(),
        };
        fs::write(
            &evidence,
            serde_json::to_vec(&ReferenceVlmReadinessRecord {
                contract_version: REFERENCE_READY_VERSION,
                provider_instance_id: ready.instance_id.clone(),
                upstream_revision: COMFYUI_VLM_UPSTREAM_REVISION.to_owned(),
                model_id: COMFYUI_VLM_MODEL_ID.to_owned(),
                model_snapshot_path: snapshot.to_string_lossy().into_owned(),
                modern_vlm_schema_ok: true,
                video_reasoner_schema_ok: true,
                image_smoke_ok: true,
                smoke_output_sha256: "a".repeat(64),
                smoke_reference_spec_sha256: "c".repeat(64),
            })
            .unwrap(),
        )
        .unwrap();

        assert!(reference_vlm_is_ready_at(&evidence, &ready));
        let mut changed = ready;
        changed.instance_id = "other-instance".to_owned();
        assert!(!reference_vlm_is_ready_at(&evidence, &changed));
    }

    #[test]
    #[ignore = "runs normal local Reference Analysis through the Ready ComfyUI VLM adapter"]
    fn live_ready_adapter_normalizes_reference_spec() {
        let ready = probe_ready_managed_profile(Duration::from_secs(150))
            .expect("Ready managed ComfyUI profile");
        let fixture = image_setup_smoke_fixture().unwrap();
        let reference = MediaReference {
            id: "live-reference-adapter-smoke".to_owned(),
            kind: MediaKind::Image,
            handle: format!("data:image/png;base64,{}", BASE64_STANDARD.encode(&fixture)),
        };
        let adapter = ComfyUiVlmReferenceAdapter::new(ready.clone());
        let normalized = adapter
            .analyze(&reference)
            .expect("normal Reference Analysis");
        assert_eq!(normalized.source_reference_id, reference.id);
        assert_eq!(normalized.analyzer_provider_id, COMFYUI_VLM_ADAPTER_ID);
        assert_eq!(
            normalized.analyzer_provider_instance_id.as_deref(),
            Some(ready.instance_id.as_str())
        );
        assert!(!normalized.summary.trim().is_empty());
        assert_eq!(normalized.provenance_sha256.len(), 64);
        eprintln!(
            "LIVE_GM5_ADAPTER instance={} summary_chars={}",
            ready.instance_id,
            normalized.summary.chars().count()
        );
    }
}
