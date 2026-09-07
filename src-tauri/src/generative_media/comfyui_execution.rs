use reqwest::{
    blocking::Client,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

const EXECUTION_CONTRACT_VERSION: u32 = 1;
const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComfyUiExecutionOutput {
    pub prompt_id: String,
    pub filename: String,
    pub subfolder: String,
    pub output_type: String,
    pub mime_type: String,
    pub bytes_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComfyUiExecutionProbeResult {
    pub comfyui_version: String,
    pub cancellation_ok: bool,
    pub smoke_generation_ok: bool,
    pub output_retrieval_ok: bool,
    pub output: ComfyUiExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComfyUiExecutionValidationRecord {
    pub profile_id: String,
    pub profile_version: u32,
    pub checkpoint_sha256: String,
    pub execution_contract_version: u32,
    pub comfyui_version: String,
    pub cancellation_ok: bool,
    pub smoke_generation_ok: bool,
    pub output_retrieval_ok: bool,
    pub output_mime_type: String,
    pub output_bytes: u64,
    pub validated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComfyUiImageReference {
    filename: String,
    subfolder: String,
    output_type: String,
}

fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .user_agent("AI-OS/1.0 Generative-Media-Execution")
        .build()
        .map_err(|_| "ComfyUI execution HTTP client could not be created".to_owned())
}

fn endpoint_url(endpoint: &str, route: &str) -> Result<String, String> {
    let mut base = endpoint.trim().trim_end_matches('/').to_owned();

    if base.is_empty() {
        return Err("ComfyUI endpoint is empty".to_owned());
    }

    base.push('/');

    let base = url::Url::parse(&base).map_err(|_| "ComfyUI endpoint is invalid".to_owned())?;

    if !matches!(base.scheme(), "http" | "https") {
        return Err("ComfyUI endpoint must use HTTP or HTTPS".to_owned());
    }

    base.join(route.trim_start_matches('/'))
        .map(|url| url.to_string())
        .map_err(|_| "ComfyUI route could not be resolved".to_owned())
}

fn get_json(client: &Client, endpoint: &str, route: &str) -> Result<Value, String> {
    let response = client
        .get(endpoint_url(endpoint, route)?)
        .send()
        .map_err(|error| format!("ComfyUI GET {route} failed: {error}"))?;

    let status = response.status();

    let body = response
        .text()
        .map_err(|_| format!("ComfyUI GET {route} returned an unreadable body"))?;

    if !status.is_success() {
        return Err(format!(
            "ComfyUI GET {route} returned HTTP {}",
            status.as_u16()
        ));
    }

    serde_json::from_str(&body).map_err(|_| format!("ComfyUI GET {route} returned invalid JSON"))
}

fn current_comfyui_version(client: &Client, endpoint: &str) -> Result<String, String> {
    let stats = get_json(client, endpoint, "/system_stats")?;

    stats
        .get("system")
        .and_then(|system| system.get("comfyui_version"))
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "ComfyUI version is missing from /system_stats".to_owned())
}

fn generation_workflow(
    checkpoint: &str,
    prompt: &str,
    filename_prefix: &str,
    width: u32,
    height: u32,
    steps: u32,
) -> Value {
    json!({
        "1": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {
                "ckpt_name": checkpoint
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
                "width": width,
                "height": height,
                "batch_size": 1
            }
        },
        "5": {
            "class_type": "KSampler",
            "inputs": {
                "seed": 42,
                "steps": steps,
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
                "filename_prefix": filename_prefix,
                "images": ["6", 0]
            }
        }
    })
}

fn submit_prompt(client: &Client, endpoint: &str, workflow: Value) -> Result<String, String> {
    let client_id = format!("ai-os-{}", Uuid::new_v4());

    let response = client
        .post(endpoint_url(endpoint, "/prompt")?)
        .json(&json!({
            "prompt": workflow,
            "client_id": client_id
        }))
        .send()
        .map_err(|error| format!("ComfyUI /prompt submission failed: {error}"))?;

    let status = response.status();

    let body = response
        .text()
        .map_err(|_| "ComfyUI /prompt returned an unreadable body".to_owned())?;

    let value: Value = serde_json::from_str(&body)
        .map_err(|_| "ComfyUI /prompt returned invalid JSON".to_owned())?;

    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .or_else(|| value.get("error").and_then(Value::as_str))
            .unwrap_or("workflow was rejected");

        return Err(format!(
            "ComfyUI /prompt returned HTTP {}: {message}",
            status.as_u16()
        ));
    }

    if value
        .get("node_errors")
        .and_then(Value::as_object)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err("ComfyUI rejected one or more workflow nodes".to_owned());
    }

    value
        .get("prompt_id")
        .and_then(Value::as_str)
        .filter(|prompt_id| !prompt_id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "ComfyUI /prompt returned no prompt_id".to_owned())
}

