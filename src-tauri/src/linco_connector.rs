//! Built-in Linco connector: AI-OS itself plays the role of `linco-connect`
//! (lincotalk/linco-bridge, MIT) on the official Linco channel, so the Linco
//! App reaches AI-OS without any separate program on this Mac.
//!
//! Protocol follows linco-bridge-connect at commit
//! 536a257dbce75de2e7000ff3b8bb01b010fb2dfe (Copyright (c) Linco contributors,
//! MIT License): one authenticated WebSocket per paired Agent slot,
//! ping/pong heartbeats, presence events, `inbound_message` in and
//! `turn_start` / `stream_chunk` / `turn_end` out. Messages are answered by the
//! existing Remote AI-OS gateway; nothing runs outside AI-OS orchestration.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, VecDeque},
    net::{TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tungstenite::{
    client::IntoClientRequest, stream::MaybeTlsStream, Error as WebSocketError, Message, WebSocket,
};

const OFFICIAL_WS_BASE: &str = "wss://app.lincotalk.com/socket/ai";
const AGENT_TYPES: [&str; 5] = ["claude", "codex", "deepseek", "hermes", "openclaw"];
const KEYCHAIN_ACCOUNT: &str = "linco-connector";
const SETTINGS_FILE: &str = "linco-connector.json";
const HEARTBEAT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
const READ_POLL: Duration = Duration::from_millis(500);
const TURN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_INBOUND_TEXT: usize = 16 * 1024;
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const DEDUPE_CAPACITY: usize = 2048;
const STARTUP_DELAY: Duration = Duration::from_secs(3);

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ws_url: Option<String>,
    device_id: String,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ConnectionState {
    NotPaired,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LincoConnectorStatus {
    state: ConnectionState,
    agent_type: Option<String>,
    device_name: String,
    last_error: Option<String>,
    connected_at: Option<String>,
}

/// What a paired Linco App reply such as "/approve" refers to.
#[derive(Clone, Debug, PartialEq)]
enum PendingDecision {
    Recommendation {
        council_session_id: String,
        recommendation_id: String,
    },
    Confirmation {
        task_id: String,
        confirmation_id: String,
    },
}

struct Running {
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    status: Option<LincoConnectorStatus>,
    running: Option<Running>,
    pending: HashMap<String, PendingDecision>,
}

#[derive(Clone, Default)]
pub(crate) struct LincoConnectorState {
    inner: Arc<Mutex<Inner>>,
}

impl LincoConnectorState {
    fn set_status(&self, state: ConnectionState, agent_type: Option<&str>, error: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        let connected_at = match (&state, inner.status.as_ref()) {
            (ConnectionState::Connected, Some(previous))
                if previous.state == ConnectionState::Connected =>
            {
                previous.connected_at.clone()
            }
            (ConnectionState::Connected, _) => Some(chrono::Utc::now().to_rfc3339()),
            _ => None,
        };
        inner.status = Some(LincoConnectorStatus {
            state,
            agent_type: agent_type.map(str::to_owned),
            device_name: device_name(),
            last_error: error,
            connected_at,
        });
    }

    fn status(&self) -> LincoConnectorStatus {
        self.inner
            .lock()
            .unwrap()
            .status
            .clone()
            .unwrap_or_else(|| LincoConnectorStatus {
                state: ConnectionState::NotPaired,
                agent_type: None,
                device_name: device_name(),
                last_error: None,
                connected_at: None,
            })
    }

    fn stop_running(&self) {
        if let Some(running) = self.inner.lock().unwrap().running.take() {
            running.stop.store(true, Ordering::SeqCst);
        }
    }

    fn remember(&self, session_key: &str, decision: Option<PendingDecision>) {
        let mut inner = self.inner.lock().unwrap();
        match decision {
            Some(decision) => {
                inner.pending.insert(session_key.to_owned(), decision);
            }
            None => {
                inner.pending.remove(session_key);
            }
        }
    }

    fn pending(&self, session_key: &str) -> Option<PendingDecision> {
        self.inner.lock().unwrap().pending.get(session_key).cloned()
    }
}

/// The pairing taken from the Linco App's Bridge page.
#[derive(Debug, PartialEq)]
struct Pairing {
    app_id: String,
    app_secret: String,
    agent_type: String,
    ws_url: Option<String>,
}

fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for character in text.chars() {
        match (quote, character) {
            (Some(open), close) if close == open => quote = None,
            (Some(_), other) => current.push(other),
            (None, '"' | '\'') => {
                quote = Some(character);
                started = true;
            }
            (None, '\\') => {}
            (None, space) if space.is_whitespace() => {
                if started || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, other) => current.push(other),
        }
    }
    if started || !current.is_empty() {
        words.push(current);
    }
    words
}

fn option_value(words: &[String], name: &str) -> Option<String> {
    let flag = format!("--{name}");
    let prefix = format!("--{name}=");
    words.iter().enumerate().find_map(|(index, word)| {
        if word == &flag {
            words.get(index + 1).cloned()
        } else {
            word.strip_prefix(&prefix).map(str::to_owned)
        }
    })
}

fn valid_credential_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .chars()
            .all(|character| character.is_ascii_graphic() && character != ':')
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn clean_value(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches(|character| character == '"' || character == '\'' || character == '`')
        .trim()
        .to_owned()
}

/// Fields from JSON or "Key: value" lines, keyed by `normalized_key`.
fn labelled_fields(text: &str) -> HashMap<String, String> {
    fn walk(value: &Value, fields: &mut HashMap<String, String>) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if let Some(text) = value.as_str() {
                        fields
                            .entry(normalized_key(key))
                            .or_insert_with(|| text.trim().to_owned());
                    } else {
                        walk(value, fields);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, fields)),
            _ => {}
        }
    }
    let mut fields = HashMap::new();
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        walk(&value, &mut fields);
    }
    for line in text.lines() {
        let Some(index) = line.find([':', '\u{ff1a}', '=']) else {
            continue;
        };
        let (key, rest) = line.split_at(index);
        let separator = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        let value = clean_value(&rest[separator..]);
        let key = normalized_key(key);
        if !key.is_empty() && !value.is_empty() {
            fields.entry(key).or_insert(value);
        }
    }
    fields
}

