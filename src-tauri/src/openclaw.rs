use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use std::{
    fs,
    net::TcpStream,
    path::PathBuf,
    time::{Duration, Instant},
};

use tungstenite::{connect, stream::MaybeTlsStream, Error as WebSocketError, Message, WebSocket};

use url::Url;

use crate::runtime::{lifecycle, models::RuntimeLocation};

const CONFIG_FILE_NAME: &str = "openclaw-servers.json";

const EXPORT_SCHEMA_VERSION: u32 = 1;
const PROTOCOL_VERSION: u64 = 4;
const CONNECTION_TIMEOUT_SECONDS: u64 = 10;

type GatewaySocket = WebSocket<MaybeTlsStream<TcpStream>>;

/* ===========================
   Stored models
=========================== */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOpenClawServer {
    id: String,
    name: String,
    server_url: String,
    gateway_token: String,

    enabled: bool,
    active: bool,
    auto_connect: bool,

    #[serde(default = "default_connection_state")]
    connection_state: String,

    #[serde(default)]
    connection_message: String,

    #[serde(default)]
    version: Option<String>,

    #[serde(default)]
    gateway_id: Option<String>,

    #[serde(default)]
    latency_ms: Option<u64>,

    #[serde(default)]
    last_checked_at: Option<String>,

    #[serde(default)]
    created_at: Option<String>,

    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenClawRuntimeSnapshot {
    pub configured: bool,
    pub location: OpenClawRuntimeLocation,
    pub connection_state: String,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenClawRuntimeLocation {
    Local,
    Remote,
    Invalid,
}

fn default_connection_state() -> String {
    "unknown".to_string()
}

/* ===========================
   Public models
=========================== */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawServer {
    pub id: String,
    pub name: String,
    pub server_url: String,

    /*
     * 真实 Token 永远不返回前端。
     */
    pub gateway_token: String,
    pub has_gateway_token: bool,

    pub enabled: bool,
    pub active: bool,
    pub auto_connect: bool,

    pub connection_state: String,
    pub connection_message: String,

    pub version: Option<String>,
    pub gateway_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub last_checked_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawServerInput {
    pub name: String,
    pub server_url: String,
    pub gateway_token: String,
    pub enabled: bool,
    pub auto_connect: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawActionResult {
    pub success: bool,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<OpenClawServer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawConnectionResult {
    pub success: bool,
    pub state: String,
    pub message: String,
    pub checked_at: String,

    pub version: Option<String>,
    pub gateway_id: Option<String>,
    pub latency_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<OpenClawServer>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawRemoteStatus {
    pub connected: bool,

    pub server_id: String,
    pub server_name: String,
    pub server_url: String,

    pub gateway_status: Option<String>,
    pub version: Option<String>,
    pub gateway_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub checked_at: Option<String>,
    pub raw_response: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawDashboardSummary {
    pub configured: bool,
    pub connected: bool,

    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub server_url: Option<String>,

    pub state: String,
    pub message: String,

    pub version: Option<String>,
    pub gateway_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawRuntimeConfig {
    pub mode: String,
    pub active_server_id: Option<String>,
    pub active_server: Option<OpenClawServer>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenClawExportDocument {
    schema_version: u32,
    exported_at: String,
    includes_secrets: bool,
    servers: Vec<OpenClawServerInput>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawExportResult {
    pub success: bool,
    pub message: String,
    pub json: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawImportResult {
    pub success: bool,
    pub message: String,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawGatewayRequest {
    pub method: String,

    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawGatewayResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<Value>,
    pub raw_response: Option<String>,
}

/* ===========================
   Internal Gateway models
=========================== */

struct GatewaySession {
    socket: GatewaySocket,
    version: Option<String>,
    gateway_id: Option<String>,
    latency_ms: u64,
}

struct GatewayFailure {
    state: String,
    message: String,
    payload: Option<Value>,
    latency_ms: Option<u64>,
}

/* ===========================
   Result helpers
=========================== */

fn action_success(
    message: impl Into<String>,
    server: Option<OpenClawServer>,
) -> OpenClawActionResult {
    OpenClawActionResult {
        success: true,
        message: message.into(),
        server,
    }
}

fn action_failure(message: impl Into<String>) -> OpenClawActionResult {
    OpenClawActionResult {
        success: false,
        message: message.into(),
        server: None,
    }
}

fn connection_failure(
    state: impl Into<String>,
    message: impl Into<String>,
) -> OpenClawConnectionResult {
    OpenClawConnectionResult {
        success: false,
        state: state.into(),
        message: message.into(),
        checked_at: Utc::now().to_rfc3339(),

        version: None,
        gateway_id: None,
        latency_ms: None,
        server: None,
    }
}

/* ===========================
   Configuration storage
=========================== */

fn legacy_config_directory() -> Result<PathBuf, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "Unable to determine the home directory.".to_string())?;

    Ok(home.join(".ai-os").join("config"))
}

fn config_directory() -> Result<PathBuf, String> {
    let base = dirs::config_dir()
        .ok_or_else(|| "Unable to determine the system configuration directory.".to_string())?;

    Ok(base.join("AI OS"))
}

fn config_file() -> Result<PathBuf, String> {
    let directory = config_directory()?;

    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Unable to create configuration directory {}: {}",
            directory.display(),
            error
        )
    })?;

    let new_path = directory.join(CONFIG_FILE_NAME);

    // 首次运行时，将旧配置迁移到新目录
    if !new_path.exists() {
        let old_path = legacy_config_directory()?.join(CONFIG_FILE_NAME);

        if old_path.exists() {
            std::fs::copy(&old_path, &new_path).map_err(|error| {
                format!(
                    "Unable to migrate configuration from {} to {}: {}",
                    old_path.display(),
                    new_path.display(),
                    error
                )
            })?;
        }
    }

    Ok(new_path)
}

fn read_servers() -> Result<Vec<StoredOpenClawServer>, String> {
    let path = config_file()?;

    read_servers_at(&path)
}

fn read_servers_at(path: &PathBuf) -> Result<Vec<StoredOpenClawServer>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        format!(
            "Unable to read OpenClaw configuration {}: {}",
            path.display(),
            error,
        )
    })?;

    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&contents)
        .map_err(|error| format!("Unable to parse OpenClaw configuration: {}", error,))
}

pub(crate) fn runtime_snapshot() -> Result<OpenClawRuntimeSnapshot, String> {
    let current_path = config_directory()?.join(CONFIG_FILE_NAME);
    let legacy_path = legacy_config_directory()?.join(CONFIG_FILE_NAME);
    let servers = if current_path.exists() {
        read_servers_at(&current_path)?
    } else if legacy_path.exists() {
        read_servers_at(&legacy_path)?
    } else {
        Vec::new()
    };

    let Some(active) = servers
        .iter()
        .find(|server| server.active && server.enabled)
    else {
        return Ok(OpenClawRuntimeSnapshot {
            configured: false,
            location: OpenClawRuntimeLocation::Invalid,
            connection_state: "unknown".to_string(),
            last_checked_at: None,
        });
    };

    Ok(OpenClawRuntimeSnapshot {
        configured: true,
        location: classify_runtime_location(&active.server_url),
        connection_state: if active.connection_state.trim().is_empty() {
            "unknown".to_string()
        } else {
            active.connection_state.clone()
        },
        last_checked_at: active.last_checked_at.clone(),
    })
}

pub(crate) fn active_runtime_endpoint() -> Result<Option<String>, String> {
    let current_path = config_directory()?.join(CONFIG_FILE_NAME);
    let legacy_path = legacy_config_directory()?.join(CONFIG_FILE_NAME);
    let servers = if current_path.exists() {
        read_servers_at(&current_path)?
    } else if legacy_path.exists() {
        read_servers_at(&legacy_path)?
    } else {
        Vec::new()
    };

    Ok(servers
        .into_iter()
        .find(|server| server.active && server.enabled)
        .map(|server| server.server_url))
}

fn classify_runtime_location(server_url: &str) -> OpenClawRuntimeLocation {
    match lifecycle::classify_runtime_url(server_url) {
        Some(RuntimeLocation::Local) => OpenClawRuntimeLocation::Local,
        Some(RuntimeLocation::Remote) => OpenClawRuntimeLocation::Remote,
        _ => OpenClawRuntimeLocation::Invalid,
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn classifies_localhost_and_loopback_addresses_as_local() {
        for url in [
            "http://localhost:18789",
            "http://127.0.0.1:18789",
            "http://127.12.34.56:18789",
            "http://[::1]:18789",
        ] {
            assert_eq!(
                classify_runtime_location(url),
                OpenClawRuntimeLocation::Local
            );
        }
    }

    #[test]
    fn classifies_valid_non_loopback_hostname_as_remote() {
        assert_eq!(
            classify_runtime_location("https://gateway.example.com"),
            OpenClawRuntimeLocation::Remote
        );
    }

    #[test]
    fn classifies_invalid_or_hostless_url_as_invalid() {
        for url in ["not a url", "file:///tmp/openclaw.sock"] {
            assert_eq!(
                classify_runtime_location(url),
                OpenClawRuntimeLocation::Invalid
            );
        }
    }
}

fn write_servers(servers: &[StoredOpenClawServer]) -> Result<(), String> {
    let directory = config_directory()?;

    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Unable to create OpenClaw configuration directory: {}",
            error,
        )
    })?;

    let path = config_file()?;

    let contents = serde_json::to_string_pretty(servers)
        .map_err(|error| format!("Unable to serialize OpenClaw configuration: {}", error,))?;

    /*
     * 先写临时文件，再替换正式文件，
     * 避免程序中断导致配置损坏。
     */
    let temporary_path = path.with_extension("json.tmp");

    fs::write(&temporary_path, contents)
        .map_err(|error| format!("Unable to write OpenClaw configuration: {}", error,))?;

    fs::rename(&temporary_path, &path).map_err(|error| {
        format!(
            "Unable to save OpenClaw configuration {}: {}",
            path.display(),
            error,
        )
    })
}

fn public_server(stored: &StoredOpenClawServer) -> OpenClawServer {
    OpenClawServer {
        id: stored.id.clone(),
        name: stored.name.clone(),
        server_url: stored.server_url.clone(),

        gateway_token: if stored.gateway_token.trim().is_empty() {
            String::new()
        } else {
            "••••••••••••••••".to_string()
        },

        has_gateway_token: !stored.gateway_token.trim().is_empty(),

        enabled: stored.enabled,
        active: stored.active,
        auto_connect: stored.auto_connect,

        connection_state: if stored.connection_state.trim().is_empty() {
            "unknown".to_string()
        } else {
            stored.connection_state.clone()
        },

        connection_message: stored.connection_message.clone(),

        version: stored.version.clone(),
        gateway_id: stored.gateway_id.clone(),
        latency_ms: stored.latency_ms,
        last_checked_at: stored.last_checked_at.clone(),
        created_at: stored.created_at.clone(),
        updated_at: stored.updated_at.clone(),
    }
}

/* ===========================
   Validation and IDs
=========================== */

fn normalize_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        return Err("Server URL is required.".to_string());
    }

    let parsed = Url::parse(trimmed).map_err(|error| format!("Invalid Server URL: {}", error,))?;

    match parsed.scheme() {
        "http" | "https" | "ws" | "wss" => {}
        _ => {
            return Err("Server URL must use HTTP, HTTPS, WS or WSS.".to_string());
        }
    }

    if parsed.host_str().is_none() {
        return Err("Server URL must contain a host.".to_string());
    }

    Ok(trimmed.to_string())
}

