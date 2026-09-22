use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{oneshot, Mutex},
};
use uuid::Uuid;

const DEFAULT_BIND: &str = "127.0.0.1:39817";
const INBOUND_PATH: &str = "/v1/linco/inbound";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;

type PendingResponse = oneshot::Sender<Result<Value, String>>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteLincoBridgeStatus {
    enabled: bool,
    listening: bool,
    bind_address: String,
    error: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct RemoteLincoBridgeState {
    pending: Arc<Mutex<HashMap<String, PendingResponse>>>,
    status: Arc<Mutex<Option<RemoteLincoBridgeStatus>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopInboundEvent {
    request_id: String,
    envelope: Value,
}

fn allowed_keys(event_type: &str) -> &'static [&'static str] {
    match event_type {
        "inbound_message" => &["type", "sessionKey", "messageId", "text", "mode", "profileId"],
        "recommendation_response" => &[
            "type", "sessionKey", "councilSessionId", "recommendationId", "approved", "decision",
        ],
        "permission_response" | "danger_confirm" => &[
            "type", "sessionKey", "confirmationId", "requestId", "taskId", "approved", "decision",
        ],
        "stop_turn" => &["type", "sessionKey"],
        _ => &[],
    }
}

fn required_string<'a>(object: &'a serde_json::Map<String, Value>, key: &str, max: usize) -> Result<&'a str, String> {
    let value = object.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("");
    if value.is_empty() || value.len() > max {
        return Err(format!("invalid {key}"));
    }
    Ok(value)
}

fn validate_envelope(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or_else(|| "JSON object required".to_string())?;
    let event_type = required_string(object, "type", 64)?;
    let keys = allowed_keys(event_type);
    if keys.is_empty() {
        return Err("unsupported remote event type".to_string());
    }
    if object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err("unsupported remote envelope field".to_string());
    }
    required_string(object, "sessionKey", 256)?;
    match event_type {
        "inbound_message" => {
            required_string(object, "messageId", 256)?;
            required_string(object, "text", 16 * 1024)?;
            if let Some(mode) = object.get("mode") {
                if !matches!(mode.as_str(), Some("auto" | "council" | "simulation")) {
                    return Err("invalid mode".to_string());
                }
            }
        }
        "recommendation_response" => {
            required_string(object, "councilSessionId", 256)?;
            required_string(object, "recommendationId", 256)?;
        }
        "permission_response" | "danger_confirm" => {
            if object.get("confirmationId").is_none() && object.get("requestId").is_none() {
                return Err("confirmation id required".to_string());
            }
            required_string(object, "taskId", 256)?;
        }
        "stop_turn" => {}
        _ => unreachable!(),
    }
    Ok(())
}

fn token_matches(received: &str, expected: &str) -> bool {
    let received = received.as_bytes();
    let expected = expected.as_bytes();
    if received.len() != expected.len() {
        return false;
    }
    received.iter().zip(expected).fold(0u8, |diff, (left, right)| diff | (left ^ right)) == 0
}

async fn write_response(stream: &mut TcpStream, status: &str, body: Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len(),
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&bytes).await
}

async fn read_request(stream: &mut TcpStream) -> Result<(String, String, HashMap<String, String>, Vec<u8>), String> {
    let mut buffer = Vec::new();
    let header_end = loop {
        if buffer.len() >= MAX_HEADER_BYTES {
            return Err("request headers too large".to_string());
        }
        let mut chunk = [0u8; 2048];
        let count = stream.read(&mut chunk).await.map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("incomplete request".to_string());
        }
        buffer.extend_from_slice(&chunk[..count]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header = std::str::from_utf8(&buffer[..header_end]).map_err(|_| "invalid request headers".to_string())?;
    let mut lines = header.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or_else(|| "invalid request header".to_string())?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>().map_err(|_| "invalid content length".to_string()))
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("request body too large".to_string());
    }
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0u8; (content_length - body.len()).min(4096)];
        let count = stream.read(&mut chunk).await.map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("incomplete request body".to_string());
        }
        body.extend_from_slice(&chunk[..count]);
    }
    body.truncate(content_length);
    Ok((method, path, headers, body))
}