/// Accepts what the Linco App hands out: the generated
/// `linco-connect init --token "<app-id>:<app-secret>" --agent codex` commands,
/// or the App ID / App Secret pair (as copied, JSON or "App ID: ..." lines).
/// `fallback_agent` is the slot chosen in AI-OS when the text names none.
fn parse_pairing(text: &str, fallback_agent: Option<&str>) -> Result<Pairing, String> {
    let words = shell_words(text);
    let fields = labelled_fields(text);
    // Exact labels first, then prefixed ones such as LINCO_APP_ID.
    let field = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| fields.get(*name).cloned())
            .or_else(|| {
                names.iter().find_map(|name| {
                    fields
                        .iter()
                        .find(|(key, _)| key.len() <= name.len() + 16 && key.ends_with(name))
                        .map(|(_, value)| value.clone())
                })
            })
    };
    let token = option_value(&words, "token").or_else(|| field(&["token", "lincotoken"]));
    let (token_id, token_secret) = token
        .as_deref()
        .and_then(|token| token.split_once(':'))
        .map(|(id, secret)| (Some(id.trim().to_owned()), Some(secret.trim().to_owned())))
        .unwrap_or((None, None));
    let app_id = option_value(&words, "app-id")
        .or(token_id)
        .or_else(|| field(&["appid", "clientid"]))
        .map(|value| clean_value(&value));
    let app_secret = option_value(&words, "app-secret")
        .or(token_secret)
        .or_else(|| field(&["appsecret", "clientsecret", "secret"]))
        .map(|value| clean_value(&value));
    let (Some(app_id), Some(app_secret)) = (app_id, app_secret) else {
        return Err(
            "Paste the App ID and App Secret (or the generated commands) from the Linco App."
                .to_owned(),
        );
    };
    if !valid_credential_part(&app_id) || !valid_credential_part(&app_secret) {
        return Err(
            "The Linco App ID or App Secret looks incomplete. Copy it again from the Linco App."
                .to_owned(),
        );
    }
    let agent_type = option_value(&words, "agent")
        .or_else(|| field(&["agent", "agenttype"]))
        .or_else(|| fallback_agent.map(str::to_owned))
        .map(|value| value.trim().to_ascii_lowercase())
        .ok_or_else(|| "Choose which Linco Agent slot AI-OS should use.".to_owned())?;
    if !AGENT_TYPES.contains(&agent_type.as_str()) {
        return Err(format!(
            "Linco does not offer an Agent slot named {agent_type}."
        ));
    }
    let ws_url = option_value(&words, "ws-url")
        .or_else(|| field(&["wsurl", "websocketurl"]))
        .map(|value| value.trim().to_owned());
    if let Some(url) = &ws_url {
        validate_ws_url(url)?;
    }
    Ok(Pairing {
        app_id,
        app_secret,
        agent_type,
        ws_url,
    })
}

fn validate_ws_url(value: &str) -> Result<(), String> {
    let url =
        url::Url::parse(value).map_err(|_| "The Linco address is not a valid URL.".to_owned())?;
    if url.scheme() != "wss" || url.host_str().is_none() {
        return Err("The Linco address must use wss://.".to_owned());
    }
    Ok(())
}