fn websocket_url(server_url: &str) -> Result<String, String> {
    let normalized = normalize_url(server_url)?;

    let mut parsed = Url::parse(&normalized).map_err(|error| error.to_string())?;

    match parsed.scheme() {
        "http" => {
            parsed
                .set_scheme("ws")
                .map_err(|_| "Unable to convert HTTP URL to WebSocket URL.".to_string())?;
        }

        "https" => {
            parsed
                .set_scheme("wss")
                .map_err(|_| "Unable to convert HTTPS URL to WebSocket URL.".to_string())?;
        }

        "ws" | "wss" => {}

        _ => {
            return Err("Unsupported OpenClaw URL scheme.".to_string());
        }
    }

    if parsed.path().is_empty() {
        parsed.set_path("/");
    }

    Ok(parsed.to_string())
}

fn validate_input(input: &OpenClawServerInput, require_token: bool) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("Server name is required.".to_string());
    }

    normalize_url(&input.server_url)?;

    if require_token && input.gateway_token.trim().is_empty() {
        return Err("Gateway Token is required.".to_string());
    }

    Ok(())
}

fn generate_id(name: &str) -> String {
    let slug = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    format!(
        "{}-{}",
        if slug.is_empty() { "openclaw" } else { &slug },
        Utc::now().timestamp_millis(),
    )
}