async fn handle_connection(mut stream: TcpStream, app: AppHandle, state: RemoteLincoBridgeState, token: Arc<String>) {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(message) => {
            let _ = write_response(&mut stream, "400 Bad Request", json!({ "ok": false, "error": message })).await;
            return;
        }
    };
    let (method, path, headers, body) = request;
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !token_matches(supplied, token.as_str()) {
        let _ = write_response(&mut stream, "401 Unauthorized", json!({ "ok": false, "error": "unauthorized" })).await;
        return;
    }
    if method == "GET" && path == "/health" {
        let _ = write_response(&mut stream, "200 OK", json!({ "ok": true, "service": "ai-os-remote-linco" })).await;
        return;
    }
    if method != "POST" || path != INBOUND_PATH {
        let _ = write_response(&mut stream, "404 Not Found", json!({ "ok": false, "error": "not found" })).await;
        return;
    }
    if headers.get("content-type").map(|value| value.split(';').next().unwrap_or_default()) != Some("application/json") {
        let _ = write_response(&mut stream, "415 Unsupported Media Type", json!({ "ok": false, "error": "application/json required" })).await;
        return;
    }
    let envelope: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            let _ = write_response(&mut stream, "400 Bad Request", json!({ "ok": false, "error": "invalid JSON" })).await;
            return;
        }
    };
    if let Err(message) = validate_envelope(&envelope) {
        let _ = write_response(&mut stream, "400 Bad Request", json!({ "ok": false, "error": message })).await;
        return;
    }
    let request_id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    state.pending.lock().await.insert(request_id.clone(), sender);
    if app.emit("remote-linco://inbound", DesktopInboundEvent { request_id: request_id.clone(), envelope }).is_err() {
        state.pending.lock().await.remove(&request_id);
        let _ = write_response(&mut stream, "503 Service Unavailable", json!({ "ok": false, "error": "desktop event bridge unavailable" })).await;
        return;
    }
    match tokio::time::timeout(Duration::from_secs(120), receiver).await {
        Ok(Ok(Ok(events))) => {
            let _ = write_response(&mut stream, "200 OK", json!({ "ok": true, "events": events })).await;
        }
        Ok(Ok(Err(message))) => {
            let _ = write_response(&mut stream, "422 Unprocessable Entity", json!({ "ok": false, "error": message })).await;
        }
        _ => {
            state.pending.lock().await.remove(&request_id);
            let _ = write_response(&mut stream, "504 Gateway Timeout", json!({ "ok": false, "error": "desktop response timeout" })).await;
        }
    }
}

pub(crate) fn start(app: AppHandle) -> Result<(), String> {
    let state = app.state::<RemoteLincoBridgeState>().inner().clone();
    if std::env::var("AI_OS_REMOTE_LINCO_ENABLED").ok().as_deref() != Some("1") {
        tauri::async_runtime::block_on(async {
            *state.status.lock().await = Some(RemoteLincoBridgeStatus {
                enabled: false,
                listening: false,
                bind_address: DEFAULT_BIND.to_string(),
                error: None,
            });
        });
        return Ok(());
    }
    let token = std::env::var("AI_OS_REMOTE_LINCO_TOKEN").map_err(|_| "AI_OS_REMOTE_LINCO_TOKEN is required".to_string())?;
    if token.len() < 32 {
        return Err("AI_OS_REMOTE_LINCO_TOKEN must contain at least 32 characters".to_string());
    }
    let bind_address = std::env::var("AI_OS_REMOTE_LINCO_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    if !bind_address.starts_with("127.0.0.1:") {
        return Err("remote Linco bridge must bind to 127.0.0.1".to_string());
    }
    let task_state = state.clone();
    tauri::async_runtime::spawn(async move {
        match TcpListener::bind(&bind_address).await {
            Ok(listener) => {
                *task_state.status.lock().await = Some(RemoteLincoBridgeStatus {
                    enabled: true,
                    listening: true,
                    bind_address: bind_address.clone(),
                    error: None,
                });
                let token = Arc::new(token);
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(handle_connection(stream, app.clone(), task_state.clone(), token.clone()));
                }
            }
            Err(error) => {
                *task_state.status.lock().await = Some(RemoteLincoBridgeStatus {
                    enabled: true,
                    listening: false,
                    bind_address,
                    error: Some(error.to_string()),
                });
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_remote_linco_bridge_status(state: State<'_, RemoteLincoBridgeState>) -> Result<RemoteLincoBridgeStatus, String> {
    state.status.lock().await.clone().ok_or_else(|| "remote Linco bridge not initialized".to_string())
}

#[tauri::command]
pub(crate) async fn complete_remote_linco_request(
    state: State<'_, RemoteLincoBridgeState>,
    request_id: String,
    events: Option<Value>,
    error: Option<String>,
) -> Result<(), String> {
    let sender = state.pending.lock().await.remove(&request_id).ok_or_else(|| "remote request is not pending".to_string())?;
    let response = match (events, error) {
        (Some(events), None) if events.is_array() => Ok(events),
        (None, Some(error)) if !error.trim().is_empty() => Err(error.chars().take(300).collect()),
        _ => return Err("exactly one valid remote result is required".to_string()),
    };
    sender.send(response).map_err(|_| "remote requester disconnected".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_bounded_remote_envelopes() {
        assert!(validate_envelope(&json!({
            "type": "inbound_message", "sessionKey": "linco-1", "messageId": "m-1", "text": "hello"
        })).is_ok());
        assert!(validate_envelope(&json!({
            "type": "stop_turn", "sessionKey": "linco-1", "command": "openclaw"
        })).is_err());
        assert!(validate_envelope(&json!({
            "type": "permission_response", "sessionKey": "linco-1", "taskId": "task-1", "approved": true
        })).is_err());
    }

    #[test]
    fn bearer_comparison_rejects_wrong_values() {
        assert!(token_matches("01234567890123456789012345678901", "01234567890123456789012345678901"));
        assert!(!token_matches("wrong", "01234567890123456789012345678901"));
    }
}