fn connection_url(settings: &Settings, token: &str) -> Result<String, String> {
    let base = settings
        .ws_url
        .clone()
        .unwrap_or_else(|| format!("{OFFICIAL_WS_BASE}/{}", settings.agent_type));
    validate_ws_url(&base)?;
    let mut url = url::Url::parse(&base).map_err(|_| "The Linco address is invalid.".to_owned())?;
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !matches!(key.as_ref(), "appId" | "appSecret" | "token"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(kept)
        .append_pair("token", token);
    Ok(url.to_string())
}

fn settings_path() -> Result<std::path::PathBuf, String> {
    Ok(crate::providers::provider_config_directory()?.join(SETTINGS_FILE))
}

fn read_settings() -> Option<Settings> {
    let bytes = std::fs::read(settings_path().ok()?).ok()?;
    let settings: Settings = serde_json::from_slice(&bytes).ok()?;
    AGENT_TYPES
        .contains(&settings.agent_type.as_str())
        .then_some(settings)
}

fn write_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "AI-OS could not save the Linco connection".to_owned())?;
    }
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|_| "AI-OS could not save the Linco connection".to_owned())?;
    std::fs::write(&path, bytes)
        .map_err(|_| "AI-OS could not save the Linco connection".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn device_name() -> String {
    sysinfo::System::host_name()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Mac".to_owned())
}

fn device(settings: &Settings, online: bool) -> Value {
    if !online {
        return json!({ "id": settings.device_id });
    }
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    json!({
        "id": settings.device_id,
        "name": device_name(),
        "platform": platform,
        "arch": arch,
    })
}

fn client_info() -> Value {
    json!({
        "name": "ai-os",
        "version": env!("CARGO_PKG_VERSION"),
        "installType": "app",
        "selfUpdateSupported": false,
    })
}

fn heartbeat(settings: &Settings, kind: &str) -> Value {
    json!({
        "type": kind,
        "from": settings.agent_type,
        "to": "robot",
        "source": "ws",
        "ts": chrono::Utc::now().timestamp_millis(),
        "accountId": "main",
        "agentId": "main",
        "channel": "linco",
        "device": device(settings, true),
        "client": client_info(),
    })
}

fn presence(settings: &Settings, status: &str, reason: Option<&str>) -> Value {
    let mut value = json!({
        "type": "presence_event",
        "from": settings.agent_type,
        "to": "robot",
        "source": "ws",
        "ts": chrono::Utc::now().timestamp_millis(),
        "accountId": "main",
        "agentId": "main",
        "channel": "linco",
        "status": status,
        "device": device(settings, status != "offline"),
        "client": client_info(),
    });
    if let Some(reason) = reason {
        value["reason"] = Value::String(reason.to_owned());
    }
    value
}

/// The routing fields a reply must carry back to the same Linco conversation.
#[derive(Clone, Debug)]
struct Turn {
    session_key: String,
    message_id: String,
    stream_id: String,
    meta: Map<String, Value>,
}

impl Turn {
    fn from_inbound(message: &Value) -> Option<Self> {
        let session_key = message.get("sessionKey")?.as_str()?.trim().to_owned();
        if session_key.is_empty() || session_key.len() > 256 {
            return None;
        }
        let message_id = message
            .get("messageId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 256)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("ai-os-{}", uuid::Uuid::new_v4()));
        let mut meta = Map::new();
        for key in [
            "accountId",
            "agentId",
            "userId",
            "targetId",
            "chatType",
            "targetType",
            "channel",
        ] {
            if let Some(value) = message.get(key).filter(|value| value.is_string()) {
                meta.insert(key.to_owned(), value.clone());
            }
        }
        let stream_id = message
            .get("streamId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("linco-stream-{message_id}"));
        Some(Self {
            session_key,
            message_id,
            stream_id,
            meta,
        })
    }

    fn frame(&self, agent_type: &str, mut payload: Map<String, Value>) -> Value {
        for (key, value) in &self.meta {
            payload.entry(key.clone()).or_insert_with(|| value.clone());
        }
        payload.entry("accountId").or_insert_with(|| json!("main"));
        payload.entry("agentId").or_insert_with(|| json!("main"));
        payload.entry("channel").or_insert_with(|| json!("linco"));
        payload.insert("sessionKey".to_owned(), json!(self.session_key));
        payload.insert("messageId".to_owned(), json!(self.message_id));
        payload.insert("streamId".to_owned(), json!(self.stream_id));
        payload.insert("from".to_owned(), json!(agent_type));
        payload.insert("to".to_owned(), json!("robot"));
        payload.insert("source".to_owned(), json!("ws"));
        payload.insert(
            "ts".to_owned(),
            json!(chrono::Utc::now().timestamp_millis()),
        );
        Value::Object(payload)
    }

    fn start(&self, agent_type: &str) -> Value {
        let mut payload = Map::new();
        payload.insert("type".to_owned(), json!("turn_start"));
        payload.insert("reason".to_owned(), json!("started"));
        self.frame(agent_type, payload)
    }

    fn answer(&self, agent_type: &str, text: &str) -> Value {
        let mut payload = Map::new();
        payload.insert("type".to_owned(), json!("stream_chunk"));
        payload.insert("mode".to_owned(), json!("chunk"));
        payload.insert("delta".to_owned(), json!(""));
        payload.insert("fullText".to_owned(), json!(text));
        payload.insert("phase".to_owned(), json!("final_answer"));
        payload.insert("ephemeral".to_owned(), json!(false));
        payload.insert("replacePrevious".to_owned(), json!(false));
        payload.insert("done".to_owned(), json!(true));
        self.frame(agent_type, payload)
    }

    fn notice(&self, agent_type: &str, text: &str, fault: bool) -> Value {
        let mut payload = Map::new();
        payload.insert("type".to_owned(), json!("outbound_message"));
        payload.insert("text".to_owned(), json!(text));
        payload.insert("bridgeFault".to_owned(), json!(fault));
        let mut frame = self.frame(agent_type, payload);
        let kind = if fault { "error" } else { "system" };
        frame["messageId"] = json!(format!(
            "linco-{kind}-{}",
            chrono::Utc::now().timestamp_millis()
        ));
        frame
    }

    fn command_result(&self, agent_type: &str, command: &str, data: Value) -> Value {
        let mut payload = Map::new();
        payload.insert("type".to_owned(), json!("slash_command_result"));
        payload.insert("command".to_owned(), json!(command));
        payload.insert("version".to_owned(), json!(1));
        payload.insert("data".to_owned(), data);
        payload.insert("requestId".to_owned(), json!(self.message_id));
        self.frame(agent_type, payload)
    }

    fn end(&self, agent_type: &str, reason: &str) -> Value {
        let mut payload = Map::new();
        payload.insert("type".to_owned(), json!("turn_end"));
        payload.insert("reason".to_owned(), json!(reason));
        payload.insert("requestId".to_owned(), json!(self.message_id));
        self.frame(agent_type, payload)
    }
}

/// AI-OS is the single context the Linco App can bind this slot to.
const AI_OS_CONTEXT_ID: &str = "ai-os";

fn context_picker() -> Value {
    let bind = format!("/agent --bind {AI_OS_CONTEXT_ID}");
    json!({
        "version": 1,
        "current": AI_OS_CONTEXT_ID,
        "defaultId": AI_OS_CONTEXT_ID,
        "defaultProfile": AI_OS_CONTEXT_ID,
        "account": "",
        "listError": "",
        "items": [{
            "index": 1,
            "id": AI_OS_CONTEXT_ID,
            "name": "AI-OS",
            "model": "AI-OS orchestration",
            "workspace": "",
            "agentDir": "",
            "isCurrent": true,
            "isDefault": true,
            "account": "",
            "command": bind,
            "bindCommand": bind,
        }],
        "actions": [{
            "label": "✓ AI-OS",
            "text": bind,
            "command": bind,
            "type": "command",
            "action": "bind",
            "agentId": AI_OS_CONTEXT_ID,
            "account": "",
            "bindCommand": bind,
        }],
    })
}

/// Bridge control commands the Linco App sends while importing or opening a
/// chat (linco-connect's /agent, /profile and /history). They are answered
/// here; everything else, including /approve and /reject, goes to AI-OS.
fn bridge_command(text: &str, turn: &Turn, agent_type: &str) -> Option<Vec<Value>> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut words = trimmed.split_whitespace();
    let command = words.next()?.to_ascii_lowercase();
    let argument = words.next().unwrap_or_default();
    let reply = match command.as_str() {
        "/agent" | "/profile" => match argument {
            "--bind" => turn.notice(agent_type, "This Linco Agent is connected to AI-OS.", false),
            "--cancel" => turn.notice(agent_type, "Cancelled.", false),
            _ => turn.command_result(agent_type, command.trim_start_matches('/'), context_picker()),
        },
        "/history" | "/history-reload" | "/sync-history" => {
            turn.command_result(agent_type, "history", json!({ "rounds": [] }))
        }
        "/help" | "/status" => turn.notice(
            agent_type,
            "Connected to AI-OS. Send any request; actions that change something ask for /approve or /reject.",
            false,
        ),
        _ => return None,
    };
    Some(vec![reply, turn.end(agent_type, "completed")])
}

fn routed_to(message: &Value, agent_type: &str) -> bool {
    match message.get("to").and_then(Value::as_str) {
        None => true,
        Some(to) => ["agent", "claude", agent_type].contains(&to),
    }
}

fn string_field<'a>(event: &'a Value, key: &str) -> Option<&'a str> {
    event
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Renders Remote AI-OS gateway events as one Linco answer, and the decision
/// (if any) the next "/approve" or "/reject" refers to.
fn render_events(events: &[Value]) -> (String, bool, Option<PendingDecision>) {
    let mut lines: Vec<String> = Vec::new();
    let mut failed = false;
    let mut decision = None;
    // The streamed answer: chunks are fragments of one text, and the closing
    // "completed" event repeats that text in full, so it replaces them.
    let mut answer: Option<(usize, String)> = None;
    for event in events {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "council_recommendation"
            | "recommendation_approval_required"
            | "revised_recommendation" => {
                if let Some(recommendation) = event.get("recommendation") {
                    if let Some(summary) = string_field(recommendation, "summary") {
                        lines.push(summary.to_owned());
                    }
                    if let Some(actions) = recommendation
                        .get("proposedActions")
                        .and_then(Value::as_array)
                    {
                        for action in actions.iter().filter_map(Value::as_str) {
                            lines.push(format!("• {action}"));
                        }
                    }
                } else if let Some(message) = string_field(event, "message") {
                    lines.push(message.to_owned());
                }
                if kind != "council_recommendation" {
                    if let (Some(council), Some(recommendation)) = (
                        string_field(event, "councilSessionId"),
                        string_field(event, "recommendationId"),
                    ) {
                        decision = Some(PendingDecision::Recommendation {
                            council_session_id: council.to_owned(),
                            recommendation_id: recommendation.to_owned(),
                        });
                    }
                }
            }
            "execution_confirmation_required" => {
                if let Some(confirmation) = event.get("confirmation") {
                    let action = string_field(confirmation, "action").unwrap_or("this action");
                    let reason = string_field(confirmation, "reason").unwrap_or_default();
                    lines.push(
                        format!("Confirmation needed: {action}. {reason}")
                            .trim()
                            .to_owned(),
                    );
                } else if let Some(message) = string_field(event, "message") {
                    lines.push(message.to_owned());
                }
                if let (Some(task), Some(confirmation)) = (
                    string_field(event, "taskId"),
                    string_field(event, "confirmationId"),
                ) {
                    decision = Some(PendingDecision::Confirmation {
                        task_id: task.to_owned(),
                        confirmation_id: confirmation.to_owned(),
                    });
                }
            }
            "failed" | "error" | "blocked" => {
                failed |= kind != "blocked";
                if let Some(message) = string_field(event, "message") {
                    lines.push(message.to_owned());
                }
            }
            "session_ready" | "turn_start" | "turn_end" => {}
            "assistant_chunk" | "completed" | "cancelled" => {
                let Some(text) = event.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let (_, streamed) = answer.get_or_insert_with(|| (lines.len(), String::new()));
                if kind == "assistant_chunk" {
                    streamed.push_str(text);
                } else if !text.trim().is_empty() {
                    *streamed = text.to_owned();
                }
            }
            _ => {
                if let Some(message) = string_field(event, "message") {
                    lines.push(message.to_owned());
                }
            }
        }
    }
    if let Some((position, text)) = answer {
        let text = text.trim();
        if !text.is_empty() {
            lines.insert(position.min(lines.len()), text.to_owned());
        }
    }
    if decision.is_some() {
        lines.push("Reply /approve to continue or /reject to decline.".to_owned());
    }
    lines.dedup();
    (lines.join("\n\n"), failed, decision)
}