fn unique_server_name(servers: &[StoredOpenClawServer], requested_name: &str) -> String {
    let base = requested_name.trim();

    if !servers
        .iter()
        .any(|server| server.name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }

    for number in 2..10_000 {
        let candidate = format!("{} {}", base, number);

        if !servers
            .iter()
            .any(|server| server.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }

    format!("{} {}", base, Utc::now().timestamp_millis(),)
}

/* ===========================
   WebSocket helpers
=========================== */

fn configure_socket_timeout(socket: &mut GatewaySocket) {
    let duration = Some(Duration::from_secs(CONNECTION_TIMEOUT_SECONDS));

    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(duration);

            let _ = stream.set_write_timeout(duration);
        }

        MaybeTlsStream::NativeTls(stream) => {
            let tcp = stream.get_mut();

            let _ = tcp.set_read_timeout(duration);

            let _ = tcp.set_write_timeout(duration);
        }

        _ => {}
    }
}

fn websocket_error(error: WebSocketError) -> String {
    match error {
        WebSocketError::Io(io_error) => {
            format!("Unable to reach OpenClaw Gateway: {}", io_error,)
        }

        WebSocketError::Http(response) => {
            format!(
                "OpenClaw rejected the WebSocket request with HTTP status {}.",
                response.status(),
            )
        }

        other => {
            format!("OpenClaw WebSocket error: {}", other,)
        }
    }
}

fn read_json_message(socket: &mut GatewaySocket) -> Result<Value, String> {
    loop {
        let message = socket.read().map_err(websocket_error)?;

        match message {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref())
                    .map_err(|error| format!("OpenClaw returned invalid JSON: {}", error,));
            }

            Message::Binary(bytes) => {
                return serde_json::from_slice(&bytes)
                    .map_err(|error| format!("OpenClaw returned invalid JSON: {}", error,));
            }

            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .map_err(websocket_error)?;
            }

            Message::Close(frame) => {
                return Err(format!("OpenClaw closed the connection: {:?}", frame,));
            }

            _ => {}
        }
    }
}

fn classify_gateway_error(message: &str, code: Option<&str>) -> String {
    let combined = format!("{} {}", message, code.unwrap_or_default(),).to_lowercase();

    if combined.contains("unauthorized")
        || combined.contains("invalid token")
        || combined.contains("token missing")
        || (combined.contains("auth") && combined.contains("token"))
    {
        return "unauthorized".to_string();
    }

    if combined.contains("pairing")
        || combined.contains("pairing_required")
        || combined.contains("device approval")
    {
        return "pairing-required".to_string();
    }

    if combined.contains("connection refused")
        || combined.contains("timed out")
        || combined.contains("network")
        || combined.contains("dns")
    {
        return "unreachable".to_string();
    }

    "error".to_string()
}