fn history_entry(value: &Value, prompt_id: &str) -> Option<Value> {
    value
        .get(prompt_id)
        .cloned()
        .or_else(|| value.get("outputs").map(|_| value.clone()))
}

fn history_error(entry: &Value) -> Option<String> {
    let status = entry.get("status")?;

    let status_str = status
        .get("status_str")
        .and_then(Value::as_str)
        .unwrap_or("");

    if matches!(status_str, "error" | "failed") {
        return Some(format!("ComfyUI execution ended with status {status_str}"));
    }

    let messages = status.get("messages").and_then(Value::as_array)?;

    for message in messages {
        let Some(parts) = message.as_array() else {
            continue;
        };

        let Some(kind) = parts.first().and_then(Value::as_str) else {
            continue;
        };

        if matches!(
            kind,
            "execution_error" | "execution_interrupted" | "execution_cached_error"
        ) {
            return Some(format!("ComfyUI execution reported {kind}"));
        }
    }

    None
}

fn history_is_complete(entry: &Value) -> Result<bool, String> {
    if let Some(error) = history_error(entry) {
        return Err(error);
    }

    let Some(status) = entry.get("status") else {
        return Ok(false);
    };

    let completed = status
        .get("completed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if !completed {
        return Ok(false);
    }

    let status_str = status
        .get("status_str")
        .and_then(Value::as_str)
        .unwrap_or("");

    if status_str.is_empty() || status_str == "success" {
        Ok(true)
    } else {
        Err(format!(
            "ComfyUI execution completed with unexpected status {status_str}"
        ))
    }
}

fn wait_for_completed_history(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            return Err(format!(
                "ComfyUI prompt {prompt_id} did not complete before the execution timeout"
            ));
        }

        let history = get_json(client, endpoint, &format!("/history/{prompt_id}"))?;

        if let Some(entry) = history_entry(&history, prompt_id) {
            if history_is_complete(&entry)? {
                return Ok(entry);
            }
        }

        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn extract_image_reference(entry: &Value) -> Result<ComfyUiImageReference, String> {
    let outputs = entry
        .get("outputs")
        .and_then(Value::as_object)
        .ok_or_else(|| "ComfyUI completed history contains no outputs".to_owned())?;

    for output in outputs.values() {
        let Some(images) = output.get("images").and_then(Value::as_array) else {
            continue;
        };

        for image in images {
            let Some(filename) = image.get("filename").and_then(Value::as_str) else {
                continue;
            };

            if filename.is_empty() {
                continue;
            }

            let output_type = image
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("output");

            if !matches!(output_type, "output" | "temp") {
                continue;
            }

            return Ok(ComfyUiImageReference {
                filename: filename.to_owned(),
                subfolder: image
                    .get("subfolder")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                output_type: output_type.to_owned(),
            });
        }
    }

    Err("ComfyUI completed history contains no retrievable image".to_owned())
}

fn image_mime(bytes: &[u8], declared: Option<&str>) -> Result<String, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png".to_owned());
    }

    if bytes.len() >= 3 && bytes[..3] == [0xff, 0xd8, 0xff] {
        return Ok("image/jpeg".to_owned());
    }

    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok("image/webp".to_owned());
    }

    Err(format!(
        "ComfyUI /view returned bytes that are not a supported image{}",
        declared
            .filter(|value| !value.is_empty())
            .map(|value| format!(" ({value})"))
            .unwrap_or_default()
    ))
}