/// The Remote AI-OS envelope for one Linco message.
fn envelope_for(
    message: &Value,
    turn: &Turn,
    pending: Option<PendingDecision>,
) -> Result<Value, String> {
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let approval = match kind {
        "danger_confirm" | "permission_response" => {
            Some(message.get("approved") == Some(&json!(true)))
        }
        "inbound_message" => {
            let text = message
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            match text {
                "/approve" if pending.is_some() => Some(true),
                "/reject" if pending.is_some() => Some(false),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(approved) = approval {
        let decision = if approved { "approve" } else { "reject" };
        return match pending {
            Some(PendingDecision::Recommendation {
                council_session_id,
                recommendation_id,
            }) => Ok(json!({
                "type": "recommendation_response",
                "sessionKey": turn.session_key,
                "councilSessionId": council_session_id,
                "recommendationId": recommendation_id,
                "approved": approved,
                "decision": decision,
            })),
            Some(PendingDecision::Confirmation {
                task_id,
                confirmation_id,
            }) => Ok(json!({
                "type": "permission_response",
                "sessionKey": turn.session_key,
                "taskId": task_id,
                "confirmationId": confirmation_id,
                "approved": approved,
                "decision": decision,
            })),
            None => Err("Nothing is waiting for a decision.".to_owned()),
        };
    }
    let text = message
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "The message is empty.".to_owned())?;
    if text.len() > MAX_INBOUND_TEXT {
        return Err("The message is too long.".to_owned());
    }
    Ok(json!({
        "type": "inbound_message",
        "sessionKey": turn.session_key,
        "messageId": turn.message_id,
        "text": text,
    }))
}

struct Dedupe {
    seen: VecDeque<String>,
}

impl Dedupe {
    fn first_time(&mut self, key: String) -> bool {
        if self.seen.contains(&key) {
            return false;
        }
        if self.seen.len() >= DEDUPE_CAPACITY {
            self.seen.pop_front();
        }
        self.seen.push_back(key);
        true
    }
}

fn open_socket(url: &str) -> Result<Socket, String> {
    let request = url
        .into_client_request()
        .map_err(|_| "The Linco address is invalid.".to_owned())?;
    let host = request.uri().host().unwrap_or_default().to_owned();
    let port = request.uri().port_u16().unwrap_or(443);
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "Linco could not be reached (DNS).".to_owned())?
        .next()
        .ok_or_else(|| "Linco could not be reached (DNS).".to_owned())?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
        .map_err(|_| "Linco could not be reached.".to_owned())?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT)).ok();
    stream.set_write_timeout(Some(CONNECT_TIMEOUT)).ok();
    let (socket, _) = tungstenite::client_tls(request, stream).map_err(|error| match error {
        tungstenite::HandshakeError::Failure(WebSocketError::Http(response)) => format!(
            "Linco refused the connection (HTTP {}). Pair again from the Linco App if this persists.",
            response.status().as_u16()
        ),
        _ => "Linco could not be reached.".to_owned(),
    })?;
    if let MaybeTlsStream::NativeTls(stream) = socket.get_ref() {
        stream.get_ref().set_read_timeout(Some(READ_POLL)).ok();
    }
    Ok(socket)
}