fn extract_error(response: &Value) -> (String, Option<String>) {
    let error_value = response.get("error");

    let message = error_value
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("OpenClaw rejected the request.")
        .to_string();

    let code = error_value
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            error_value
                .and_then(|value| value.get("details"))
                .and_then(|details| details.get("code"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    (message, code)
}

fn find_string_recursive(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(Value::as_str) {
                return Some(found.to_string());
            }

            for child in map.values() {
                if let Some(found) = find_string_recursive(child, key) {
                    return Some(found);
                }
            }

            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = find_string_recursive(item, key) {
                    return Some(found);
                }
            }

            None
        }
        _ => None,
    }
}

fn extract_gateway_metadata(payload: Option<&Value>) -> (Option<String>, Option<String>) {
    let version = payload.and_then(|value| {
        value
            .get("server")
            .and_then(|server| server.get("version"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| find_string_recursive(value, "version"))
    });

    let gateway_id = payload.and_then(|value| {
        find_string_recursive(value, "instanceId")
            .or_else(|| find_string_recursive(value, "instance_id"))
            .or_else(|| find_string_recursive(value, "gatewayId"))
            .or_else(|| find_string_recursive(value, "gateway_id"))
    });

    (version, gateway_id)
}

/* ===========================
   Gateway connection
=========================== */

fn open_gateway_session(
    server_url: &str,
    gateway_token: &str,
) -> Result<GatewaySession, GatewayFailure> {
    let started_at = Instant::now();

    let ws_url = websocket_url(server_url).map_err(|message| GatewayFailure {
        state: "error".to_string(),
        message,
        payload: None,
        latency_ms: None,
    })?;

    let (mut socket, _response) = connect(ws_url.as_str()).map_err(|error| GatewayFailure {
        state: "unreachable".to_string(),
        message: websocket_error(error),
        payload: None,
        latency_ms: Some(started_at.elapsed().as_millis() as u64),
    })?;

    configure_socket_timeout(&mut socket);

    let challenge = read_json_message(&mut socket).map_err(|message| GatewayFailure {
        state: "unreachable".to_string(),
        message,
        payload: None,
        latency_ms: Some(started_at.elapsed().as_millis() as u64),
    })?;

    let challenge_event = challenge.get("event").and_then(Value::as_str);

    if challenge_event != Some("connect.challenge") {
        let _ = socket.close(None);

        return Err(GatewayFailure {
            state: "error".to_string(),

            message: "The server responded, but it did not provide an OpenClaw connect challenge."
                .to_string(),

            payload: Some(challenge),

            latency_ms: Some(started_at.elapsed().as_millis() as u64),
        });
    }

    let request_id = format!("ai-os-connect-{}", Utc::now().timestamp_millis(),);

    /*
     * 已根据本机 OpenClaw 2026.6.11
     * 验证通过的身份格式：
     *
     * client.id   = cli
     * client.mode = cli
     * role        = operator
     */
    let connect_request = json!({
        "type": "req",
        "id": request_id,
        "method": "connect",

        "params": {
            "minProtocol": PROTOCOL_VERSION,
            "maxProtocol": PROTOCOL_VERSION,

            "client": {
                "id": "cli",

                "version": env!(
                    "CARGO_PKG_VERSION"
                ),

                "platform":
                    std::env::consts::OS,

                "mode": "cli"
            },

            "role": "operator",

            "scopes": [
                "operator.read",
                "operator.write"
            ],

            "caps": [],
            "commands": [],
            "permissions": {},

            "auth": {
                "token": gateway_token
            },

            "locale": "en-US",

            "userAgent": format!(
                "openclaw-cli/{}",
                env!(
                    "CARGO_PKG_VERSION"
                )
            )
        }
    });

    socket
        .send(Message::Text(connect_request.to_string().into()))
        .map_err(|error| GatewayFailure {
            state: "unreachable".to_string(),
            message: websocket_error(error),
            payload: None,
            latency_ms: Some(started_at.elapsed().as_millis() as u64),
        })?;

    let response = read_json_message(&mut socket).map_err(|message| GatewayFailure {
        state: "unreachable".to_string(),
        message,
        payload: None,
        latency_ms: Some(started_at.elapsed().as_millis() as u64),
    })?;

    let latency_ms = started_at.elapsed().as_millis() as u64;

    let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);

    if !ok {
        let _ = socket.close(None);

        let (message, code) = extract_error(&response);

        let state = classify_gateway_error(&message, code.as_deref());

        return Err(GatewayFailure {
            state,
            message,
            payload: Some(response),
            latency_ms: Some(latency_ms),
        });
    }

    let payload = response.get("payload").cloned();

    let (version, gateway_id) = extract_gateway_metadata(payload.as_ref());

    Ok(GatewaySession {
        socket,
        version,
        gateway_id,
        latency_ms,
    })
}
/* ===========================
   Gateway requests
=========================== */