fn retrieve_output(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
    image: &ComfyUiImageReference,
) -> Result<ComfyUiExecutionOutput, String> {
    let mut view_url = url::Url::parse(&endpoint_url(endpoint, "/view")?)
        .map_err(|_| "ComfyUI /view URL is invalid".to_owned())?;

    view_url
        .query_pairs_mut()
        .append_pair("filename", &image.filename)
        .append_pair("subfolder", &image.subfolder)
        .append_pair("type", &image.output_type);

    let response = client
        .get(view_url)
        .send()
        .map_err(|error| format!("ComfyUI /view output retrieval failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "ComfyUI /view returned HTTP {}",
            response.status().as_u16()
        ));
    }

    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_OUTPUT_BYTES)
    {
        return Err("ComfyUI output exceeds the AI-OS validation bound".to_owned());
    }

    let declared_mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let bytes = response
        .bytes()
        .map_err(|_| "ComfyUI /view output body could not be read".to_owned())?;

    if bytes.is_empty() {
        return Err("ComfyUI /view returned an empty image".to_owned());
    }

    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("ComfyUI output exceeds the AI-OS validation bound".to_owned());
    }

    let mime_type = image_mime(&bytes, declared_mime.as_deref())?;

    Ok(ComfyUiExecutionOutput {
        prompt_id: prompt_id.to_owned(),
        filename: image.filename.clone(),
        subfolder: image.subfolder.clone(),
        output_type: image.output_type.clone(),
        mime_type,
        bytes_len: bytes.len() as u64,
    })
}

fn run_smoke_generation(
    client: &Client,
    endpoint: &str,
    checkpoint: &str,
    timeout: Duration,
) -> Result<ComfyUiExecutionOutput, String> {
    let prefix = format!("AI_OS_GM2C_SMOKE_{}", Uuid::new_v4().simple());

    let workflow = generation_workflow(
        checkpoint,
        "a simple red cube on a clean white background, studio lighting",
        &prefix,
        256,
        256,
        4,
    );

    let prompt_id = submit_prompt(client, endpoint, workflow)?;

    let history = wait_for_completed_history(client, endpoint, &prompt_id, timeout)?;

    let image = extract_image_reference(&history)?;

    retrieve_output(client, endpoint, &prompt_id, &image)
}

fn queue_contains_prompt(queue: &Value, field: &str, prompt_id: &str) -> bool {
    queue
        .get(field)
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.as_array()
                    .and_then(|values| values.get(1))
                    .and_then(Value::as_str)
                    == Some(prompt_id)
            })
        })
}

fn wait_until_prompt_running(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            return Err("ComfyUI cancellation probe never reached the running queue".to_owned());
        }

        let queue = get_json(client, endpoint, "/queue")?;

        if queue_contains_prompt(&queue, "queue_running", prompt_id) {
            return Ok(());
        }

        let history = get_json(client, endpoint, &format!("/history/{prompt_id}"))?;

        if let Some(entry) = history_entry(&history, prompt_id) {
            if history_is_complete(&entry)? {
                return Err(
                    "ComfyUI cancellation probe completed before it could be interrupted"
                        .to_owned(),
                );
            }
        }

        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn interrupt_running_prompt(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
) -> Result<(), String> {
    let queue = get_json(client, endpoint, "/queue")?;

    if queue_contains_prompt(&queue, "queue_running", prompt_id) {
        let response = client
            .post(endpoint_url(endpoint, "/interrupt")?)
            .send()
            .map_err(|error| format!("ComfyUI /interrupt failed: {error}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "ComfyUI /interrupt returned HTTP {}",
                response.status().as_u16()
            ));
        }

        return Ok(());
    }

    if queue_contains_prompt(&queue, "queue_pending", prompt_id) {
        let response = client
            .post(endpoint_url(endpoint, "/queue")?)
            .json(&json!({
                "delete": [prompt_id]
            }))
            .send()
            .map_err(|error| format!("ComfyUI queued-prompt cancellation failed: {error}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "ComfyUI queued-prompt cancellation returned HTTP {}",
                response.status().as_u16()
            ));
        }

        return Ok(());
    }

    Err("ComfyUI prompt was no longer cancellable".to_owned())
}