fn send(socket: &mut Socket, frame: &Value) -> Result<(), WebSocketError> {
    socket.send(Message::text(frame.to_string()))
}

enum Outbound {
    Frame(Value),
}

const LOG_FILE: &str = "linco-connector.log";
const LOG_MAX_BYTES: u64 = 256 * 1024;

/// Local diagnostics only: frame types, slash-command names and connection
/// events. Never message text, tokens or user identifiers.
fn log_event(line: &str) {
    let Ok(directory) = crate::providers::provider_config_directory() else {
        return;
    };
    let path = directory.join(LOG_FILE);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > LOG_MAX_BYTES) {
        let _ = std::fs::rename(&path, directory.join(format!("{LOG_FILE}.1")));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{} {line}", chrono::Utc::now().to_rfc3339());
    }
}

fn describe_inbound(message: &Value) -> String {
    let kind = message.get("type").and_then(Value::as_str).unwrap_or("?");
    let to = message.get("to").and_then(Value::as_str).unwrap_or("-");
    let command = message
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| text.starts_with('/'))
        .and_then(|text| text.split_whitespace().next())
        .filter(|word| {
            word.len() <= 32
                && word[1..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .unwrap_or("");
    format!("inbound type={kind} to={to} command={command}")
}

fn handle_message(
    app: &AppHandle,
    state: &LincoConnectorState,
    settings: &Settings,
    message: Value,
    dedupe: &mut Dedupe,
    outbound: &mpsc::Sender<Outbound>,
) -> Option<Value> {
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(kind, "ping" | "pong") {
        log_event(&describe_inbound(&message));
    }
    match kind {
        "ping" => return Some(heartbeat(settings, "pong")),
        "pong" => return None,
        "inbound_message" | "stop_turn" | "danger_confirm" | "permission_response" => {}
        _ => return None,
    }
    if kind == "inbound_message" && !routed_to(&message, &settings.agent_type) {
        return None;
    }
    let turn = Turn::from_inbound(&message)?;
    if kind == "inbound_message" {
        let key = format!(
            "{}:{}",
            message
                .get("accountId")
                .and_then(Value::as_str)
                .unwrap_or("default"),
            turn.message_id
        );
        if !dedupe.first_time(key) {
            return None;
        }
    }
    let agent_type = settings.agent_type.clone();
    if kind == "stop_turn" {
        let envelope = json!({ "type": "stop_turn", "sessionKey": turn.session_key });
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let remote = app.state::<crate::remote_linco::RemoteLincoBridgeState>();
            let _ =
                crate::remote_linco::dispatch_inbound(&app, &remote, envelope, TURN_TIMEOUT).await;
        });
        return Some(turn.end(&agent_type, "cancelled"));
    }
    if kind == "inbound_message" {
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(frames) = bridge_command(text, &turn, &agent_type) {
            for frame in frames {
                let _ = outbound.send(Outbound::Frame(frame));
            }
            return None;
        }
    }
    let envelope = match envelope_for(&message, &turn, state.pending(&turn.session_key)) {
        Ok(envelope) => envelope,
        Err(reason) => {
            let _ = outbound.send(Outbound::Frame(turn.notice(&agent_type, &reason, false)));
            return Some(turn.end(&agent_type, "completed"));
        }
    };
    let _ = outbound.send(Outbound::Frame(turn.start(&agent_type)));
    let app = app.clone();
    let state = state.clone();
    let outbound = outbound.clone();
    tauri::async_runtime::spawn(async move {
        let remote = app.state::<crate::remote_linco::RemoteLincoBridgeState>();
        let result =
            crate::remote_linco::dispatch_inbound(&app, &remote, envelope, TURN_TIMEOUT).await;
        let frames = match result {
            Ok(events) => {
                let events = events.as_array().cloned().unwrap_or_default();
                let (text, failed, decision) = render_events(&events);
                state.remember(&turn.session_key, decision);
                let text = if text.is_empty() {
                    "Done.".to_owned()
                } else {
                    text
                };
                vec![
                    turn.answer(&agent_type, &text),
                    turn.end(&agent_type, if failed { "failed" } else { "completed" }),
                ]
            }
            Err(error) => {
                let reason = match error {
                    crate::remote_linco::DispatchError::Unavailable => {
                        "AI-OS is still starting. Please try again in a moment.".to_owned()
                    }
                    crate::remote_linco::DispatchError::TimedOut => {
                        "AI-OS did not finish in time.".to_owned()
                    }
                    crate::remote_linco::DispatchError::Rejected(message) => message,
                };
                vec![
                    turn.notice(&agent_type, &reason, true),
                    turn.end(&agent_type, "failed"),
                ]
            }
        };
        for frame in frames {
            let _ = outbound.send(Outbound::Frame(frame));
        }
    });
    None
}