fn invoke_gateway_method(
    session: &mut GatewaySession,
    method: &str,
    params: Option<Value>,
) -> Result<Value, GatewayFailure> {
    let method = method.trim();

    if method.is_empty() {
        return Err(GatewayFailure {
            state: "error".to_string(),

            message: "Gateway method is required.".to_string(),

            payload: None,
            latency_ms: None,
        });
    }

    let request_id = format!("ai-os-request-{}", Utc::now().timestamp_millis(),);

    let request = json!({
        "type": "req",
        "id": request_id,
        "method": method,
        "params": params.unwrap_or_else(
            || json!({})
        )
    });

    session
        .socket
        .send(Message::Text(request.to_string().into()))
        .map_err(|error| GatewayFailure {
            state: "unreachable".to_string(),

            message: websocket_error(error),

            payload: None,

            latency_ms: Some(session.latency_ms),
        })?;

    loop {
        let response =
            read_json_message(&mut session.socket).map_err(|message| GatewayFailure {
                state: "unreachable".to_string(),

                message,

                payload: None,

                latency_ms: Some(session.latency_ms),
            })?;

        /*
         * OpenClaw 可能在请求响应之间发送事件。
         * 这里只处理与当前 request id 对应的响应。
         */
        let response_id = response.get("id").and_then(Value::as_str);

        if response_id != Some(request_id.as_str()) {
            continue;
        }

        let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);

        if ok {
            return Ok(response.get("payload").cloned().unwrap_or(Value::Null));
        }

        let (message, code) = extract_error(&response);

        return Err(GatewayFailure {
            state: classify_gateway_error(&message, code.as_deref()),

            message,

            payload: Some(response),

            latency_ms: Some(session.latency_ms),
        });
    }
}

fn test_server_connection(server_url: &str, gateway_token: &str) -> OpenClawConnectionResult {
    let checked_at = Utc::now().to_rfc3339();

    match open_gateway_session(server_url, gateway_token) {
        Ok(mut session) => {
            let _ = session.socket.close(None);

            let message = match (session.version.as_ref(), session.latency_ms) {
                (Some(version), latency) => {
                    format!(
                        "Connected to OpenClaw Gateway {} in {} ms.",
                        version, latency,
                    )
                }

                (None, latency) => {
                    format!("Connected to OpenClaw Gateway in {} ms.", latency,)
                }
            };

            OpenClawConnectionResult {
                success: true,

                state: "connected".to_string(),

                message,

                checked_at,

                version: session.version,

                gateway_id: session.gateway_id,

                latency_ms: Some(session.latency_ms),

                server: None,
            }
        }

        Err(failure) => OpenClawConnectionResult {
            success: false,

            state: failure.state,

            message: failure.message,

            checked_at,

            version: None,

            gateway_id: None,

            latency_ms: failure.latency_ms,

            server: None,
        },
    }
}

fn apply_connection_result(stored: &mut StoredOpenClawServer, result: &OpenClawConnectionResult) {
    stored.connection_state = result.state.clone();

    stored.connection_message = result.message.clone();

    stored.version = result.version.clone();

    stored.gateway_id = result.gateway_id.clone();

    stored.latency_ms = result.latency_ms;

    stored.last_checked_at = Some(result.checked_at.clone());

    stored.updated_at = Some(Utc::now().to_rfc3339());
}