fn wait_until_prompt_leaves_queue(
    client: &Client,
    endpoint: &str,
    prompt_id: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            return Err("Interrupted ComfyUI prompt remained in the queue".to_owned());
        }

        let queue = get_json(client, endpoint, "/queue")?;

        let running = queue_contains_prompt(&queue, "queue_running", prompt_id);
        let pending = queue_contains_prompt(&queue, "queue_pending", prompt_id);

        if !running && !pending {
            return Ok(());
        }

        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn run_cancellation_probe(
    client: &Client,
    endpoint: &str,
    checkpoint: &str,
) -> Result<bool, String> {
    let prefix = format!("AI_OS_GM2C_CANCEL_{}", Uuid::new_v4().simple());

    // Deliberately long enough to guarantee a cancellation window after the
    // model has entered the running queue. The prompt is interrupted as soon
    // as running is observed.
    let workflow = generation_workflow(
        checkpoint,
        "AI-OS cancellation validation fixture",
        &prefix,
        64,
        64,
        1000,
    );

    let prompt_id = submit_prompt(client, endpoint, workflow)?;

    wait_until_prompt_running(client, endpoint, &prompt_id, Duration::from_secs(120))?;

    interrupt_running_prompt(client, endpoint, &prompt_id)?;

    wait_until_prompt_leaves_queue(client, endpoint, &prompt_id, Duration::from_secs(30))?;

    Ok(true)
}

pub(crate) fn run_execution_validation(
    endpoint: &str,
    checkpoint: &str,
) -> Result<ComfyUiExecutionProbeResult, String> {
    if checkpoint.trim().is_empty() {
        return Err("Managed ComfyUI checkpoint name is empty".to_owned());
    }

    let client = client()?;

    let comfyui_version = current_comfyui_version(&client, endpoint)?;

    let cancellation_ok = run_cancellation_probe(&client, endpoint, checkpoint)?;

    if !cancellation_ok {
        return Err("ComfyUI cancellation validation failed".to_owned());
    }

    let output = run_smoke_generation(&client, endpoint, checkpoint, Duration::from_secs(300))?;

    Ok(ComfyUiExecutionProbeResult {
        comfyui_version,
        cancellation_ok,
        smoke_generation_ok: true,
        output_retrieval_ok: output.bytes_len > 0,
        output,
    })
}

pub(crate) fn execution_validation_path(manifest_path: &Path) -> Result<PathBuf, String> {
    let parent = manifest_path
        .parent()
        .ok_or_else(|| "Managed profile directory is unavailable".to_owned())?;

    let stem = manifest_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Managed profile filename is invalid".to_owned())?;

    Ok(parent.join(format!("{stem}.execution.json")))
}