/// One connection's lifetime; returns why it ended.
fn serve(
    app: &AppHandle,
    state: &LincoConnectorState,
    settings: &Settings,
    socket: &mut Socket,
    stop: &AtomicBool,
    outbound_tx: &mpsc::Sender<Outbound>,
    outbound_rx: &mpsc::Receiver<Outbound>,
    dedupe: &mut Dedupe,
) -> Option<String> {
    if send(socket, &presence(settings, "online", None)).is_err() {
        return Some("Linco closed the connection.".to_owned());
    }
    let mut last_heartbeat = Instant::now();
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = send(socket, &presence(settings, "offline", Some("shutdown")));
            let _ = socket.close(None);
            let _ = socket.flush();
            return None;
        }
        while let Ok(Outbound::Frame(frame)) = outbound_rx.try_recv() {
            if send(socket, &frame).is_err() {
                return Some("Linco closed the connection.".to_owned());
            }
        }
        if last_heartbeat.elapsed() >= HEARTBEAT {
            if send(socket, &heartbeat(settings, "ping")).is_err() {
                return Some("Linco closed the connection.".to_owned());
            }
            last_heartbeat = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) if text.len() <= MAX_FRAME_BYTES => {
                if let Ok(message) = serde_json::from_str::<Value>(text.as_str()) {
                    if let Some(reply) =
                        handle_message(app, state, settings, message, dedupe, outbound_tx)
                    {
                        if send(socket, &reply).is_err() {
                            return Some("Linco closed the connection.".to_owned());
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => return Some("Linco closed the connection.".to_owned()),
            Ok(_) => {}
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Some("The Linco connection dropped.".to_owned()),
        }
    }
}

fn sleep_unless_stopped(stop: &AtomicBool, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn run(
    app: AppHandle,
    state: LincoConnectorState,
    settings: Settings,
    token: String,
    stop: Arc<AtomicBool>,
) {
    let (outbound_tx, outbound_rx) = mpsc::channel();
    let mut dedupe = Dedupe {
        seen: VecDeque::new(),
    };
    let mut attempts: u32 = 0;
    let agent = settings.agent_type.clone();
    while !stop.load(Ordering::SeqCst) {
        state.set_status(
            if attempts == 0 {
                ConnectionState::Connecting
            } else {
                ConnectionState::Reconnecting
            },
            Some(&agent),
            None,
        );
        let ended = connection_url(&settings, &token).and_then(|url| open_socket(&url));
        let reason = match ended {
            Ok(mut socket) => {
                attempts = 0;
                log_event(&format!("connected slot={agent}"));
                state.set_status(ConnectionState::Connected, Some(&agent), None);
                serve(
                    &app,
                    &state,
                    &settings,
                    &mut socket,
                    &stop,
                    &outbound_tx,
                    &outbound_rx,
                    &mut dedupe,
                )
            }
            Err(reason) => Some(reason),
        };
        let Some(reason) = reason else { break };
        let delay = RECONNECT_MIN
            .saturating_mul(2u32.saturating_pow(attempts.min(5)))
            .min(RECONNECT_MAX);
        attempts = attempts.saturating_add(1);
        log_event(&format!("disconnected: {reason}"));
        state.set_status(ConnectionState::Reconnecting, Some(&agent), Some(reason));
        sleep_unless_stopped(&stop, delay);
    }
}

fn read_token() -> Option<String> {
    crate::providers::read_secret(KEYCHAIN_ACCOUNT, "linco_connector_start")
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|token| token.contains(':'))
}

fn launch(app: &AppHandle, state: &LincoConnectorState, settings: Settings, token: String) {
    state.stop_running();
    let stop = Arc::new(AtomicBool::new(false));
    state.inner.lock().unwrap().running = Some(Running { stop: stop.clone() });
    let app = app.clone();
    let state = state.clone();
    std::thread::Builder::new()
        .name("ai-os-linco-connector".to_owned())
        .spawn(move || run(app, state, settings, token, stop))
        .ok();
}

/// Called once at launch: reconnect a paired Linco App without any user step.
pub(crate) fn start(app: AppHandle) {
    let state = app.state::<LincoConnectorState>().inner().clone();
    let Some(settings) = read_settings() else {
        state.set_status(ConnectionState::NotPaired, None, None);
        return;
    };
    std::thread::spawn(move || {
        // Let the window register the Remote AI-OS listener first.
        std::thread::sleep(STARTUP_DELAY);
        match read_token() {
            Some(token) => launch(&app, &state, settings, token),
            None => state.set_status(
                ConnectionState::NotPaired,
                Some(&settings.agent_type),
                Some("The Linco pairing is missing from Keychain. Pair again.".to_owned()),
            ),
        }
    });
}

pub(crate) fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<LincoConnectorState>() {
        state.stop_running();
    }
}