fn ensure_single_active_server(servers: &mut [StoredOpenClawServer]) {
    let active_enabled_indexes = servers
        .iter()
        .enumerate()
        .filter_map(|(index, server)| {
            if server.active && server.enabled {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if let Some(first_active) = active_enabled_indexes.first().copied() {
        for (index, server) in servers.iter_mut().enumerate() {
            server.active = index == first_active;
        }

        return;
    }

    if let Some(first_enabled) = servers.iter().position(|server| server.enabled) {
        for (index, server) in servers.iter_mut().enumerate() {
            server.active = index == first_enabled;
        }
    } else {
        for server in servers.iter_mut() {
            server.active = false;
        }
    }
}

fn raw_json(value: Option<&Value>) -> Option<String> {
    value.and_then(|payload| serde_json::to_string_pretty(payload).ok())
}

/* ===========================
   Server management commands
=========================== */

#[tauri::command]
pub fn list_openclaw_servers() -> Result<Vec<OpenClawServer>, String> {
    Ok(read_servers()?.iter().map(public_server).collect())
}

#[tauri::command]
pub fn save_openclaw_server(server: OpenClawServerInput) -> OpenClawActionResult {
    if let Err(error) = validate_input(&server, true) {
        return action_failure(error);
    }

    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let normalized_url = match normalize_url(&server.server_url) {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let now = Utc::now().to_rfc3339();

    let stored = StoredOpenClawServer {
        id: generate_id(&server.name),

        name: unique_server_name(&servers, &server.name),

        server_url: normalized_url,

        gateway_token: server.gateway_token.trim().to_string(),

        enabled: server.enabled,

        active: false,

        auto_connect: server.auto_connect,

        connection_state: "unknown".to_string(),

        connection_message: "Connection has not been tested.".to_string(),

        version: None,

        gateway_id: None,

        latency_ms: None,

        last_checked_at: None,

        created_at: Some(now.clone()),

        updated_at: Some(now),
    };

    servers.push(stored);

    ensure_single_active_server(&mut servers);

    let created = match servers.last() {
        Some(value) => value.clone(),

        None => {
            return action_failure("Unable to create OpenClaw server.");
        }
    };

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!("OpenClaw server {} was added.", created.name,),
        Some(public_server(&created)),
    )
}

#[tauri::command]
pub fn update_openclaw_server(id: String, server: OpenClawServerInput) -> OpenClawActionResult {
    if let Err(error) = validate_input(&server, false) {
        return action_failure(error);
    }

    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let Some(index) = servers.iter().position(|item| item.id == id) else {
        return action_failure("OpenClaw server was not found.");
    };

    let normalized_url = match normalize_url(&server.server_url) {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let old_token = servers[index].gateway_token.clone();

    let old_url = servers[index].server_url.clone();

    let token_changed = !server.gateway_token.trim().is_empty();

    servers[index].name = server.name.trim().to_string();

    servers[index].server_url = normalized_url;

    servers[index].gateway_token = if token_changed {
        server.gateway_token.trim().to_string()
    } else {
        old_token
    };

    servers[index].enabled = server.enabled;

    servers[index].auto_connect = server.auto_connect;

    servers[index].updated_at = Some(Utc::now().to_rfc3339());

    let connection_changed = old_url != servers[index].server_url || token_changed;

    if connection_changed {
        servers[index].connection_state = "unknown".to_string();

        servers[index].connection_message =
            "Connection settings changed. Test the server again.".to_string();

        servers[index].version = None;

        servers[index].gateway_id = None;

        servers[index].latency_ms = None;

        servers[index].last_checked_at = None;
    }

    ensure_single_active_server(&mut servers);

    let updated = servers[index].clone();

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!("OpenClaw server {} was updated.", updated.name,),
        Some(public_server(&updated)),
    )
}

#[tauri::command]
pub fn delete_openclaw_server(id: String) -> OpenClawActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let Some(index) = servers.iter().position(|server| server.id == id) else {
        return action_failure("OpenClaw server was not found.");
    };

    let removed = servers.remove(index);

    ensure_single_active_server(&mut servers);

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!("OpenClaw server {} was deleted.", removed.name,),
        None,
    )
}

#[tauri::command]
pub fn duplicate_openclaw_server(id: String) -> OpenClawActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let Some(source) = servers.iter().find(|server| server.id == id).cloned() else {
        return action_failure("OpenClaw server was not found.");
    };

    let now = Utc::now().to_rfc3339();

    let duplicate_name = unique_server_name(&servers, &format!("{} Copy", source.name,));

    let duplicate = StoredOpenClawServer {
        id: generate_id(&duplicate_name),

        name: duplicate_name,

        server_url: source.server_url,

        gateway_token: source.gateway_token,

        enabled: source.enabled,

        active: false,

        auto_connect: source.auto_connect,

        connection_state: "unknown".to_string(),

        connection_message: "Duplicated server has not been tested.".to_string(),

        version: None,

        gateway_id: None,

        latency_ms: None,

        last_checked_at: None,

        created_at: Some(now.clone()),

        updated_at: Some(now),
    };

    servers.push(duplicate.clone());

    ensure_single_active_server(&mut servers);

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!("OpenClaw server {} was duplicated.", duplicate.name,),
        Some(public_server(&duplicate)),
    )
}

#[tauri::command]
pub fn toggle_openclaw_server(id: String, enabled: bool) -> OpenClawActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let Some(index) = servers.iter().position(|server| server.id == id) else {
        return action_failure("OpenClaw server was not found.");
    };

    servers[index].enabled = enabled;

    servers[index].updated_at = Some(Utc::now().to_rfc3339());

    if !enabled {
        servers[index].active = false;
    }

    ensure_single_active_server(&mut servers);

    let updated = servers[index].clone();

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!(
            "{} was {}.",
            updated.name,
            if enabled { "enabled" } else { "disabled" },
        ),
        Some(public_server(&updated)),
    )
}

#[tauri::command]
pub fn set_active_openclaw_server(id: String) -> OpenClawActionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return action_failure(error);
        }
    };

    let Some(target_index) = servers.iter().position(|server| server.id == id) else {
        return action_failure("OpenClaw server was not found.");
    };

    if !servers[target_index].enabled {
        return action_failure("A disabled server cannot be made active.");
    }

    for (index, server) in servers.iter_mut().enumerate() {
        server.active = index == target_index;
    }

    servers[target_index].updated_at = Some(Utc::now().to_rfc3339());

    let updated = servers[target_index].clone();

    if let Err(error) = write_servers(&servers) {
        return action_failure(error);
    }

    action_success(
        format!("{} is now the active OpenClaw server.", updated.name,),
        Some(public_server(&updated)),
    )
}

/* ===========================
   Connection commands
=========================== */

#[tauri::command]
pub fn test_openclaw_connection(id: String) -> OpenClawConnectionResult {
    let mut servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return connection_failure("error", error);
        }
    };

    let Some(index) = servers.iter().position(|server| server.id == id) else {
        return connection_failure("error", "OpenClaw server was not found.");
    };

    let mut result =
        test_server_connection(&servers[index].server_url, &servers[index].gateway_token);

    apply_connection_result(&mut servers[index], &result);

    result.server = Some(public_server(&servers[index]));

    if let Err(error) = write_servers(&servers) {
        return connection_failure("error", error);
    }

    result
}