pub(crate) fn persist_execution_validation(
    manifest_path: &Path,
    profile_id: &str,
    profile_version: u32,
    checkpoint_sha256: &str,
    probe: &ComfyUiExecutionProbeResult,
) -> Result<PathBuf, String> {
    let path = execution_validation_path(manifest_path)?;

    let parent = path
        .parent()
        .ok_or_else(|| "Execution validation directory is unavailable".to_owned())?;

    fs::create_dir_all(parent)
        .map_err(|_| "Execution validation directory could not be created".to_owned())?;

    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("execution.json");

    let temporary = parent.join(format!(".{filename}.{}.tmp", Uuid::new_v4()));

    let record = ComfyUiExecutionValidationRecord {
        profile_id: profile_id.to_owned(),
        profile_version,
        checkpoint_sha256: checkpoint_sha256.to_owned(),
        execution_contract_version: EXECUTION_CONTRACT_VERSION,
        comfyui_version: probe.comfyui_version.clone(),
        cancellation_ok: probe.cancellation_ok,
        smoke_generation_ok: probe.smoke_generation_ok,
        output_retrieval_ok: probe.output_retrieval_ok,
        output_mime_type: probe.output.mime_type.clone(),
        output_bytes: probe.output.bytes_len,
        validated_at: chrono::Utc::now().to_rfc3339(),
    };

    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|_| "Execution validation record could not be encoded".to_owned())?;

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "Execution validation temporary file could not be created".to_owned())?;

    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);

        return Err(format!(
            "Execution validation temporary file could not be written: {error}"
        ));
    }

    drop(file);

    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);

        return Err(format!(
            "Execution validation record could not be installed atomically: {error}"
        ));
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut content_length = 0_usize;
        let mut header_end = None;

        loop {
            let read = stream.read(&mut buffer).unwrap();

            if read == 0 {
                break;
            }

            request.extend_from_slice(&buffer[..read]);

            if header_end.is_none() {
                if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    header_end = Some(position + 4);

                    let headers = String::from_utf8_lossy(&request[..position + 4]);

                    for line in headers.lines() {
                        if let Some(value) =
                            line.to_ascii_lowercase().strip_prefix("content-length:")
                        {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                }
            }

            if let Some(end) = header_end {
                if request.len() >= end + content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&request).to_string()
    }

    fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
        let headers = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: {content_type}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            body.len()
        );

        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn workflow_uses_only_the_managed_core_nodes() {
        let workflow = generation_workflow(
            "fixture.safetensors",
            "fixture prompt",
            "fixture",
            256,
            256,
            4,
        );

        let classes = workflow
            .as_object()
            .unwrap()
            .values()
            .filter_map(|node| node.get("class_type"))
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(
            classes,
            vec![
                "CheckpointLoaderSimple",
                "CLIPTextEncode",
                "CLIPTextEncode",
                "EmptyLatentImage",
                "KSampler",
                "VAEDecode",
                "SaveImage",
            ]
        );

        assert_eq!(workflow["1"]["inputs"]["ckpt_name"], "fixture.safetensors");

        assert_eq!(workflow["4"]["inputs"]["width"], 256);
        assert_eq!(workflow["5"]["inputs"]["steps"], 4);
    }

    #[test]
    fn completed_error_history_fails_closed() {
        let entry = json!({
            "status": {
                "status_str": "error",
                "completed": true,
                "messages": []
            },
            "outputs": {}
        });

        assert!(history_is_complete(&entry).is_err());
    }

    #[test]
    fn completed_history_without_image_fails_closed() {
        let entry = json!({
            "status": {
                "status_str": "success",
                "completed": true,
                "messages": []
            },
            "outputs": {
                "7": {}
            }
        });

        assert!(history_is_complete(&entry).unwrap());
        assert!(extract_image_reference(&entry).is_err());
    }

    #[test]
    fn real_http_shape_submits_waits_and_retrieves_image_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /prompt "));
            write_response(
                &mut stream,
                "application/json",
                br#"{"prompt_id":"prompt-1","number":1}"#,
            );

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /history/prompt-1 "));
            write_response(
                &mut stream,
                "application/json",
                br#"{
                  "prompt-1":{
                    "status":{
                      "status_str":"success",
                      "completed":true,
                      "messages":[]
                    },
                    "outputs":{
                      "7":{
                        "images":[
                          {
                            "filename":"fixture.png",
                            "subfolder":"",
                            "type":"output"
                          }
                        ]
                      }
                    }
                  }
                }"#,
            );

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /view?"));
            assert!(request.contains("filename=fixture.png"));
            let png = b"\x89PNG\r\n\x1a\nfixture";
            write_response(&mut stream, "image/png", png);
        });

        let endpoint = format!("http://{address}");
        let client = client().unwrap();

        let workflow = generation_workflow("fixture.safetensors", "fixture", "fixture", 64, 64, 1);

        let prompt_id = submit_prompt(&client, &endpoint, workflow).unwrap();

        let history =
            wait_for_completed_history(&client, &endpoint, &prompt_id, Duration::from_secs(3))
                .unwrap();

        let image = extract_image_reference(&history).unwrap();

        let output = retrieve_output(&client, &endpoint, &prompt_id, &image).unwrap();

        server.join().unwrap();

        assert_eq!(output.prompt_id, "prompt-1");
        assert_eq!(output.mime_type, "image/png");
        assert!(output.bytes_len > 8);
    }

    #[test]
    fn running_prompt_cancellation_uses_interrupt_and_waits_for_queue_exit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /prompt "));
            write_response(
                &mut stream,
                "application/json",
                br#"{"prompt_id":"cancel-1","number":1}"#,
            );

            let running = br#"{
              "queue_running":[
                [1,"cancel-1",{},{}]
              ],
              "queue_pending":[]
            }"#;

            // wait_until_prompt_running
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /queue "));
            write_response(&mut stream, "application/json", running);

            // interrupt_running_prompt first re-reads queue
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /queue "));
            write_response(&mut stream, "application/json", running);

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /interrupt "));
            write_response(&mut stream, "application/json", br#"{}"#);

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /queue "));
            write_response(
                &mut stream,
                "application/json",
                br#"{"queue_running":[],"queue_pending":[]}"#,
            );
        });

        let endpoint = format!("http://{address}");
        let client = client().unwrap();

        assert!(run_cancellation_probe(&client, &endpoint, "fixture.safetensors").unwrap());

        server.join().unwrap();
    }

    #[test]
    fn execution_validation_record_is_atomic_and_pinned() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("profile.json");
        fs::write(&manifest, b"{}").unwrap();

        let probe = ComfyUiExecutionProbeResult {
            comfyui_version: "0.34.5".to_owned(),
            cancellation_ok: true,
            smoke_generation_ok: true,
            output_retrieval_ok: true,
            output: ComfyUiExecutionOutput {
                prompt_id: "prompt-1".to_owned(),
                filename: "fixture.png".to_owned(),
                subfolder: String::new(),
                output_type: "output".to_owned(),
                mime_type: "image/png".to_owned(),
                bytes_len: 123,
            },
        };

        let path = persist_execution_validation(
            &manifest,
            "comfyui-checkpoint-t2i-v1",
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &probe,
        )
        .unwrap();

        let record: ComfyUiExecutionValidationRecord =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert_eq!(record.execution_contract_version, 1);
        assert_eq!(record.comfyui_version, "0.34.5");
        assert!(record.cancellation_ok);
        assert!(record.smoke_generation_ok);
        assert!(record.output_retrieval_ok);
        assert_eq!(record.output_mime_type, "image/png");
        assert_eq!(record.output_bytes, 123);

        let temporary_files = fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count();

        assert_eq!(temporary_files, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "runs a real managed ComfyUI cancellation + image generation"]
    fn live_managed_profile_execution_reaches_ready() {
        use crate::generative_media::{
            comfyui_macos::validate_managed_profile_execution, provider::LocalMediaReadiness,
        };

        let report = validate_managed_profile_execution(Duration::from_secs(150))
            .expect("real managed ComfyUI execution validation");

        assert!(report.cancellation_ok);
        assert!(report.smoke_generation_checked);
        assert!(report.evidence.smoke_generation_ok);
        assert!(report.output_retrieval_checked);
        assert!(report.evidence.output_retrieval_ok);
        assert!(report.output.bytes_len > 0);
        assert!(report.validation_path.is_file());
        assert_eq!(report.readiness(), LocalMediaReadiness::Ready);

        eprintln!(
            "LIVE_GM2C_EXECUTION instance={} cancelled={} prompt_id={} output_bytes={} mime={} smoke={} output={} state={:?}",
            report.instance_id,
            report.cancellation_ok,
            report.output.prompt_id,
            report.output.bytes_len,
            report.output.mime_type,
            report.evidence.smoke_generation_ok,
            report.evidence.output_retrieval_ok,
            report.readiness(),
        );
    }
}