#[tauri::command]
pub(crate) fn get_linco_connector_status(
    state: tauri::State<'_, LincoConnectorState>,
) -> LincoConnectorStatus {
    state.status()
}

#[tauri::command]
pub(crate) fn pair_linco_connector(
    app: AppHandle,
    state: tauri::State<'_, LincoConnectorState>,
    commands: String,
    agent_type: Option<String>,
) -> Result<LincoConnectorStatus, String> {
    let pairing = parse_pairing(&commands, agent_type.as_deref())?;
    let settings = Settings {
        agent_type: pairing.agent_type,
        ws_url: pairing.ws_url,
        device_id: read_settings()
            .map(|settings| settings.device_id)
            .unwrap_or_else(|| format!("ai-os-{}", uuid::Uuid::new_v4())),
    };
    let token = format!("{}:{}", pairing.app_id, pairing.app_secret);
    crate::providers::store_secret(KEYCHAIN_ACCOUNT, token.as_bytes(), "linco_connector_pair")?;
    write_settings(&settings)?;
    state.set_status(
        ConnectionState::Connecting,
        Some(&settings.agent_type),
        None,
    );
    launch(&app, &state, settings, token);
    Ok(state.status())
}

#[tauri::command]
pub(crate) fn unpair_linco_connector(
    state: tauri::State<'_, LincoConnectorState>,
) -> Result<LincoConnectorStatus, String> {
    state.stop_running();
    crate::providers::remove_secret(KEYCHAIN_ACCOUNT, "linco_connector_unpair")?;
    if let Ok(path) = settings_path() {
        let _ = std::fs::remove_file(path);
    }
    state.set_status(ConnectionState::NotPaired, None, None);
    Ok(state.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_COMMANDS: &str = "npm install -g linco-connect\nlinco-connect init --token \"app-123:sec-XYZ_9\" --agent codex\nlinco-connect start --daemon";

    #[test]
    fn pairing_reads_the_linco_app_commands() {
        let pairing = parse_pairing(APP_COMMANDS, None).unwrap();
        assert_eq!(
            pairing,
            Pairing {
                app_id: "app-123".into(),
                app_secret: "sec-XYZ_9".into(),
                agent_type: "codex".into(),
                ws_url: None,
            }
        );
        let continued = parse_pairing("linco-connect init \\\n  --token 'a:b' \\\n  --agent=openclaw --ws-url wss://example.test/socket/ai/openclaw", None).unwrap();
        assert_eq!(continued.agent_type, "openclaw");
        assert_eq!(
            continued.ws_url.as_deref(),
            Some("wss://example.test/socket/ai/openclaw")
        );
    }

    #[test]
    fn pairing_rejects_incomplete_or_unsafe_input() {
        assert!(parse_pairing("app-123:sec", Some("openclaw")).is_err());
        assert!(parse_pairing("linco-connect init --token app-123 --agent codex", None).is_err());
        assert!(parse_pairing("linco-connect init --token a:b", None).is_err());
        assert!(parse_pairing("linco-connect init --token a:b --agent ai-os", None).is_err());
        assert!(parse_pairing(
            "linco-connect init --token a:b --agent codex --ws-url ws://insecure",
            None
        )
        .is_err());
        assert!(parse_pairing("App ID: b1b09c7788fb", Some("openclaw")).is_err());
    }

    #[test]
    fn pairing_reads_the_app_id_and_secret_the_openclaw_page_copies() {
        let expected = Pairing {
            app_id: "b1b09c7788fb".into(),
            app_secret: "s3cr3t-Value".into(),
            agent_type: "openclaw".into(),
            ws_url: None,
        };
        let lines = "App ID: b1b09c7788fb\nApp Secret\u{ff1a} s3cr3t-Value";
        assert_eq!(parse_pairing(lines, Some("openclaw")).unwrap(), expected);
        let json =
            r#"{"channels": {"linco": {"appId": "b1b09c7788fb", "appSecret": "s3cr3t-Value"}}}"#;
        assert_eq!(parse_pairing(json, Some("openclaw")).unwrap(), expected);
        let env = "LINCO_APP_ID=b1b09c7788fb\nLINCO_APP_SECRET=\"s3cr3t-Value\"";
        assert_eq!(parse_pairing(env, Some("openclaw")).unwrap(), expected);
        assert!(parse_pairing(lines, None).is_err());
    }

    #[test]
    fn connection_url_uses_the_official_slot_and_replaces_stray_credentials() {
        let settings = Settings {
            agent_type: "openclaw".into(),
            ws_url: None,
            device_id: "ai-os-device".into(),
        };
        assert_eq!(
            connection_url(&settings, "a:b").unwrap(),
            "wss://app.lincotalk.com/socket/ai/openclaw?token=a%3Ab"
        );
        let custom = Settings {
            ws_url: Some("wss://example.test/x?appSecret=old&keep=1".into()),
            ..settings
        };
        assert_eq!(
            connection_url(&custom, "a:b").unwrap(),
            "wss://example.test/x?keep=1&token=a%3Ab"
        );
    }

    #[test]
    fn heartbeat_identifies_ai_os_honestly() {
        let settings = Settings {
            agent_type: "codex".into(),
            ws_url: None,
            device_id: "ai-os-device".into(),
        };
        let ping = heartbeat(&settings, "ping");
        assert_eq!(ping["type"], "ping");
        assert_eq!(ping["from"], "codex");
        assert_eq!(ping["client"]["name"], "ai-os");
        assert_eq!(
            presence(&settings, "offline", Some("shutdown"))["device"],
            json!({"id": "ai-os-device"})
        );
    }

    #[test]
    fn replies_carry_the_inbound_routing_fields() {
        let inbound = json!({
            "type": "inbound_message", "sessionKey": "s-1", "messageId": "m-1",
            "accountId": "acct", "userId": "u-1", "text": "hello", "to": "codex"
        });
        assert!(routed_to(&inbound, "codex"));
        assert!(!routed_to(&json!({"to": "hermes"}), "codex"));
        let turn = Turn::from_inbound(&inbound).unwrap();
        let answer = turn.answer("codex", "Hi");
        assert_eq!(answer["type"], "stream_chunk");
        assert_eq!(answer["fullText"], "Hi");
        assert_eq!(answer["done"], true);
        assert_eq!(answer["sessionKey"], "s-1");
        assert_eq!(answer["accountId"], "acct");
        assert_eq!(answer["userId"], "u-1");
        assert_eq!(answer["streamId"], "linco-stream-m-1");
        let end = turn.end("codex", "completed");
        assert_eq!(end["requestId"], "m-1");
    }

    #[test]
    fn approval_replies_resolve_the_pending_decision_only() {
        let turn = Turn::from_inbound(&json!({"sessionKey": "s-1", "messageId": "m-2"})).unwrap();
        let pending = PendingDecision::Confirmation {
            task_id: "task-1".into(),
            confirmation_id: "confirm-1".into(),
        };
        let approve = json!({"type": "inbound_message", "text": "/approve"});
        assert_eq!(
            envelope_for(&approve, &turn, Some(pending.clone())).unwrap()["type"],
            "permission_response"
        );
        assert_eq!(
            envelope_for(&approve, &turn, None).unwrap(),
            json!({"type": "inbound_message", "sessionKey": "s-1", "messageId": "m-2", "text": "/approve"})
        );
        let danger = json!({"type": "danger_confirm", "approved": false});
        let rejected = envelope_for(&danger, &turn, Some(pending)).unwrap();
        assert_eq!(rejected["approved"], false);
        assert_eq!(rejected["confirmationId"], "confirm-1");
        assert!(envelope_for(&danger, &turn, None).is_err());
    }

    #[test]
    fn gateway_events_render_as_one_answer() {
        let events = vec![
            json!({"type": "turn_start", "message": "ignored"}),
            json!({"type": "assistant_chunk", "message": "Here is the plan."}),
            json!({"type": "execution_confirmation_required", "taskId": "t", "confirmationId": "c",
                   "confirmation": {"action": "Move 3 files", "reason": "It changes your disk."}}),
        ];
        let (text, failed, decision) = render_events(&events);
        assert!(!failed);
        assert!(text.starts_with("Here is the plan."));
        assert!(text.contains("Confirmation needed: Move 3 files. It changes your disk."));
        assert!(text.ends_with("Reply /approve to continue or /reject to decline."));
        assert_eq!(
            decision,
            Some(PendingDecision::Confirmation {
                task_id: "t".into(),
                confirmation_id: "c".into()
            })
        );
        let (_, failed, _) = render_events(&[json!({"type": "failed", "message": "No model"})]);
        assert!(failed);
    }

    #[test]
    fn bridge_commands_list_ai_os_as_the_only_context() {
        let turn = Turn::from_inbound(&json!({
            "sessionKey": "s-1", "messageId": "r-1", "streamId": "linco-cmd-r-1"
        }))
        .unwrap();
        let frames = bridge_command("/agent", &turn, "openclaw").unwrap();
        assert_eq!(frames[0]["type"], "slash_command_result");
        assert_eq!(frames[0]["command"], "agent");
        assert_eq!(frames[0]["streamId"], "linco-cmd-r-1");
        assert_eq!(frames[0]["data"]["items"][0]["id"], "ai-os");
        assert_eq!(
            frames[0]["data"]["items"][0]["bindCommand"],
            "/agent --bind ai-os"
        );
        assert_eq!(frames[1]["type"], "turn_end");
        let bound = bridge_command("/agent --bind ai-os", &turn, "openclaw").unwrap();
        assert_eq!(bound[0]["type"], "outbound_message");
        let history = bridge_command("/history 20", &turn, "openclaw").unwrap();
        assert_eq!(history[0]["data"], json!({"rounds": []}));
        assert!(bridge_command("/approve", &turn, "openclaw").is_none());
        assert!(bridge_command("hello", &turn, "openclaw").is_none());
    }

    #[test]
    fn streamed_chunks_and_the_completed_text_render_once() {
        let events = vec![
            json!({"type": "assistant_chunk", "message": "你好！有什么问题"}),
            json!({"type": "assistant_chunk", "message": "或者需要帮助的"}),
            json!({"type": "assistant_chunk", "message": "吗？"}),
            json!({"type": "completed", "message": "你好！有什么问题或者需要帮助的吗？"}),
        ];
        let (text, failed, _) = render_events(&events);
        assert_eq!(text, "你好！有什么问题或者需要帮助的吗？");
        assert!(!failed);
        let chunks_only = vec![
            json!({"type": "assistant_chunk", "message": "Part one, "}),
            json!({"type": "assistant_chunk", "message": "part two."}),
            json!({"type": "completed"}),
        ];
        assert_eq!(render_events(&chunks_only).0, "Part one, part two.");
    }

    #[test]
    fn duplicate_messages_are_ignored() {
        let mut dedupe = Dedupe {
            seen: VecDeque::new(),
        };
        assert!(dedupe.first_time("a:m-1".into()));
        assert!(!dedupe.first_time("a:m-1".into()));
    }
}