#[tauri::command]
pub fn test_openclaw_connection_input(server: OpenClawServerInput) -> OpenClawConnectionResult {
    if let Err(error) = validate_input(&server, true) {
        return connection_failure("error", error);
    }

    test_server_connection(&server.server_url, &server.gateway_token)
}

#[tauri::command]
pub fn test_all_openclaw_servers() -> Result<Vec<OpenClawConnectionResult>, String> {
    let mut servers = read_servers()?;
    let mut results = Vec::new();

    // 保存当前最佳候选：
    // (服务器在列表中的下标, 延迟毫秒数)
    let mut best_candidate: Option<(usize, u64)> = None;

    for (index, server) in servers.iter_mut().enumerate() {
        if !server.enabled {
            continue;
        }

        let mut result = test_server_connection(&server.server_url, &server.gateway_token);

        apply_connection_result(server, &result);

        // 只有同时满足以下条件的服务器才参与自动选择：
        // 1. 已启用
        // 2. Auto Connect 已开启
        // 3. 连接测试成功
        // 4. 能取得有效延迟
        if server.auto_connect && result.success {
            if let Some(latency_ms) = result.latency_ms {
                let should_replace = best_candidate
                    .map(|(_, best_latency)| latency_ms < best_latency)
                    .unwrap_or(true);

                if should_replace {
                    best_candidate = Some((index, latency_ms));
                }
            }
        }

        result.server = Some(public_server(server));

        results.push(result);
    }

    // 找到候选服务器后，
    // 把延迟最低的服务器设为唯一 Active。
    // 如果没有任何可用候选，则保留原有 Active 状态。
    if let Some((best_index, _)) = best_candidate {
        for (index, server) in servers.iter_mut().enumerate() {
            server.active = index == best_index;
        }

        servers[best_index].updated_at = Some(Utc::now().to_rfc3339());
    }

    write_servers(&servers)?;

    Ok(results)
}

#[tauri::command]
pub fn get_active_openclaw_status() -> Result<OpenClawRemoteStatus, String> {
    let mut servers = read_servers()?;

    let index = servers
        .iter()
        .position(|server| server.active && server.enabled)
        .ok_or_else(|| "No active OpenClaw server is configured.".to_string())?;

    let result = test_server_connection(&servers[index].server_url, &servers[index].gateway_token);

    apply_connection_result(&mut servers[index], &result);

    let server = servers[index].clone();

    write_servers(&servers)?;

    Ok(OpenClawRemoteStatus {
        connected: result.success,

        server_id: server.id,

        server_name: server.name,

        server_url: server.server_url,

        gateway_status: Some(result.state),

        version: result.version,

        gateway_id: result.gateway_id,

        latency_ms: result.latency_ms,

        checked_at: Some(result.checked_at),

        raw_response: None,
    })
}

#[tauri::command]
pub fn get_openclaw_dashboard_summary() -> Result<OpenClawDashboardSummary, String> {
    let servers = read_servers()?;

    let Some(active) = servers
        .iter()
        .find(|server| server.active && server.enabled)
    else {
        return Ok(OpenClawDashboardSummary {
            configured: false,

            connected: false,

            server_id: None,

            server_name: None,

            server_url: None,

            state: "unknown".to_string(),

            message: "No active OpenClaw server is configured.".to_string(),

            version: None,

            gateway_id: None,

            latency_ms: None,

            last_checked_at: None,
        });
    };

    Ok(OpenClawDashboardSummary {
        configured: true,

        connected: active.connection_state == "connected",

        server_id: Some(active.id.clone()),

        server_name: Some(active.name.clone()),

        server_url: Some(active.server_url.clone()),

        state: active.connection_state.clone(),

        message: active.connection_message.clone(),

        version: active.version.clone(),

        gateway_id: active.gateway_id.clone(),

        latency_ms: active.latency_ms,

        last_checked_at: active.last_checked_at.clone(),
    })
}

#[tauri::command]
pub fn get_openclaw_runtime_config() -> Result<OpenClawRuntimeConfig, String> {
    let servers = read_servers()?;

    let active = servers
        .iter()
        .find(|server| server.active && server.enabled);

    Ok(OpenClawRuntimeConfig {
        mode: match active {
            Some(server) => {
                let parsed = Url::parse(&server.server_url).ok();

                let is_local = parsed
                    .as_ref()
                    .and_then(Url::host_str)
                    .map(|host| host == "127.0.0.1" || host == "localhost" || host == "::1")
                    .unwrap_or(false);

                if is_local {
                    "local".to_string()
                } else {
                    "remote".to_string()
                }
            }

            None => "local".to_string(),
        },

        active_server_id: active.map(|server| server.id.clone()),

        active_server: active.map(public_server),
    })
}

/* ===========================
   Import / Export
=========================== */

#[tauri::command]
pub fn export_openclaw_servers(include_secrets: bool) -> OpenClawExportResult {
    let servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return OpenClawExportResult {
                success: false,

                message: error,

                json: None,
            };
        }
    };

    let document = OpenClawExportDocument {
        schema_version: EXPORT_SCHEMA_VERSION,

        exported_at: Utc::now().to_rfc3339(),

        includes_secrets: include_secrets,

        servers: servers
            .iter()
            .map(|server| OpenClawServerInput {
                name: server.name.clone(),

                server_url: server.server_url.clone(),

                gateway_token: if include_secrets {
                    server.gateway_token.clone()
                } else {
                    String::new()
                },

                enabled: server.enabled,

                auto_connect: server.auto_connect,
            })
            .collect(),
    };

    match serde_json::to_string_pretty(&document) {
        Ok(json) => OpenClawExportResult {
            success: true,

            message: format!("Exported {} OpenClaw server(s).", servers.len(),),

            json: Some(json),
        },

        Err(error) => OpenClawExportResult {
            success: false,

            message: format!("Unable to export OpenClaw servers: {}", error,),

            json: None,
        },
    }
}

#[tauri::command]
pub fn import_openclaw_servers(json: String, replace_existing: bool) -> OpenClawImportResult {
    let document = match serde_json::from_str::<OpenClawExportDocument>(&json) {
        Ok(value) => value,

        Err(error) => {
            return OpenClawImportResult {
                success: false,

                message: format!("Unable to parse OpenClaw import file: {}", error,),

                imported_count: 0,

                skipped_count: 0,
            };
        }
    };

    if document.schema_version != EXPORT_SCHEMA_VERSION {
        return OpenClawImportResult {
            success: false,

            message: format!(
                "Unsupported OpenClaw export schema version: {}",
                document.schema_version,
            ),

            imported_count: 0,

            skipped_count: document.servers.len(),
        };
    }

    let mut servers = if replace_existing {
        Vec::new()
    } else {
        match read_servers() {
            Ok(value) => value,

            Err(error) => {
                return OpenClawImportResult {
                    success: false,

                    message: error,

                    imported_count: 0,

                    skipped_count: 0,
                };
            }
        }
    };

    let mut imported_count = 0usize;

    let mut skipped_count = 0usize;

    for input in document.servers {
        if validate_input(&input, false).is_err() {
            skipped_count += 1;

            continue;
        }

        let normalized_url = match normalize_url(&input.server_url) {
            Ok(value) => value,

            Err(_) => {
                skipped_count += 1;

                continue;
            }
        };

        let duplicate = servers
            .iter()
            .any(|server| server.server_url.eq_ignore_ascii_case(&normalized_url));

        if duplicate {
            skipped_count += 1;

            continue;
        }

        let now = Utc::now().to_rfc3339();

        let name = unique_server_name(&servers, &input.name);

        servers.push(StoredOpenClawServer {
            id: generate_id(&name),

            name,

            server_url: normalized_url,

            gateway_token: input.gateway_token.trim().to_string(),

            enabled: input.enabled,

            active: false,

            auto_connect: input.auto_connect,

            connection_state: "unknown".to_string(),

            connection_message: if input.gateway_token.trim().is_empty() {
                "Imported without a Gateway Token.".to_string()
            } else {
                "Imported server has not been tested.".to_string()
            },

            version: None,

            gateway_id: None,

            latency_ms: None,

            last_checked_at: None,

            created_at: Some(now.clone()),

            updated_at: Some(now),
        });

        imported_count += 1;
    }

    ensure_single_active_server(&mut servers);

    if let Err(error) = write_servers(&servers) {
        return OpenClawImportResult {
            success: false,

            message: error,

            imported_count: 0,

            skipped_count,
        };
    }

    OpenClawImportResult {
        success: true,

        message: format!(
            "Imported {} OpenClaw server(s); skipped {}.",
            imported_count, skipped_count,
        ),

        imported_count,

        skipped_count,
    }
}

/* ===========================
   Unified active Gateway API
=========================== */

#[tauri::command]
pub fn invoke_active_openclaw_gateway(request: OpenClawGatewayRequest) -> OpenClawGatewayResponse {
    let servers = match read_servers() {
        Ok(value) => value,

        Err(error) => {
            return OpenClawGatewayResponse {
                success: false,

                message: error,

                data: None,

                raw_response: None,
            };
        }
    };

    let Some(active) = servers
        .iter()
        .find(|server| server.active && server.enabled)
    else {
        return OpenClawGatewayResponse {
            success: false,

            message: "No active OpenClaw server is configured.".to_string(),

            data: None,

            raw_response: None,
        };
    };

    let mut session = match open_gateway_session(&active.server_url, &active.gateway_token) {
        Ok(value) => value,

        Err(failure) => {
            return OpenClawGatewayResponse {
                success: false,

                message: failure.message,

                data: None,

                raw_response: raw_json(failure.payload.as_ref()),
            };
        }
    };

    let result = invoke_gateway_method(&mut session, &request.method, request.params);

    let _ = session.socket.close(None);

    match result {
        Ok(payload) => OpenClawGatewayResponse {
            success: true,

            message: format!(
                "OpenClaw Gateway method {} completed successfully.",
                request.method,
            ),

            raw_response: raw_json(Some(&payload)),

            data: Some(payload),
        },

        Err(failure) => OpenClawGatewayResponse {
            success: false,

            message: failure.message,

            data: None,

            raw_response: raw_json(failure.payload.as_ref()),
        },
    }
}
