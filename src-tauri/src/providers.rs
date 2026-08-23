use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

const KEYCHAIN_SERVICE: &str = "com.ai-os.provider";
const OAUTH_CLIENT_KEYCHAIN_SERVICE: &str = "com.ai-os.oauth-client";

const PROVIDER_INSTANCES_FILE: &str = "provider-instances.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderCredentialKind {
    #[serde(rename = "oauth")]
    OAuth,

    #[serde(rename = "api-key")]
    ApiKey,

    #[serde(rename = "local")]
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderAuthenticationMethod {
    ApiKey,

    CliAccount,

    #[serde(rename = "oauth-pkce")]
    OAuthPkce,

    #[serde(rename = "oauth-loopback")]
    OAuthLoopback,

    DeviceCode,
    ImportedCredential,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderConnectionState {
    NotConfigured,
    Connecting,
    ReadyForTest,
    Connected,
    RefreshRequired,
    Expired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderAdapterKind {
    Native,
    Catalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderAdapterDescriptor {
    pub provider_id: String,
    pub display_name: String,
    pub adapter_kind: ProviderAdapterKind,
    pub credential_kinds: Vec<ProviderCredentialKind>,
    pub authentication_methods: Vec<ProviderAuthenticationMethod>,
    pub capabilities: Vec<String>,
    pub supports_model_discovery: bool,
    pub supports_token_refresh: bool,
    pub supports_multiple_credentials: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderCredentialRef {
    pub kind: ProviderCredentialKind,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keychain_account: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    pub refreshable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderModelEntry {
    pub id: String,
    pub provider_instance_id: String,
    pub remote_model_id: String,
    pub display_name: String,

    #[serde(default)]
    pub capabilities: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,

    pub enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProviderInstance {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub credential: ProviderCredentialRef,
    pub connection_state: ProviderConnectionState,

    #[serde(default)]
    pub models: Vec<ProviderModelEntry>,

    pub created_at: String,
    pub updated_at: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tested_at: Option<String>,
}

fn provider_config_directory() -> Result<std::path::PathBuf, String> {
    let base = dirs::config_dir()
        .ok_or_else(|| "Unable to determine the AI-OS configuration directory".to_owned())?;

    Ok(base.join("AI OS"))
}

fn provider_instances_file() -> Result<std::path::PathBuf, String> {
    Ok(provider_config_directory()?.join(PROVIDER_INSTANCES_FILE))
}

fn validate_provider_text(value: &str, label: &str, max_length: usize) -> Result<String, String> {
    let value = value.trim();

    if value.is_empty() || value.len() > max_length {
        return Err(format!("{label} is invalid"));
    }

    if value.chars().any(char::is_control) {
        return Err(format!("{label} contains unsupported characters"));
    }

    Ok(value.to_owned())
}

fn validate_provider_instance(mut instance: ProviderInstance) -> Result<ProviderInstance, String> {
    instance.id = validate_instance_id(&instance.id)?.to_owned();
    instance.provider_id = validate_provider_text(&instance.provider_id, "Provider ID", 100)?;
    instance.display_name =
        validate_provider_text(&instance.display_name, "Provider display name", 200)?;

    if let Some(account) = instance.credential.keychain_account.as_mut() {
        *account = validate_instance_id(account)?.to_owned();
    }

    if instance.models.len() > 500 {
        return Err("Provider contains too many models".to_owned());
    }

    let mut model_ids = std::collections::HashSet::new();
    let mut default_count = 0usize;

    for model in &mut instance.models {
        model.id = validate_provider_text(&model.id, "Provider model ID", 300)?;
        model.provider_instance_id = validate_instance_id(&model.provider_instance_id)?.to_owned();
        model.remote_model_id = validate_model_id(&model.remote_model_id)?.to_owned();
        model.display_name =
            validate_provider_text(&model.display_name, "Provider model name", 300)?;

        if model.provider_instance_id != instance.id {
            return Err("Provider model belongs to another Provider instance".to_owned());
        }

        if !model_ids.insert(model.id.clone()) {
            return Err("Provider contains duplicate model IDs".to_owned());
        }

        if model.is_default {
            default_count += 1;
        }

        model.capabilities = model
            .capabilities
            .iter()
            .map(|capability| capability.trim().to_owned())
            .filter(|capability| !capability.is_empty())
            .collect();

        model.capabilities.sort();
        model.capabilities.dedup();
    }

    if default_count > 1 {
        return Err("Provider contains more than one default model".to_owned());
    }

    instance.created_at =
        validate_provider_text(&instance.created_at, "Provider creation timestamp", 100)?;
    instance.updated_at =
        validate_provider_text(&instance.updated_at, "Provider update timestamp", 100)?;

    if let Some(last_tested_at) = instance.last_tested_at.as_mut() {
        *last_tested_at = validate_provider_text(last_tested_at, "Provider test timestamp", 100)?;
    }

    Ok(instance)
}

fn read_provider_instances_at(path: &std::path::Path) -> Result<Vec<ProviderInstance>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|_| "Unable to read Provider configuration".to_owned())?;

    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    let instances: Vec<ProviderInstance> = serde_json::from_str(&contents)
        .map_err(|_| "Provider configuration is malformed".to_owned())?;

    instances
        .into_iter()
        .map(validate_provider_instance)
        .collect()
}

fn write_provider_instances_at(
    path: &std::path::Path,
    instances: &[ProviderInstance],
) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|_| "Unable to create Provider configuration directory".to_owned())?;
    }

    let contents = serde_json::to_string_pretty(instances)
        .map_err(|_| "Unable to serialize Provider configuration".to_owned())?;

    let temporary_path = path.with_extension("json.tmp");

    std::fs::write(&temporary_path, contents)
        .map_err(|_| "Unable to write Provider configuration".to_owned())?;

    std::fs::rename(&temporary_path, path)
        .map_err(|_| "Unable to finalize Provider configuration".to_owned())
}

fn read_provider_instances() -> Result<Vec<ProviderInstance>, String> {
    read_provider_instances_at(&provider_instances_file()?)
}

fn write_provider_instances(instances: &[ProviderInstance]) -> Result<(), String> {
    write_provider_instances_at(&provider_instances_file()?, instances)
}

#[tauri::command]
pub(crate) fn list_provider_instances() -> Result<Vec<ProviderInstance>, String> {
    read_provider_instances()
}

#[tauri::command]
pub(crate) fn save_provider_instance(
    instance: ProviderInstance,
) -> Result<ProviderInstance, String> {
    let mut instance = validate_provider_instance(instance)?;
    let mut instances = read_provider_instances()?;

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(existing) = instances
        .iter()
        .find(|candidate| candidate.id == instance.id)
    {
        instance.created_at = existing.created_at.clone();
    }

    instance.updated_at = now;

    instances.retain(|candidate| candidate.id != instance.id);
    instances.push(instance.clone());
    instances.sort_by(|left, right| left.id.cmp(&right.id));

    write_provider_instances(&instances)?;

    Ok(instance)
}

#[tauri::command]
pub(crate) fn remove_provider_instance(instance_id: String) -> Result<bool, String> {
    let instance_id = validate_instance_id(instance_id.trim())?;
    let mut instances = read_provider_instances()?;
    let original_length = instances.len();

    instances.retain(|candidate| candidate.id != instance_id);

    if instances.len() == original_length {
        return Ok(false);
    }

    write_provider_instances(&instances)?;

    Ok(true)
}

const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;
const OAUTH_SESSION_TTL_SECONDS: u64 = 10 * 60;
const OAUTH_LOOPBACK_PATH: &str = "/oauth/callback";
const OAUTH_LOOPBACK_MAX_REQUEST_BYTES: usize = 16 * 1024;
const OAUTH_LOOPBACK_MAX_CONNECTIONS: usize = 8;

static AI_CENTER_REQUESTS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();
static OAUTH_SESSIONS: OnceLock<Mutex<HashMap<String, OAuthSession>>> = OnceLock::new();
static OAUTH_CANCELLATIONS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();
static GROK_DEVICE_SESSIONS: OnceLock<Mutex<HashMap<String, GrokDeviceSession>>> = OnceLock::new();
static KIMI_DEVICE_SESSIONS: OnceLock<Mutex<HashMap<String, GrokDeviceSession>>> = OnceLock::new();

const GROK_OAUTH_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const GROK_OAUTH_ISSUER: &str = "https://auth.x.ai";
const GROK_OAUTH_SCOPES: &str = "openid profile email offline_access grok-cli:access api:access conversations:read conversations:write workspaces:read workspaces:write";
const KIMI_OAUTH_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_OAUTH_ISSUER: &str = "https://auth.kimi.com";

#[derive(Clone)]
struct GrokDeviceSession {
    provider_instance_id: String,
    device_code: String,
    interval_seconds: u64,
    expires_at: u64,
    cancellation: CancellationToken,
}

fn ai_center_requests() -> &'static Mutex<HashMap<String, CancellationToken>> {
    AI_CENTER_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn oauth_sessions() -> &'static Mutex<HashMap<String, OAuthSession>> {
    OAUTH_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn oauth_cancellations() -> &'static Mutex<HashMap<String, CancellationToken>> {
    OAUTH_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn grok_device_sessions() -> &'static Mutex<HashMap<String, GrokDeviceSession>> {
    GROK_DEVICE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn kimi_device_sessions() -> &'static Mutex<HashMap<String, GrokDeviceSession>> {
    KIMI_DEVICE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn unix_timestamp_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "System clock is unavailable".to_owned())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderCredentialInput {
    provider_instance_id: String,
    secret: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderCredentialQuery {
    provider_instance_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderCredentialStatus {
    provider_instance_id: String,
    has_credential: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderAdapterQuery {
    provider_id: String,
    provider_instance_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveredProviderModel {
    id: String,
    display_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderAdapterResult {
    provider_id: String,
    level: String,
    message: String,
    models: Vec<DiscoveredProviderModel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginOAuthInput {
    provider_id: String,
    provider_instance_id: String,
    client_id: String,
    authorization_url: String,
    token_url: String,
    scopes: Vec<String>,
    resource_project_id: Option<String>,
    callback_port: Option<u16>,
    callback_path: Option<String>,
    authorization_params: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginOAuthResult {
    authorization_url: String,
    redirect_uri: String,
    state: String,
}

#[derive(Clone, PartialEq, Eq)]
struct OAuthSession {
    provider_id: String,
    provider_instance_id: String,
    client_id: String,
    client_secret: Option<String>,
    token_url: String,
    resource_project_id: Option<String>,
    route_kind: Option<String>,
    redirect_uri: String,
    verifier: String,
    created_at: u64,
    expires_at: u64,
}

impl std::fmt::Debug for OAuthSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthSession")
            .field("provider_id", &self.provider_id)
            .field("provider_instance_id", &self.provider_instance_id)
            .field("client_id", &self.client_id)
            .field("client_secret_configured", &self.client_secret.is_some())
            .field("token_url", &self.token_url)
            .field("resource_project_id", &self.resource_project_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[cfg(target_os = "macos")]
fn read_oauth_client_secret(provider_id: &str) -> Result<Option<String>, String> {
    match security_framework::passwords::get_generic_password(
        OAUTH_CLIENT_KEYCHAIN_SERVICE,
        provider_id,
    ) {
        Ok(secret) => String::from_utf8(secret)
            .map(Some)
            .map_err(|_| "OAuth client credential in Keychain is invalid".to_owned()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(_) => Err("AI-OS could not read the OAuth client credential".to_owned()),
    }
}

#[cfg(not(target_os = "macos"))]
fn read_oauth_client_secret(_provider_id: &str) -> Result<Option<String>, String> {
    Ok(None)
}

impl OAuthSession {
    fn is_expired_at(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOAuthCompletedEvent {
    provider_id: String,
    provider_instance_id: String,
    state: String,
    expires_at: Option<String>,
    refreshable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOAuthErrorEvent {
    provider_id: String,
    provider_instance_id: String,
    state: String,
    message: String,
}

fn oauth_session_key(provider_id: &str, state: &str) -> String {
    format!("{provider_id}:{state}")
}

fn store_oauth_session(state: &str, session: OAuthSession) -> Result<(), String> {
    let key = oauth_session_key(&session.provider_id, state);
    let mut sessions = oauth_sessions()
        .lock()
        .map_err(|_| "OAuth session state is unavailable".to_owned())?;

    sessions.retain(|_, candidate| {
        unix_timestamp_seconds()
            .map(|now| !candidate.is_expired_at(now))
            .unwrap_or(false)
    });

    if sessions.contains_key(&key) {
        return Err("OAuth session already exists".to_owned());
    }

    sessions.insert(key, session);
    Ok(())
}

fn consume_oauth_session(provider_id: &str, state: &str) -> Result<OAuthSession, String> {
    let provider_id = provider_id.trim();
    let state = state.trim();

    if provider_id.is_empty() || state.is_empty() {
        return Err("OAuth callback is incomplete".to_owned());
    }

    let key = oauth_session_key(provider_id, state);
    let session = oauth_sessions()
        .lock()
        .map_err(|_| "OAuth session state is unavailable".to_owned())?
        .remove(&key)
        .ok_or_else(|| "OAuth session is invalid or expired".to_owned())?;

    let now = unix_timestamp_seconds()?;
    if session.is_expired_at(now) {
        return Err("OAuth session is invalid or expired".to_owned());
    }

    if session.provider_id != provider_id {
        return Err("OAuth session Provider does not match".to_owned());
    }

    Ok(session)
}

fn discard_oauth_session(provider_id: &str, state: &str) -> Result<bool, String> {
    let key = oauth_session_key(provider_id.trim(), state.trim());
    let session_removed = oauth_sessions()
        .lock()
        .map_err(|_| "OAuth session state is unavailable".to_owned())?
        .remove(&key)
        .is_some();

    let token = oauth_cancellations()
        .lock()
        .map_err(|_| "OAuth cancellation state is unavailable".to_owned())?
        .remove(&key);
    if let Some(token) = token {
        token.cancel();
        return Ok(true);
    }

    Ok(session_removed)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompleteOAuthInput {
    provider_id: String,
    state: String,
    code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelOAuthInput {
    provider_id: String,
    state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompleteOAuthResult {
    provider_instance_id: String,
    expires_at: Option<String>,
    refreshable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RefreshOAuthResult {
    provider_instance_id: String,
    expires_at: Option<String>,
    refreshable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginGrokDeviceAuthResult {
    state: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    user_code: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct GrokDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GrokDeviceTokenError {
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateProviderResponseInput {
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
    prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateProviderResponseResult {
    provider_id: String,
    model_id: String,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AiCenterExecutionSource {
    Local,
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AiCenterAttemptOutcome {
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCenterAttempt {
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
    source: AiCenterExecutionSource,
    started_at: String,
    completed_at: String,
    latency_ms: u64,
    outcome: AiCenterAttemptOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCenterCanonicalInvocationMetadata {
    invocation_id: String,
    route_mode: AiCenterRouteMode,
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
    source: AiCenterExecutionSource,
    started_at: String,
    completed_at: String,
    latency_ms: u64,
    input_tokens: usize,
    output_tokens: usize,
    token_accuracy: &'static str,
    fallback_occurred: bool,
    attempts: Vec<AiCenterAttempt>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecuteAiCenterInput {
    route_mode: AiCenterRouteMode,
    manual_candidate: Option<AiCenterRouteCandidate>,
    prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecuteAiCenterResult {
    provider_id: String,
    model_id: String,
    text: String,
    metadata: AiCenterCanonicalInvocationMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCenterRouteCandidate {
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AiCenterRouteMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveAiCenterRouteInput {
    route_mode: AiCenterRouteMode,
    manual_candidate: Option<AiCenterRouteCandidate>,
    attempted_count: usize,
    emitted_output: bool,
    cancelled: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutionCapabilityRequirement {
    pub minimum_context_window: u64,
}

pub(crate) fn execution_capability_requirement(
    capability: &str,
) -> Option<ExecutionCapabilityRequirement> {
    match capability {
        "download.start" => Some(ExecutionCapabilityRequirement {
            // Current measured requirement for baidu-drive execution.
            minimum_context_window: 65_536,
        }),
        _ => None,
    }
}

fn normalized_execution_model(model: &str) -> Option<(&str, &str)> {
    let (provider_id, model_id) = model.trim().split_once('/')?;

    let provider_id = if provider_id == "ollama" || provider_id == "ollama-ai-os" {
        "ollama"
    } else {
        provider_id
    };

    Some((provider_id, model_id))
}

fn execution_agent_candidates_from_config(
    capability: &str,
    config: &serde_json::Value,
    route_candidates: &[AiCenterRouteCandidate],
) -> Result<Vec<String>, String> {
    let requirement = execution_capability_requirement(capability)
        .ok_or_else(|| format!("No execution requirement declared for {capability}"))?;

    let agents = config["agents"]["list"]
        .as_array()
        .ok_or_else(|| "OpenClaw execution-agent list is unavailable".to_owned())?;

    let mut candidates = agents
        .iter()
        .filter_map(|agent| {
            let agent_id = agent["id"].as_str()?;

            if !agent_id.starts_with("ai-os-exec-") {
                return None;
            }

            let num_ctx = agent["params"]["num_ctx"].as_u64()?;
            if num_ctx < requirement.minimum_context_window {
                return None;
            }

            let primary_model = agent["model"]["primary"].as_str()?;
            let (provider_id, model_id) = normalized_execution_model(primary_model)?;

            let route_index = route_candidates.iter().position(|candidate| {
                let candidate_provider = if candidate.provider_id == "ollama"
                    || candidate.provider_instance_id == "ollama-local"
                {
                    "ollama"
                } else {
                    candidate.provider_id.as_str()
                };

                candidate_provider == provider_id && candidate.model_id == model_id
            })?;

            Some((route_index, agent_id.to_owned()))
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|(route_index, _)| *route_index);

    Ok(candidates
        .into_iter()
        .map(|(_, agent_id)| agent_id)
        .collect())
}

pub(crate) fn execution_agent_candidates(capability: &str) -> Result<Vec<String>, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Unable to determine OpenClaw configuration location".to_owned())?;

    let contents = std::fs::read_to_string(home.join(".openclaw/openclaw.json"))
        .map_err(|_| "Unable to read OpenClaw execution-agent configuration".to_owned())?;

    let config: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|_| "OpenClaw execution-agent configuration is malformed".to_owned())?;

    let (_, route_candidates) = route_candidates()?;

    execution_agent_candidates_from_config(capability, &config, &route_candidates)
}

fn auto_route_candidates(
    instances: &[ProviderInstance],
    local_model_ids: &[String],
) -> Vec<AiCenterRouteCandidate> {
    let mut candidates = local_model_ids
        .iter()
        .map(|model_id| AiCenterRouteCandidate {
            provider_id: "ollama".to_owned(),
            provider_instance_id: "ollama-local".to_owned(),
            model_id: model_id.clone(),
        })
        .collect::<Vec<_>>();

    for instance in instances
        .iter()
        .filter(|instance| instance.connection_state == ProviderConnectionState::Connected)
    {
        let mut enabled = instance
            .models
            .iter()
            .filter(|model| model.enabled)
            .collect::<Vec<_>>();
        enabled.sort_by_key(|model| !model.is_default);

        candidates.extend(enabled.into_iter().map(|model| AiCenterRouteCandidate {
            provider_id: instance.provider_id.clone(),
            provider_instance_id: instance.id.clone(),
            model_id: model.remote_model_id.clone(),
        }));
    }

    candidates
}

fn execution_source(candidate: &AiCenterRouteCandidate) -> AiCenterExecutionSource {
    if candidate.provider_id == "ollama" || candidate.provider_instance_id == "ollama-local" {
        AiCenterExecutionSource::Local
    } else {
        AiCenterExecutionSource::Cloud
    }
}

fn estimate_tokens(value: &str) -> usize {
    value.encode_utf16().count().div_ceil(4)
}

fn safe_attempt_error(message: &str) -> String {
    let message = message.to_lowercase();
    if message.contains("cancel") {
        "cancelled"
    } else if ["401", "403", "reconnect", "credential", "auth"]
        .iter()
        .any(|value| message.contains(value))
    {
        "authentication"
    } else if ["429", "rate limit", "busy"]
        .iter()
        .any(|value| message.contains(value))
    {
        "rate-limited"
    } else if message.contains("timeout") {
        "timeout"
    } else if message.contains("no text") || message.contains("empty") {
        "empty-response"
    } else if ["connect", "reach", "network", "stream was interrupted"]
        .iter()
        .any(|value| message.contains(value))
    {
        "unavailable"
    } else {
        "provider-error"
    }
    .to_owned()
}

fn complete_attempt(
    candidate: &AiCenterRouteCandidate,
    started_at: String,
    started: Instant,
    outcome: AiCenterAttemptOutcome,
    error: Option<&str>,
) -> AiCenterAttempt {
    AiCenterAttempt {
        provider_id: candidate.provider_id.clone(),
        provider_instance_id: candidate.provider_instance_id.clone(),
        model_id: candidate.model_id.clone(),
        source: execution_source(candidate),
        started_at,
        completed_at: chrono::Utc::now().to_rfc3339(),
        latency_ms: started.elapsed().as_millis() as u64,
        outcome,
        error_category: error.map(safe_attempt_error),
    }
}

fn canonical_metadata(
    invocation_id: String,
    route_mode: AiCenterRouteMode,
    candidate: &AiCenterRouteCandidate,
    started_at: String,
    started: Instant,
    input: &str,
    output: &str,
    attempts: Vec<AiCenterAttempt>,
) -> AiCenterCanonicalInvocationMetadata {
    AiCenterCanonicalInvocationMetadata {
        invocation_id,
        route_mode,
        provider_id: candidate.provider_id.clone(),
        provider_instance_id: candidate.provider_instance_id.clone(),
        model_id: candidate.model_id.clone(),
        source: execution_source(candidate),
        started_at,
        completed_at: chrono::Utc::now().to_rfc3339(),
        latency_ms: started.elapsed().as_millis() as u64,
        input_tokens: estimate_tokens(input),
        output_tokens: estimate_tokens(output),
        token_accuracy: "estimated",
        fallback_occurred: attempts.len() > 1,
        attempts,
    }
}

fn route_candidates() -> Result<(Vec<ProviderInstance>, Vec<AiCenterRouteCandidate>), String> {
    let instances = read_provider_instances()?;
    let local_model_ids = crate::models::list_ollama_models()
        .unwrap_or_default()
        .into_iter()
        .map(|model| model.model)
        .collect::<Vec<_>>();
    let candidates = auto_route_candidates(&instances, &local_model_ids);
    Ok((instances, candidates))
}

fn select_route_candidate(
    input: &ResolveAiCenterRouteInput,
    candidates: &[AiCenterRouteCandidate],
) -> Result<Option<AiCenterRouteCandidate>, String> {
    if input.cancelled || input.emitted_output {
        return Ok(None);
    }

    match input.route_mode {
        AiCenterRouteMode::Auto => Ok(candidates.get(input.attempted_count).cloned()),
        AiCenterRouteMode::Manual if input.attempted_count > 0 => Ok(None),
        AiCenterRouteMode::Manual => {
            let candidate = input
                .manual_candidate
                .as_ref()
                .ok_or_else(|| "NO_DEFAULT_MODEL".to_owned())?;
            if !candidates.contains(candidate) {
                return Err("NO_CONNECTED_PROVIDER".to_owned());
            }
            Ok(Some(candidate.clone()))
        }
    }
}

#[tauri::command]
pub(crate) fn resolve_ai_center_route(
    input: ResolveAiCenterRouteInput,
) -> Result<Option<AiCenterRouteCandidate>, String> {
    let (_, candidates) = route_candidates()?;
    select_route_candidate(&input, &candidates)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamProviderResponseInput {
    operation_id: String,
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
    messages: Vec<ProviderChatMessage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecuteAiCenterStreamInput {
    operation_id: String,
    route_mode: AiCenterRouteMode,
    manual_candidate: Option<AiCenterRouteCandidate>,
    messages: Vec<ProviderChatMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiCenterChunkEvent {
    operation_id: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiCenterDoneEvent {
    operation_id: String,
    cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<AiCenterCanonicalInvocationMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiCenterErrorEvent {
    operation_id: String,
    message: String,
}

#[derive(Debug, Clone, Copy)]
enum AuthStyle {
    Bearer,
    Anthropic,
    Google,
    None,
}

#[derive(Debug, Clone, Copy)]
struct ProviderAdapterSpec {
    id: &'static str,
    models_url: &'static str,
    auth: AuthStyle,
}

#[derive(Debug, Clone)]
struct ProviderAdapterRegistration {
    descriptor: ProviderAdapterDescriptor,
    native: Option<ProviderAdapterSpec>,
}

fn provider_adapter(
    provider_id: &str,
    display_name: &str,
    adapter_kind: ProviderAdapterKind,
    credential_kinds: &[ProviderCredentialKind],
    capabilities: &[&str],
    supports_model_discovery: bool,
    supports_token_refresh: bool,
    native: Option<ProviderAdapterSpec>,
) -> ProviderAdapterRegistration {
    ProviderAdapterRegistration {
        descriptor: ProviderAdapterDescriptor {
            provider_id: provider_id.to_owned(),
            display_name: display_name.to_owned(),
            adapter_kind,
            credential_kinds: credential_kinds.to_vec(),
            authentication_methods: credential_kinds
                .iter()
                .flat_map(|credential_kind| match credential_kind {
                    ProviderCredentialKind::ApiKey => vec![ProviderAuthenticationMethod::ApiKey],
                    ProviderCredentialKind::Local => vec![ProviderAuthenticationMethod::Local],
                    ProviderCredentialKind::OAuth => vec![
                        ProviderAuthenticationMethod::OAuthPkce,
                        ProviderAuthenticationMethod::OAuthLoopback,
                    ],
                })
                .collect(),
            capabilities: capabilities
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
            supports_model_discovery,
            supports_token_refresh,
            supports_multiple_credentials: false,
        },
        native,
    }
}

fn provider_adapter_registry() -> Vec<ProviderAdapterRegistration> {
    let mut registrations = vec![
        provider_adapter(
            "openai",
            "OpenAI",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            true,
            Some(ProviderAdapterSpec {
                id: "openai",
                models_url: "https://api.openai.com/v1/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "anthropic",
            "Anthropic",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::ApiKey,
                ProviderCredentialKind::Local,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "anthropic",
                models_url: "https://api.anthropic.com/v1/models",
                auth: AuthStyle::Anthropic,
            }),
        ),
        provider_adapter(
            "google",
            "Google",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            true,
            Some(ProviderAdapterSpec {
                id: "google",
                models_url: "https://generativelanguage.googleapis.com/v1beta/models",
                auth: AuthStyle::Google,
            }),
        ),
        provider_adapter(
            "grok",
            "xAI",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            true,
            Some(ProviderAdapterSpec {
                id: "grok",
                models_url: "https://api.x.ai/v1/language-models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "deepseek",
            "DeepSeek",
            ProviderAdapterKind::Native,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "reasoning", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "deepseek",
                models_url: "https://api.deepseek.com/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "openrouter",
            "OpenRouter",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "openrouter",
                models_url: "https://openrouter.ai/api/v1/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "doubao",
            "Doubao",
            ProviderAdapterKind::Catalog,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "tool-use"],
            true,
            false,
            None,
        ),
        provider_adapter(
            "kimi",
            "Kimi Code",
            ProviderAdapterKind::Native,
            &[
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            true,
            Some(ProviderAdapterSpec {
                id: "kimi",
                models_url: "https://api.kimi.com/coding/v1/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "meta",
            "Meta Model API",
            ProviderAdapterKind::Native,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "meta",
                models_url: "https://api.meta.ai/v1/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "compatible",
            "Other AI",
            ProviderAdapterKind::Catalog,
            &[ProviderCredentialKind::ApiKey],
            &["chat"],
            false,
            false,
            None,
        ),
        provider_adapter(
            "ollama",
            "Ollama",
            ProviderAdapterKind::Native,
            &[ProviderCredentialKind::Local],
            &["chat", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "ollama",
                models_url: "http://127.0.0.1:11434/api/tags",
                auth: AuthStyle::None,
            }),
        ),
    ];

    if let Some(anthropic) = registrations
        .iter_mut()
        .find(|registration| registration.descriptor.provider_id == "anthropic")
    {
        anthropic
            .descriptor
            .authentication_methods
            .retain(|method| *method != ProviderAuthenticationMethod::Local);
        anthropic
            .descriptor
            .authentication_methods
            .push(ProviderAuthenticationMethod::CliAccount);
    }

    if let Some(grok) = registrations
        .iter_mut()
        .find(|registration| registration.descriptor.provider_id == "grok")
    {
        grok.descriptor.authentication_methods.retain(|method| {
            !matches!(
                method,
                ProviderAuthenticationMethod::OAuthPkce
                    | ProviderAuthenticationMethod::OAuthLoopback
            )
        });
        grok.descriptor
            .authentication_methods
            .insert(0, ProviderAuthenticationMethod::DeviceCode);
    }

    if let Some(kimi) = registrations
        .iter_mut()
        .find(|registration| registration.descriptor.provider_id == "kimi")
    {
        kimi.descriptor.authentication_methods.retain(|method| {
            !matches!(
                method,
                ProviderAuthenticationMethod::OAuthPkce
                    | ProviderAuthenticationMethod::OAuthLoopback
            )
        });
        kimi.descriptor
            .authentication_methods
            .insert(0, ProviderAuthenticationMethod::DeviceCode);
    }

    registrations
}

fn find_provider_adapter(provider_id: &str) -> Option<ProviderAdapterRegistration> {
    let provider_id = provider_id.trim();

    if provider_id.is_empty() {
        return None;
    }

    provider_adapter_registry()
        .into_iter()
        .find(|registration| registration.descriptor.provider_id == provider_id)
}

fn adapter_spec(provider_id: &str) -> Result<ProviderAdapterSpec, String> {
    find_provider_adapter(provider_id)
        .and_then(|registration| registration.native)
        .ok_or_else(|| "this Provider does not have a native AI-OS Adapter yet".to_owned())
}

#[tauri::command]
pub(crate) fn list_provider_adapters() -> Vec<ProviderAdapterDescriptor> {
    provider_adapter_registry()
        .into_iter()
        .map(|registration| registration.descriptor)
        .collect()
}

#[tauri::command]
pub(crate) fn get_provider_adapter(
    provider_id: String,
) -> Result<ProviderAdapterDescriptor, String> {
    find_provider_adapter(&provider_id)
        .map(|registration| registration.descriptor)
        .ok_or_else(|| format!("Provider Adapter was not found: {}", provider_id.trim()))
}

fn validate_instance_id(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err("provider instance id is invalid".to_owned());
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err("provider instance id contains unsupported characters".to_owned());
    }
    Ok(value)
}

#[cfg(target_os = "macos")]
fn store_secret(account: &str, secret: &[u8]) -> Result<(), String> {
    security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, account, secret)
        .map_err(|_| "macOS Keychain could not store the Provider credential".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn store_secret(_account: &str, _secret: &[u8]) -> Result<(), String> {
    Err("secure Provider credentials are not supported on this platform".to_owned())
}

#[cfg(target_os = "macos")]
fn secret_exists(account: &str) -> Result<bool, String> {
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account) {
        Ok(secret) => Ok(!secret.is_empty()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(false),
        Err(_) => Err("macOS Keychain credential status is unavailable".to_owned()),
    }
}

#[cfg(target_os = "macos")]
fn read_secret(account: &str) -> Result<Vec<u8>, String> {
    security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account)
        .map_err(|_| "Provider credential is missing from macOS Keychain".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn read_secret(_account: &str) -> Result<Vec<u8>, String> {
    Err("secure Provider credentials are not supported on this platform".to_owned())
}

async fn refresh_oauth_token(account: &str, token: &Value) -> Result<Value, String> {
    let refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Provider account sign-in must be renewed".to_owned())?;
    let metadata = token
        .get("_aios")
        .ok_or_else(|| "OAuth refresh metadata is missing".to_owned())?;
    let token_url = metadata
        .get("tokenUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth token URL is missing".to_owned())?;
    let client_id = metadata
        .get("clientId")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth client ID is missing".to_owned())?;
    let provider_id = metadata
        .get("providerId")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth Provider identity is missing".to_owned())?;
    let client_secret = read_oauth_client_secret(provider_id)?;
    validate_https_url(token_url, "OAuth token URL")?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    if let Some(secret) = client_secret.as_deref() {
        form.push(("client_secret", secret));
    }
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize OAuth refresh".to_owned())?
        .post(token_url)
        .form(&form)
        .send()
        .await
        .map_err(|_| "OAuth token service could not be reached".to_owned())?;
    if !response.status().is_success() {
        return Err("Provider account sign-in must be renewed".to_owned());
    }
    let refreshed: Value = response
        .json()
        .await
        .map_err(|_| "OAuth provider returned an invalid refresh token".to_owned())?;
    let merged = merge_refreshed_oauth_token(token, &refreshed)?;
    store_secret(
        account,
        &serde_json::to_vec(&merged)
            .map_err(|_| "AI-OS could not secure the refreshed OAuth token".to_owned())?,
    )?;
    Ok(merged)
}

fn merge_refreshed_oauth_token(token: &Value, refreshed: &Value) -> Result<Value, String> {
    if !refreshed
        .get("access_token")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("OAuth response did not contain an access token".to_owned());
    }

    let mut merged = token.clone();
    let object = merged
        .as_object_mut()
        .ok_or_else(|| "OAuth token record is invalid".to_owned())?;
    if let Some(refreshed_object) = refreshed.as_object() {
        for (key, value) in refreshed_object {
            if key == "_aios"
                || (key == "refresh_token"
                    && !value.as_str().is_some_and(|value| !value.trim().is_empty()))
            {
                continue;
            }
            object.insert(key.clone(), value.clone());
        }
    }
    let expires_at = refreshed
        .get("expires_in")
        .and_then(Value::as_i64)
        .map(|seconds| (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339());
    if let Some(metadata) = object.get_mut("_aios").and_then(Value::as_object_mut) {
        metadata.insert(
            "expiresAt".to_owned(),
            expires_at.map(Value::String).unwrap_or(Value::Null),
        );
    }
    Ok(merged)
}

fn oauth_credential_metadata(token: &Value) -> (Option<String>, bool) {
    let expires_at = token
        .pointer("/_aios/expiresAt")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let refreshable = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    (expires_at, refreshable)
}

#[tauri::command]
pub(crate) async fn refresh_provider_oauth(
    query: ProviderCredentialQuery,
) -> Result<RefreshOAuthResult, String> {
    let account = validate_instance_id(&query.provider_instance_id)?;
    let secret = read_secret(account)?;
    let token = serde_json::from_slice::<Value>(&secret)
        .map_err(|_| "Provider credential is not an OAuth token".to_owned())?;
    let refreshed = refresh_oauth_token(account, &token).await?;
    let (expires_at, refreshable) = oauth_credential_metadata(&refreshed);

    Ok(RefreshOAuthResult {
        provider_instance_id: account.to_owned(),
        expires_at,
        refreshable,
    })
}

#[tauri::command]
pub(crate) async fn begin_grok_device_auth(
    query: ProviderCredentialQuery,
) -> Result<BeginGrokDeviceAuthResult, String> {
    let instance_id = validate_instance_id(&query.provider_instance_id)?.to_owned();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize Grok sign-in".to_owned())?;
    let response = client
        .post(format!("{GROK_OAUTH_ISSUER}/oauth2/device/code"))
        .header("x-grok-client-surface", "ui")
        .header("x-grok-client-version", env!("CARGO_PKG_VERSION"))
        .form(&[
            ("client_id", GROK_OAUTH_CLIENT_ID),
            ("scope", GROK_OAUTH_SCOPES),
            ("referrer", "grok-build"),
        ])
        .send()
        .await
        .map_err(|_| "Grok sign-in service could not be reached".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "Grok sign-in could not start (HTTP {})",
            response.status().as_u16()
        ));
    }
    let device: GrokDeviceCodeResponse = response
        .json()
        .await
        .map_err(|_| "Grok returned an invalid sign-in response".to_owned())?;
    validate_https_url(&device.verification_uri, "Grok verification URL")?;
    if let Some(url) = device.verification_uri_complete.as_deref() {
        validate_https_url(url, "Grok verification URL")?;
    }
    if device.user_code.is_empty()
        || !device
            .user_code
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("Grok returned an invalid verification code".to_owned());
    }
    let state = uuid::Uuid::new_v4().simple().to_string();
    let expires_at = unix_timestamp_seconds()?
        .checked_add(device.expires_in)
        .ok_or_else(|| "Grok sign-in expiry is invalid".to_owned())?;
    grok_device_sessions()
        .lock()
        .map_err(|_| "Grok sign-in state is unavailable".to_owned())?
        .insert(
            state.clone(),
            GrokDeviceSession {
                provider_instance_id: instance_id,
                device_code: device.device_code,
                interval_seconds: device.interval.unwrap_or(5).max(1),
                expires_at,
                cancellation: CancellationToken::new(),
            },
        );
    Ok(BeginGrokDeviceAuthResult {
        state,
        verification_uri: device.verification_uri,
        verification_uri_complete: device.verification_uri_complete,
        user_code: device.user_code,
        expires_in: device.expires_in,
    })
}

#[tauri::command]
pub(crate) async fn complete_grok_device_auth(
    state: String,
) -> Result<CompleteOAuthResult, String> {
    let session = grok_device_sessions()
        .lock()
        .map_err(|_| "Grok sign-in state is unavailable".to_owned())?
        .get(state.trim())
        .cloned()
        .ok_or_else(|| "Grok sign-in is invalid or expired".to_owned())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize Grok sign-in".to_owned())?;
    let mut interval = Duration::from_secs(session.interval_seconds);
    loop {
        if unix_timestamp_seconds()? >= session.expires_at {
            break;
        }
        tokio::select! {
            _ = session.cancellation.cancelled() => {
                grok_device_sessions().lock().ok().map(|mut sessions| sessions.remove(state.trim()));
                return Err("Grok sign-in was cancelled".to_owned());
            }
            _ = tokio::time::sleep(interval) => {}
        }
        let response = client
            .post(format!("{GROK_OAUTH_ISSUER}/oauth2/token"))
            .header("x-grok-client-surface", "ui")
            .header("x-grok-client-version", env!("CARGO_PKG_VERSION"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", session.device_code.as_str()),
                ("client_id", GROK_OAUTH_CLIENT_ID),
            ])
            .send()
            .await
            .map_err(|_| "Grok sign-in service could not be reached".to_owned())?;
        if response.status().is_success() {
            let mut token: Value = response
                .json()
                .await
                .map_err(|_| "Grok returned an invalid token".to_owned())?;
            token
                .get("access_token")
                .and_then(Value::as_str)
                .ok_or_else(|| "Grok sign-in returned no access token".to_owned())?;
            let expires_at = token
                .get("expires_in")
                .and_then(Value::as_i64)
                .map(|seconds| {
                    (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
                });
            let account_id = token
                .get("access_token")
                .and_then(Value::as_str)
                .and_then(jwt_subject);
            token
                .as_object_mut()
                .ok_or_else(|| "Grok returned an invalid token".to_owned())?
                .insert(
                    "_aios".to_owned(),
                    serde_json::json!({
                        "providerId": "grok",
                        "clientId": GROK_OAUTH_CLIENT_ID,
                        "tokenUrl": format!("{GROK_OAUTH_ISSUER}/oauth2/token"),
                        "expiresAt": expires_at.clone(),
                        "routeKind": "grok-oauth",
                        "accountId": account_id,
                    }),
                );
            let refreshable = token.get("refresh_token").is_some();
            store_secret(
                &session.provider_instance_id,
                &serde_json::to_vec(&token)
                    .map_err(|_| "AI-OS could not secure the Grok token".to_owned())?,
            )?;
            grok_device_sessions()
                .lock()
                .ok()
                .map(|mut sessions| sessions.remove(state.trim()));
            return Ok(CompleteOAuthResult {
                provider_instance_id: session.provider_instance_id,
                expires_at,
                refreshable,
            });
        }
        let error: GrokDeviceTokenError = response
            .json()
            .await
            .map_err(|_| "Grok returned an invalid sign-in status".to_owned())?;
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            "access_denied" => {
                grok_device_sessions()
                    .lock()
                    .ok()
                    .map(|mut sessions| sessions.remove(state.trim()));
                return Err("Grok sign-in was denied".to_owned());
            }
            "expired_token" => break,
            _ => return Err("Grok token exchange failed".to_owned()),
        }
    }
    grok_device_sessions()
        .lock()
        .ok()
        .map(|mut sessions| sessions.remove(state.trim()));
    Err("Grok sign-in expired. Please try again.".to_owned())
}

#[tauri::command]
pub(crate) fn cancel_grok_device_auth(state: String) -> Result<bool, String> {
    let session = grok_device_sessions()
        .lock()
        .map_err(|_| "Grok sign-in state is unavailable".to_owned())?
        .remove(state.trim());
    if let Some(session) = session {
        session.cancellation.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub(crate) async fn begin_kimi_device_auth(
    query: ProviderCredentialQuery,
) -> Result<BeginGrokDeviceAuthResult, String> {
    let instance_id = validate_instance_id(&query.provider_instance_id)?.to_owned();
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize Kimi Code sign-in".to_owned())?
        .post(format!(
            "{KIMI_OAUTH_ISSUER}/api/oauth/device_authorization"
        ))
        .form(&[("client_id", KIMI_OAUTH_CLIENT_ID)])
        .send()
        .await
        .map_err(|_| "Kimi Code sign-in service could not be reached".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "Kimi Code sign-in could not start (HTTP {})",
            response.status().as_u16()
        ));
    }
    let device: GrokDeviceCodeResponse = response
        .json()
        .await
        .map_err(|_| "Kimi Code returned an invalid sign-in response".to_owned())?;
    validate_https_url(&device.verification_uri, "Kimi Code verification URL")?;
    if let Some(url) = device.verification_uri_complete.as_deref() {
        validate_https_url(url, "Kimi Code verification URL")?;
    }
    let state = uuid::Uuid::new_v4().simple().to_string();
    let expires_at = unix_timestamp_seconds()?
        .checked_add(device.expires_in)
        .ok_or_else(|| "Kimi Code sign-in expiry is invalid".to_owned())?;
    kimi_device_sessions()
        .lock()
        .map_err(|_| "Kimi Code sign-in state is unavailable".to_owned())?
        .insert(
            state.clone(),
            GrokDeviceSession {
                provider_instance_id: instance_id,
                device_code: device.device_code,
                interval_seconds: device.interval.unwrap_or(5).max(1),
                expires_at,
                cancellation: CancellationToken::new(),
            },
        );
    Ok(BeginGrokDeviceAuthResult {
        state,
        verification_uri: device.verification_uri,
        verification_uri_complete: device.verification_uri_complete,
        user_code: device.user_code,
        expires_in: device.expires_in,
    })
}

#[tauri::command]
pub(crate) async fn complete_kimi_device_auth(
    state: String,
) -> Result<CompleteOAuthResult, String> {
    let session = kimi_device_sessions()
        .lock()
        .map_err(|_| "Kimi Code sign-in state is unavailable".to_owned())?
        .get(state.trim())
        .cloned()
        .ok_or_else(|| "Kimi Code sign-in is invalid or expired".to_owned())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize Kimi Code sign-in".to_owned())?;
    let mut interval = Duration::from_secs(session.interval_seconds);
    loop {
        if unix_timestamp_seconds()? >= session.expires_at {
            break;
        }
        tokio::select! {
            _ = session.cancellation.cancelled() => {
                kimi_device_sessions().lock().ok().map(|mut sessions| sessions.remove(state.trim()));
                return Err("Kimi Code sign-in was cancelled".to_owned());
            }
            _ = tokio::time::sleep(interval) => {}
        }
        let response = client
            .post(format!("{KIMI_OAUTH_ISSUER}/api/oauth/token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", session.device_code.as_str()),
                ("client_id", KIMI_OAUTH_CLIENT_ID),
            ])
            .send()
            .await
            .map_err(|_| "Kimi Code sign-in service could not be reached".to_owned())?;
        if response.status().is_success() {
            let mut token: Value = response
                .json()
                .await
                .map_err(|_| "Kimi Code returned an invalid token".to_owned())?;
            token
                .get("access_token")
                .and_then(Value::as_str)
                .ok_or_else(|| "Kimi Code sign-in returned no access token".to_owned())?;
            let expires_at = token
                .get("expires_in")
                .and_then(Value::as_i64)
                .map(|seconds| {
                    (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
                });
            token
                .as_object_mut()
                .ok_or_else(|| "Kimi Code returned an invalid token".to_owned())?
                .insert(
                    "_aios".to_owned(),
                    serde_json::json!({
                        "providerId": "kimi",
                        "clientId": KIMI_OAUTH_CLIENT_ID,
                        "tokenUrl": format!("{KIMI_OAUTH_ISSUER}/api/oauth/token"),
                        "expiresAt": expires_at.clone(),
                        "routeKind": "kimi-code-oauth"
                    }),
                );
            let refreshable = token.get("refresh_token").is_some();
            store_secret(
                &session.provider_instance_id,
                &serde_json::to_vec(&token)
                    .map_err(|_| "AI-OS could not secure the Kimi Code token".to_owned())?,
            )?;
            kimi_device_sessions()
                .lock()
                .ok()
                .map(|mut sessions| sessions.remove(state.trim()));
            return Ok(CompleteOAuthResult {
                provider_instance_id: session.provider_instance_id,
                expires_at,
                refreshable,
            });
        }
        let error: GrokDeviceTokenError = response
            .json()
            .await
            .map_err(|_| "Kimi Code returned an invalid sign-in status".to_owned())?;
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            "access_denied" => return Err("Kimi Code sign-in was denied".to_owned()),
            "expired_token" => break,
            _ => return Err("Kimi Code token exchange failed".to_owned()),
        }
    }
    kimi_device_sessions()
        .lock()
        .ok()
        .map(|mut sessions| sessions.remove(state.trim()));
    Err("Kimi Code sign-in expired. Please try again.".to_owned())
}

#[tauri::command]
pub(crate) fn cancel_kimi_device_auth(state: String) -> Result<bool, String> {
    let session = kimi_device_sessions()
        .lock()
        .map_err(|_| "Kimi Code sign-in state is unavailable".to_owned())?
        .remove(state.trim());
    if let Some(session) = session {
        session.cancellation.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

struct CurrentProviderCredential {
    value: String,
    oauth: bool,
    resource_project_id: Option<String>,
    route_kind: Option<String>,
    account_id: Option<String>,
}

async fn read_current_credential(account: &str) -> Result<CurrentProviderCredential, String> {
    let secret = read_secret(account)?;
    let Ok(mut token) = serde_json::from_slice::<Value>(&secret) else {
        return String::from_utf8(secret)
            .map(|value| CurrentProviderCredential {
                value,
                oauth: false,
                resource_project_id: None,
                route_kind: None,
                account_id: None,
            })
            .map_err(|_| "Provider credential in Keychain is invalid".to_owned());
    };
    let should_refresh = token
        .pointer("/_aios/expiresAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|expires_at| {
            expires_at.with_timezone(&chrono::Utc)
                <= chrono::Utc::now() + chrono::Duration::seconds(60)
        })
        .unwrap_or(false);
    if should_refresh {
        token = refresh_oauth_token(account, &token).await?;
    }
    let value = token
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "OAuth response did not contain an access token".to_owned())?;
    let resource_project_id = token
        .pointer("/_aios/resourceProjectId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let route_kind = token
        .pointer("/_aios/routeKind")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let account_id = token
        .pointer("/_aios/accountId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(CurrentProviderCredential {
        value,
        oauth: true,
        resource_project_id,
        route_kind,
        account_id,
    })
}

fn authenticate_provider_request(
    request: reqwest::RequestBuilder,
    auth: AuthStyle,
    credential: CurrentProviderCredential,
) -> reqwest::RequestBuilder {
    match auth {
        AuthStyle::Bearer => {
            let request = request.bearer_auth(credential.value);
            if credential.route_kind.as_deref() == Some("openai-codex") {
                let request = request
                    .header("OpenAI-Beta", "codex-1")
                    .header("originator", "ai-os");
                if let Some(account_id) = credential.account_id {
                    request.header("ChatGPT-Account-ID", account_id)
                } else {
                    request
                }
            } else if credential.route_kind.as_deref() == Some("grok-oauth") {
                let request = request
                    .header("X-XAI-Token-Auth", "xai-grok-cli")
                    .header("x-grok-client-version", env!("CARGO_PKG_VERSION"))
                    .header("x-grok-client-mode", "ui");
                if let Some(account_id) = credential.account_id {
                    request.header("x-userid", account_id)
                } else {
                    request
                }
            } else {
                request
            }
        }
        AuthStyle::Anthropic => request
            .header("x-api-key", credential.value)
            .header("anthropic-version", "2023-06-01"),
        AuthStyle::Google if credential.oauth => {
            let request = request.bearer_auth(credential.value);
            if let Some(project_id) = credential.resource_project_id {
                request.header("x-goog-user-project", project_id)
            } else {
                request
            }
        }
        AuthStyle::Google => request.header("x-goog-api-key", credential.value),
        AuthStyle::None => request,
    }
}

#[cfg(not(target_os = "macos"))]
fn secret_exists(_account: &str) -> Result<bool, String> {
    Err("secure Provider credentials are not supported on this platform".to_owned())
}

#[cfg(target_os = "macos")]
fn remove_secret(account: &str) -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(_) => Err("macOS Keychain could not remove the Provider credential".to_owned()),
    }
}

#[cfg(not(target_os = "macos"))]
fn remove_secret(_account: &str) -> Result<(), String> {
    Err("secure Provider credentials are not supported on this platform".to_owned())
}

#[tauri::command]
pub(crate) fn set_provider_credential(
    input: ProviderCredentialInput,
) -> Result<ProviderCredentialStatus, String> {
    let account = validate_instance_id(&input.provider_instance_id)?;
    if input.secret.trim().is_empty() {
        return Err("provider credential must not be empty".to_owned());
    }

    store_secret(account, input.secret.as_bytes())?;
    Ok(ProviderCredentialStatus {
        provider_instance_id: account.to_owned(),
        has_credential: true,
    })
}

#[tauri::command]
pub(crate) fn get_provider_credential_status(
    query: ProviderCredentialQuery,
) -> Result<ProviderCredentialStatus, String> {
    let account = validate_instance_id(&query.provider_instance_id)?;
    Ok(ProviderCredentialStatus {
        provider_instance_id: account.to_owned(),
        has_credential: secret_exists(account)?,
    })
}

#[tauri::command]
pub(crate) fn delete_provider_credential(
    query: ProviderCredentialQuery,
) -> Result<ProviderCredentialStatus, String> {
    let account = validate_instance_id(&query.provider_instance_id)?;
    remove_secret(account)?;
    Ok(ProviderCredentialStatus {
        provider_instance_id: account.to_owned(),
        has_credential: false,
    })
}

fn parse_models(provider_id: &str, body: Value) -> Result<Vec<DiscoveredProviderModel>, String> {
    let items = body
        .get(if matches!(provider_id, "ollama" | "google") {
            "models"
        } else {
            "data"
        })
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Provider returned an invalid model list".to_owned())?;

    let mut models: Vec<DiscoveredProviderModel> = items
        .iter()
        .filter_map(|item| {
            let id = if provider_id == "ollama" {
                item.get("model")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str)
            } else if provider_id == "google" {
                item.get("name")
                    .and_then(Value::as_str)
                    .map(|name| name.strip_prefix("models/").unwrap_or(name))
            } else {
                item.get("id")
                    .or_else(|| item.get("slug"))
                    .and_then(Value::as_str)
            }?;
            let display_name = item
                .get("display_name")
                .or_else(|| item.get("displayName"))
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(id);
            Some(DiscoveredProviderModel {
                id: id.to_owned(),
                display_name: display_name.to_owned(),
            })
        })
        .collect();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

async fn discover_models(
    query: &ProviderAdapterQuery,
) -> Result<Vec<DiscoveredProviderModel>, String> {
    let instance_id = validate_instance_id(&query.provider_instance_id)?;
    let spec = adapter_spec(query.provider_id.trim())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "AI-OS could not initialize the Provider connection".to_owned())?;
    let credential = if !matches!(spec.auth, AuthStyle::None) {
        Some(read_current_credential(instance_id).await?)
    } else {
        None
    };
    let route_kind = credential
        .as_ref()
        .and_then(|credential| credential.route_kind.as_deref())
        .unwrap_or_default();
    let models_url = match route_kind {
        "openai-codex" => "https://chatgpt.com/backend-api/codex/models?client_version=1.0.0",
        "grok-oauth" => "https://cli-chat-proxy.grok.com/v1/models",
        _ => spec.models_url,
    };
    let mut request = client.get(models_url);

    if let Some(credential) = credential {
        request = authenticate_provider_request(request, spec.auth, credential);
    }

    let response = request
        .send()
        .await
        .map_err(|_| format!("{} could not be reached", spec.id))?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Provider rejected this credential".to_owned(),
            429 => "Provider rate limit reached; try again shortly".to_owned(),
            _ => format!("Provider connection failed with status {}", status.as_u16()),
        });
    }
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "Provider returned an unreadable model list".to_owned())?;
    let models = parse_models(spec.id, body)?;
    if models.is_empty() {
        return Err("Provider returned no usable models".to_owned());
    }
    Ok(models)
}

fn oauth_loopback_html(title: &str, message: &str) -> String {
    format!(
        "<!doctype html>\
<html lang=\"en\">\
<head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{title}</title>\
<style>\
body{{font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#f6f7f9;color:#18181b;display:grid;place-items:center;min-height:100vh;margin:0}}\
main{{max-width:520px;padding:32px;border:1px solid #ddd;border-radius:18px;background:white;box-shadow:0 12px 40px rgba(0,0,0,.08)}}\
h1{{font-size:22px;margin:0 0 12px}}\
p{{line-height:1.55;margin:0;color:#52525b}}\
</style>\
</head>\
<body><main><h1>{title}</h1><p>{message}</p></main></body>\
</html>"
    )
}

async fn write_oauth_loopback_response(
    stream: &mut TcpStream,
    status: &str,
    title: &str,
    message: &str,
) -> Result<(), String> {
    let body = oauth_loopback_html(title, message);
    let response = format!(
        "HTTP/1.1 {status}\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\n\
Cache-Control: no-store\r\n\
Pragma: no-cache\r\n\
Connection: close\r\n\
X-Content-Type-Options: nosniff\r\n\
Referrer-Policy: no-referrer\r\n\
\r\n{}",
        body.len(),
        body
    );

    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| "OAuth callback response could not be written".to_owned())?;

    let _ = stream.shutdown().await;
    Ok(())
}

async fn read_oauth_loopback_request(stream: &mut TcpStream) -> Result<String, String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];

    loop {
        let bytes_read = stream
            .read(&mut buffer)
            .await
            .map_err(|_| "OAuth callback request could not be read".to_owned())?;

        if bytes_read == 0 {
            break;
        }

        request.extend_from_slice(&buffer[..bytes_read]);

        if request.len() > OAUTH_LOOPBACK_MAX_REQUEST_BYTES {
            return Err("OAuth callback request is too large".to_owned());
        }

        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(request).map_err(|_| "OAuth callback request is invalid".to_owned())
}

fn parse_oauth_loopback_request(request: &str, expected_state: &str) -> Result<String, String> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| "OAuth callback request is incomplete".to_owned())?;

    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "OAuth callback request is incomplete".to_owned())?;
    let target = parts
        .next()
        .ok_or_else(|| "OAuth callback request is incomplete".to_owned())?;
    let version = parts
        .next()
        .ok_or_else(|| "OAuth callback request is incomplete".to_owned())?;

    if method != "GET" {
        return Err("OAuth callback must use GET".to_owned());
    }

    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err("OAuth callback uses an unsupported HTTP version".to_owned());
    }

    let callback_url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "OAuth callback URL is invalid".to_owned())?;

    if !matches!(callback_url.path(), OAUTH_LOOPBACK_PATH | "/auth/callback") {
        return Err("OAuth callback path is invalid".to_owned());
    }

    let mut callback_state: Option<String> = None;
    let mut code: Option<String> = None;
    let mut oauth_error: Option<String> = None;

    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "state" if callback_state.is_none() => {
                callback_state = Some(value.into_owned());
            }
            "code" if code.is_none() => {
                code = Some(value.into_owned());
            }
            "error" if oauth_error.is_none() => {
                oauth_error = Some(value.into_owned());
            }
            _ => {}
        }
    }

    let callback_state =
        callback_state.ok_or_else(|| "OAuth callback did not contain state".to_owned())?;

    if callback_state != expected_state {
        return Err("OAuth callback state does not match".to_owned());
    }

    if oauth_error.is_some() {
        return Err("OAuth authorization was not completed".to_owned());
    }

    let code = code.ok_or_else(|| "OAuth callback did not contain a code".to_owned())?;

    if code.trim().is_empty() || code.len() > 4096 {
        return Err("OAuth authorization code is invalid".to_owned());
    }

    Ok(code)
}

async fn complete_oauth_exchange(
    provider_id: &str,
    state: &str,
    code: &str,
) -> Result<CompleteOAuthResult, String> {
    adapter_spec(provider_id)?;

    if code.trim().is_empty() || state.trim().is_empty() {
        return Err("OAuth callback is incomplete".to_owned());
    }

    let session = consume_oauth_session(provider_id, state)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize the OAuth connection".to_owned())?;

    let response = if provider_id == "openrouter" {
        client
            .post(&session.token_url)
            .json(&serde_json::json!({
                "code": code.trim(),
                "code_verifier": session.verifier.as_str(),
                "code_challenge_method": "S256"
            }))
            .send()
            .await
    } else {
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("client_id", session.client_id.as_str()),
            ("code", code.trim()),
            ("redirect_uri", session.redirect_uri.as_str()),
            ("code_verifier", session.verifier.as_str()),
        ];
        if let Some(secret) = session.client_secret.as_deref() {
            form.push(("client_secret", secret));
        }
        client.post(&session.token_url).form(&form).send().await
    }
    .map_err(|_| "OAuth token service could not be reached".to_owned())?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let payload = response.json::<Value>().await.ok();
        return Err(oauth_token_rejection(status, payload.as_ref()));
    }

    let mut token: Value = response
        .json()
        .await
        .map_err(|_| "OAuth provider returned an invalid token".to_owned())?;

    if provider_id == "openrouter" {
        let key = token
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| "OpenRouter account connection returned no API key".to_owned())?
            .to_owned();
        let user_id = token.get("user_id").cloned().unwrap_or(Value::Null);
        token = serde_json::json!({
            "access_token": key,
            "token_type": "Bearer",
            "openrouter_user_id": user_id
        });
    }

    token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth response did not contain an access token".to_owned())?;

    let expires_at = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .map(|seconds| (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339());

    let account_id = token
        .get("id_token")
        .and_then(Value::as_str)
        .and_then(chatgpt_account_id_from_jwt);

    if session.route_kind.as_deref() == Some("openai-codex") && account_id.is_none() {
        return Err("OpenAI Codex sign-in did not identify a ChatGPT account".to_owned());
    }

    token
        .as_object_mut()
        .ok_or_else(|| "OAuth provider returned an invalid token".to_owned())?
        .insert(
            "_aios".to_owned(),
            serde_json::json!({
                "providerId": session.provider_id,
                "clientId": session.client_id,
                "tokenUrl": session.token_url,
                "expiresAt": expires_at.clone(),
                "resourceProjectId": session.resource_project_id,
                "routeKind": session.route_kind,
                "accountId": account_id,
            }),
        );

    let refreshable = token.get("refresh_token").is_some();

    let stored = serde_json::to_vec(&token)
        .map_err(|_| "AI-OS could not secure the OAuth token".to_owned())?;

    store_secret(&session.provider_instance_id, &stored)?;

    Ok(CompleteOAuthResult {
        provider_instance_id: session.provider_instance_id,
        expires_at,
        refreshable,
    })
}

fn chatgpt_account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .or_else(|| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .map(str::to_owned)
}

fn jwt_subject(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("sub")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .map(str::to_owned)
}

fn oauth_token_rejection(status: u16, payload: Option<&Value>) -> String {
    let error_code = payload
        .and_then(|value| value.get("error"))
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        });

    match error_code {
        Some(code) => format!("OAuth provider rejected the authorization code ({code})"),
        None => format!("OAuth provider rejected the authorization code (HTTP {status})"),
    }
}

async fn run_oauth_loopback_listener(
    app: AppHandle,
    listener: TcpListener,
    provider_id: String,
    provider_instance_id: String,
    expected_state: String,
    cancellation: CancellationToken,
) {
    let listener_future = async {
        for _ in 0..OAUTH_LOOPBACK_MAX_CONNECTIONS {
            let (mut stream, peer) = listener
                .accept()
                .await
                .map_err(|_| "OAuth callback listener failed".to_owned())?;

            if !peer.ip().is_loopback() {
                let _ = write_oauth_loopback_response(
                    &mut stream,
                    "403 Forbidden",
                    "Connection rejected",
                    "AI-OS only accepts OAuth callbacks from this computer.",
                )
                .await;
                continue;
            }

            let request = match read_oauth_loopback_request(&mut stream).await {
                Ok(request) => request,
                Err(message) => {
                    let _ = write_oauth_loopback_response(
                        &mut stream,
                        "400 Bad Request",
                        "Connection failed",
                        "The OAuth callback request was invalid.",
                    )
                    .await;
                    return Err(message);
                }
            };

            let code = match parse_oauth_loopback_request(&request, &expected_state) {
                Ok(code) => code,
                Err(message) => {
                    let _ = write_oauth_loopback_response(
                        &mut stream,
                        "400 Bad Request",
                        "Connection failed",
                        "The OAuth callback could not be verified.",
                    )
                    .await;

                    return Err(message);
                }
            };

            match complete_oauth_exchange(&provider_id, &expected_state, &code).await {
                Ok(result) => {
                    let _ = write_oauth_loopback_response(
                        &mut stream,
                        "200 OK",
                        "Account connected",
                        "You can close this browser window and return to AI-OS.",
                    )
                    .await;

                    let _ = app.emit(
                        "provider-oauth://completed",
                        ProviderOAuthCompletedEvent {
                            provider_id: provider_id.clone(),
                            provider_instance_id: result.provider_instance_id,
                            state: expected_state.clone(),
                            expires_at: result.expires_at,
                            refreshable: result.refreshable,
                        },
                    );

                    return Ok(());
                }
                Err(message) => {
                    let _ = write_oauth_loopback_response(
                        &mut stream,
                        "400 Bad Request",
                        "Connection failed",
                        &message,
                    )
                    .await;

                    return Err(message);
                }
            }
        }

        Err("OAuth callback connection limit was reached".to_owned())
    };

    let result = tokio::select! {
        _ = cancellation.cancelled() => {
            return;
        }
        result = tokio::time::timeout(
            Duration::from_secs(OAUTH_SESSION_TTL_SECONDS),
            listener_future,
        ) => result,
    };

    let _ = discard_oauth_session(&provider_id, &expected_state);

    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(message)) => message,
        Err(_) => "OAuth account sign-in timed out".to_owned(),
    };

    let _ = app.emit(
        "provider-oauth://error",
        ProviderOAuthErrorEvent {
            provider_id,
            provider_instance_id,
            state: expected_state,
            message: error,
        },
    );
}

fn validate_https_url(value: &str, label: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(value).map_err(|_| format!("{label} is invalid"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{label} must use HTTPS"));
    }
    Ok(parsed)
}

#[tauri::command]
pub(crate) async fn begin_provider_oauth(
    app: AppHandle,
    input: BeginOAuthInput,
) -> Result<BeginOAuthResult, String> {
    let instance_id = validate_instance_id(&input.provider_instance_id)?;
    let provider_id = input.provider_id.trim();
    adapter_spec(provider_id)?;

    if input.client_id.trim().is_empty() {
        return Err("OAuth client ID is not configured".to_owned());
    }

    let mut authorization_url =
        validate_https_url(&input.authorization_url, "OAuth authorization URL")?;

    validate_https_url(&input.token_url, "OAuth token URL")?;

    let callback_port = input.callback_port.unwrap_or(0);
    let callback_path = input
        .callback_path
        .as_deref()
        .unwrap_or(OAUTH_LOOPBACK_PATH)
        .trim();
    if !callback_path.starts_with('/')
        || callback_path.len() > 128
        || callback_path.contains(['?', '#'])
    {
        return Err("OAuth callback path is invalid".to_owned());
    }

    let listener = TcpListener::bind(("127.0.0.1", callback_port))
        .await
        .map_err(|_| "AI-OS could not open a local OAuth callback listener".to_owned())?;

    let address = listener
        .local_addr()
        .map_err(|_| "AI-OS could not determine the OAuth callback address".to_owned())?;

    let callback_host = if provider_id == "openai" {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let redirect_uri = format!("http://{callback_host}:{}{}", address.port(), callback_path);

    let state = uuid::Uuid::new_v4().simple().to_string();
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );

    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    let created_at = unix_timestamp_seconds()?;
    let expires_at = created_at
        .checked_add(OAUTH_SESSION_TTL_SECONDS)
        .ok_or_else(|| "OAuth session expiry is invalid".to_owned())?;

    let session = OAuthSession {
        provider_id: provider_id.to_owned(),
        provider_instance_id: instance_id.to_owned(),
        client_id: input.client_id.trim().to_owned(),
        client_secret: read_oauth_client_secret(provider_id)?,
        token_url: input.token_url,
        resource_project_id: input.resource_project_id,
        route_kind: match provider_id {
            "openai" => Some("openai-codex".to_owned()),
            "openrouter" => Some("openrouter-oauth".to_owned()),
            _ => None,
        },
        redirect_uri: redirect_uri.clone(),
        verifier,
        created_at,
        expires_at,
    };

    store_oauth_session(&state, session.clone())?;

    let cancellation = CancellationToken::new();
    let cancellation_key = oauth_session_key(provider_id, &state);
    if oauth_cancellations()
        .lock()
        .map_err(|_| "OAuth cancellation state is unavailable".to_owned())?
        .insert(cancellation_key, cancellation.clone())
        .is_some()
    {
        let _ = discard_oauth_session(provider_id, &state);
        return Err("OAuth cancellation session already exists".to_owned());
    }

    if provider_id == "openrouter" {
        let callback_url = format!("{}?state={state}", session.redirect_uri);
        authorization_url
            .query_pairs_mut()
            .append_pair("callback_url", &callback_url)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
    } else {
        authorization_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &session.client_id)
            .append_pair("redirect_uri", &session.redirect_uri)
            .append_pair("scope", &input.scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
    }
    if let Some(parameters) = input.authorization_params {
        for (key, value) in parameters {
            if !matches!(
                key.as_str(),
                "response_type"
                    | "client_id"
                    | "redirect_uri"
                    | "scope"
                    | "state"
                    | "code_challenge"
                    | "code_challenge_method"
            ) && !key.trim().is_empty()
                && key.len() <= 128
                && value.len() <= 1024
            {
                authorization_url
                    .query_pairs_mut()
                    .append_pair(&key, &value);
            }
        }
    }

    tauri::async_runtime::spawn(run_oauth_loopback_listener(
        app,
        listener,
        provider_id.to_owned(),
        session.provider_instance_id.clone(),
        state.clone(),
        cancellation,
    ));

    Ok(BeginOAuthResult {
        authorization_url: authorization_url.to_string(),
        redirect_uri,
        state,
    })
}

#[tauri::command]
pub(crate) fn cancel_provider_oauth(input: CancelOAuthInput) -> Result<bool, String> {
    let provider_id = input.provider_id.trim();
    adapter_spec(provider_id)?;
    if input.state.trim().is_empty() {
        return Err("OAuth state is required".to_owned());
    }

    discard_oauth_session(provider_id, input.state.trim())
}

#[tauri::command]
pub(crate) async fn complete_provider_oauth(
    input: CompleteOAuthInput,
) -> Result<CompleteOAuthResult, String> {
    complete_oauth_exchange(
        input.provider_id.trim(),
        input.state.trim(),
        input.code.trim(),
    )
    .await
}

#[tauri::command]
pub(crate) async fn discover_provider_models(
    query: ProviderAdapterQuery,
) -> Result<ProviderAdapterResult, String> {
    let models = discover_models(&query).await?;
    Ok(ProviderAdapterResult {
        provider_id: query.provider_id,
        level: "live".to_owned(),
        message: format!("Connection tested. {} models found.", models.len()),
        models,
    })
}

#[tauri::command]
pub(crate) async fn test_provider_connection(
    query: ProviderAdapterQuery,
) -> Result<ProviderAdapterResult, String> {
    discover_provider_models(query).await
}

fn validate_model_id(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 200 {
        return Err("model ID is invalid".to_owned());
    }
    if value.chars().any(|character| {
        character.is_control() || character.is_whitespace() || "?#".contains(character)
    }) {
        return Err("model ID contains unsupported characters".to_owned());
    }
    Ok(value)
}

fn extract_openai_response_text(body: &Value) -> Option<String> {
    if let Some(text) = body.get("output_text").and_then(Value::as_str) {
        return Some(text.to_owned());
    }
    body.get("output")
        .and_then(Value::as_array)?
        .iter()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|content| content.get("text").and_then(Value::as_str))
        .map(str::to_owned)
}

#[tauri::command]
pub(crate) async fn generate_provider_response(
    input: GenerateProviderResponseInput,
) -> Result<GenerateProviderResponseResult, String> {
    let instance_id = validate_instance_id(&input.provider_instance_id)?;
    let provider_id = input.provider_id.trim();
    let model_id = validate_model_id(&input.model_id)?;
    let prompt = input.prompt.trim();
    if prompt.is_empty() || prompt.len() > 100_000 {
        return Err("message is empty or too large".to_owned());
    }
    let spec = adapter_spec(provider_id)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| "AI-OS could not initialize AI Center".to_owned())?;
    let secret = if matches!(spec.auth, AuthStyle::None) {
        None
    } else {
        Some(read_current_credential(instance_id).await?)
    };
    let uses_openai_codex = secret
        .as_ref()
        .and_then(|credential| credential.route_kind.as_deref())
        == Some("openai-codex");
    let uses_grok_oauth = secret
        .as_ref()
        .and_then(|credential| credential.route_kind.as_deref())
        == Some("grok-oauth");

    let (mut request, body) = match provider_id {
        "openai" => (
            client.post(if uses_openai_codex {
                "https://chatgpt.com/backend-api/codex/responses"
            } else {
                "https://api.openai.com/v1/responses"
            }),
            serde_json::json!({
                "model": model_id,
                "input": prompt,
                "store": !uses_openai_codex
            }),
        ),
        "anthropic" => (
            client.post("https://api.anthropic.com/v1/messages"),
            serde_json::json!({
                "model": model_id,
                "max_tokens": 2048,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "google" => (
            client.post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model_id}:generateContent"
            )),
            serde_json::json!({"contents": [{"parts": [{"text": prompt}]}]}),
        ),
        "grok" => (
            client.post(if uses_grok_oauth {
                "https://cli-chat-proxy.grok.com/v1/responses"
            } else {
                "https://api.x.ai/v1/chat/completions"
            }),
            if uses_grok_oauth {
                serde_json::json!({"model": model_id, "input": prompt, "store": false})
            } else {
                serde_json::json!({
                    "model": model_id,
                    "messages": [{"role": "user", "content": prompt}]
                })
            },
        ),
        "deepseek" => (
            client.post("https://api.deepseek.com/chat/completions"),
            serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "openrouter" => (
            client.post("https://openrouter.ai/api/v1/chat/completions"),
            serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "kimi" => (
            client.post("https://api.kimi.com/coding/v1/chat/completions"),
            serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "meta" => (
            client.post("https://api.meta.ai/v1/chat/completions"),
            serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "ollama" => (
            client.post("http://127.0.0.1:11434/api/chat"),
            serde_json::json!({
                "model": model_id,
                "stream": false,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        _ => return Err("this Provider cannot answer through AI Center yet".to_owned()),
    };

    if let Some(credential) = secret {
        request = authenticate_provider_request(request, spec.auth, credential);
    }
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|_| "AI Center could not reach the selected Provider".to_owned())?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "The selected Provider needs to be reconnected".to_owned(),
            429 => "The selected Provider is busy or rate limited".to_owned(),
            _ => format!("The selected Provider returned status {}", status.as_u16()),
        });
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "The selected Provider returned an unreadable response".to_owned())?;
    let text = match provider_id {
        "openai" => extract_openai_response_text(&body),
        "grok" if uses_grok_oauth => extract_openai_response_text(&body),
        "anthropic" => body
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "google" => body
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "ollama" => body
            .pointer("/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
    .filter(|text| !text.trim().is_empty())
    .ok_or_else(|| "The selected Provider returned no text".to_owned())?;

    Ok(GenerateProviderResponseResult {
        provider_id: provider_id.to_owned(),
        model_id: model_id.to_owned(),
        text,
    })
}

async fn execute_candidate(
    candidate: &AiCenterRouteCandidate,
    prompt: &str,
    instances: &[ProviderInstance],
    operation_id: Option<String>,
) -> Result<GenerateProviderResponseResult, String> {
    let uses_claude_code = instances.iter().any(|instance| {
        instance.id == candidate.provider_instance_id
            && instance.provider_id == "anthropic"
            && instance.credential.kind == ProviderCredentialKind::Local
    });
    if uses_claude_code {
        let response = crate::claude_code::generate_claude_code_response(
            crate::claude_code::ClaudeCodeRequest {
                operation_id,
                model_id: candidate.model_id.clone(),
                prompt: prompt.to_owned(),
            },
        )
        .await?;
        return Ok(GenerateProviderResponseResult {
            provider_id: "anthropic".to_owned(),
            model_id: response.model_id,
            text: response.text,
        });
    }

    generate_provider_response(GenerateProviderResponseInput {
        provider_id: candidate.provider_id.clone(),
        provider_instance_id: candidate.provider_instance_id.clone(),
        model_id: candidate.model_id.clone(),
        prompt: prompt.to_owned(),
    })
    .await
}

#[tauri::command]
pub(crate) async fn execute_ai_center(
    input: ExecuteAiCenterInput,
) -> Result<ExecuteAiCenterResult, String> {
    let prompt = input.prompt.trim().to_owned();
    if prompt.is_empty() || prompt.len() > 100_000 {
        return Err("message is empty or too large".to_owned());
    }
    let invocation_id = format!("invoke-{}", uuid::Uuid::new_v4());
    let invocation_started_at = chrono::Utc::now().to_rfc3339();
    let invocation_started = Instant::now();
    let (instances, candidates) = route_candidates()?;
    let mut attempts = Vec::new();
    let mut last_error = None;

    loop {
        let route_input = ResolveAiCenterRouteInput {
            route_mode: input.route_mode,
            manual_candidate: input.manual_candidate.clone(),
            attempted_count: attempts.len(),
            emitted_output: false,
            cancelled: false,
        };
        let Some(candidate) = select_route_candidate(&route_input, &candidates)? else {
            return Err(last_error.unwrap_or_else(|| "NO_CONNECTED_PROVIDER".to_owned()));
        };
        let attempt_started_at = chrono::Utc::now().to_rfc3339();
        let attempt_started = Instant::now();
        match execute_candidate(&candidate, &prompt, &instances, None).await {
            Ok(response) => {
                attempts.push(complete_attempt(
                    &candidate,
                    attempt_started_at,
                    attempt_started,
                    AiCenterAttemptOutcome::Success,
                    None,
                ));
                let metadata = canonical_metadata(
                    invocation_id,
                    input.route_mode,
                    &candidate,
                    invocation_started_at,
                    invocation_started,
                    &prompt,
                    &response.text,
                    attempts,
                );
                return Ok(ExecuteAiCenterResult {
                    provider_id: response.provider_id,
                    model_id: response.model_id,
                    text: response.text,
                    metadata,
                });
            }
            Err(error) => {
                attempts.push(complete_attempt(
                    &candidate,
                    attempt_started_at,
                    attempt_started,
                    AiCenterAttemptOutcome::Failed,
                    Some(&error),
                ));
                last_error = Some(error);
            }
        }
    }
}

fn stream_text(provider_id: &str, uses_responses_api: bool, value: &Value) -> Option<String> {
    match provider_id {
        _ if uses_responses_api => value
            .get("delta")
            .and_then(Value::as_str)
            .filter(|_| {
                value.get("type").and_then(Value::as_str) == Some("response.output_text.delta")
            })
            .map(str::to_owned),
        "anthropic" => value
            .pointer("/delta/text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "google" => value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "ollama" => value
            .pointer("/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

#[tauri::command]
pub(crate) async fn execute_ai_center_stream(
    app: AppHandle,
    input: ExecuteAiCenterStreamInput,
) -> Result<(), String> {
    let operation_id = validate_instance_id(input.operation_id.trim())?.to_owned();
    if input.messages.is_empty() || input.messages.len() > 100 {
        return Err("conversation is empty or too long".to_owned());
    }
    let prompt = input
        .messages
        .iter()
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let invocation_started_at = chrono::Utc::now().to_rfc3339();
    let invocation_started = Instant::now();
    let (instances, candidates) = route_candidates()?;
    let token = CancellationToken::new();
    ai_center_requests()
        .lock()
        .map_err(|_| "AI Center request state is unavailable".to_owned())?
        .insert(operation_id.clone(), token.clone());
    let mut attempts = Vec::new();
    let mut last_error = None;

    loop {
        let route_input = ResolveAiCenterRouteInput {
            route_mode: input.route_mode,
            manual_candidate: input.manual_candidate.clone(),
            attempted_count: attempts.len(),
            emitted_output: false,
            cancelled: token.is_cancelled(),
        };
        let Some(candidate) = select_route_candidate(&route_input, &candidates)? else {
            ai_center_requests()
                .lock()
                .ok()
                .map(|mut map| map.remove(&operation_id));
            if token.is_cancelled() {
                let candidate = attempts
                    .last()
                    .map(|attempt: &AiCenterAttempt| AiCenterRouteCandidate {
                        provider_id: attempt.provider_id.clone(),
                        provider_instance_id: attempt.provider_instance_id.clone(),
                        model_id: attempt.model_id.clone(),
                    })
                    .or_else(|| input.manual_candidate.clone())
                    .or_else(|| candidates.first().cloned())
                    .ok_or_else(|| "NO_CONNECTED_PROVIDER".to_owned())?;
                let metadata = canonical_metadata(
                    operation_id.clone(),
                    input.route_mode,
                    &candidate,
                    invocation_started_at,
                    invocation_started,
                    &prompt,
                    "",
                    attempts,
                );
                let _ = app.emit(
                    "ai-center://done",
                    AiCenterDoneEvent {
                        operation_id,
                        cancelled: true,
                        provider_id: Some(candidate.provider_id),
                        model_id: Some(candidate.model_id),
                        metadata: Some(metadata),
                    },
                );
                return Ok(());
            }
            let message = last_error.unwrap_or_else(|| "NO_CONNECTED_PROVIDER".to_owned());
            let _ = app.emit(
                "ai-center://error",
                AiCenterErrorEvent {
                    operation_id,
                    message: message.clone(),
                },
            );
            return Err(message);
        };
        let attempt_started_at = chrono::Utc::now().to_rfc3339();
        let attempt_started = Instant::now();
        let attempt_operation_id = format!("{}-{}", operation_id, attempts.len());
        let execution = execute_candidate(
            &candidate,
            &prompt,
            &instances,
            Some(attempt_operation_id.clone()),
        );
        let result = tokio::select! {
            _ = token.cancelled() => {
                let _ = crate::claude_code::cancel_claude_code_request(attempt_operation_id);
                Err("cancelled".to_owned())
            }
            result = execution => result
        };
        match result {
            Ok(response) => {
                attempts.push(complete_attempt(
                    &candidate,
                    attempt_started_at,
                    attempt_started,
                    AiCenterAttemptOutcome::Success,
                    None,
                ));
                let metadata = canonical_metadata(
                    operation_id.clone(),
                    input.route_mode,
                    &candidate,
                    invocation_started_at,
                    invocation_started,
                    &prompt,
                    &response.text,
                    attempts,
                );
                let _ = app.emit(
                    "ai-center://chunk",
                    AiCenterChunkEvent {
                        operation_id: operation_id.clone(),
                        text: response.text,
                    },
                );
                let _ = app.emit(
                    "ai-center://done",
                    AiCenterDoneEvent {
                        operation_id: operation_id.clone(),
                        cancelled: false,
                        provider_id: Some(response.provider_id),
                        model_id: Some(response.model_id),
                        metadata: Some(metadata),
                    },
                );
                ai_center_requests()
                    .lock()
                    .ok()
                    .map(|mut map| map.remove(&operation_id));
                return Ok(());
            }
            Err(_) if token.is_cancelled() => {
                attempts.push(complete_attempt(
                    &candidate,
                    attempt_started_at,
                    attempt_started,
                    AiCenterAttemptOutcome::Cancelled,
                    None,
                ));
            }
            Err(error) => {
                attempts.push(complete_attempt(
                    &candidate,
                    attempt_started_at,
                    attempt_started,
                    AiCenterAttemptOutcome::Failed,
                    Some(&error),
                ));
                last_error = Some(error);
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn start_provider_response_stream(
    app: AppHandle,
    input: StreamProviderResponseInput,
) -> Result<(), String> {
    let operation_id = validate_instance_id(input.operation_id.trim())?.to_owned();
    let instance_id = validate_instance_id(&input.provider_instance_id)?;
    let provider_id = input.provider_id.trim();
    let model_id = validate_model_id(&input.model_id)?;
    if input.messages.is_empty() || input.messages.len() > 100 {
        return Err("conversation is empty or too long".to_owned());
    }
    let total_chars: usize = input
        .messages
        .iter()
        .map(|message| message.content.len())
        .sum();
    if total_chars > 200_000
        || input.messages.iter().any(|message| {
            !matches!(message.role.as_str(), "system" | "user" | "assistant")
                || message.content.trim().is_empty()
        })
    {
        return Err("conversation contains invalid messages".to_owned());
    }
    let messages = &input.messages;
    let anthropic_system = messages
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let anthropic_messages = messages
        .iter()
        .filter(|message| message.role != "system")
        .collect::<Vec<_>>();
    let spec = adapter_spec(provider_id)?;
    let token = CancellationToken::new();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| "AI-OS could not initialize AI Center".to_owned())?;
    let secret = if matches!(spec.auth, AuthStyle::None) {
        None
    } else {
        Some(read_current_credential(instance_id).await?)
    };
    let uses_openai_codex = secret
        .as_ref()
        .and_then(|credential| credential.route_kind.as_deref())
        == Some("openai-codex");
    let uses_grok_oauth = secret
        .as_ref()
        .and_then(|credential| credential.route_kind.as_deref())
        == Some("grok-oauth");
    let (mut request, body) = match provider_id {
        "openai" => (
            client.post(if uses_openai_codex {
                "https://chatgpt.com/backend-api/codex/responses"
            } else {
                "https://api.openai.com/v1/responses"
            }),
            serde_json::json!({
                "model": model_id,
                "input": messages,
                "stream": true,
                "store": !uses_openai_codex
            }),
        ),
        "anthropic" => (
            client.post("https://api.anthropic.com/v1/messages"),
            serde_json::json!({
                "model": model_id, "max_tokens": 2048, "stream": true,
                "system": anthropic_system,
                "messages": anthropic_messages
            }),
        ),
        "google" => (
            client.post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model_id}:streamGenerateContent?alt=sse"
            )),
            serde_json::json!({
                "contents": messages.iter().map(|message| serde_json::json!({
                    "role": if message.role == "assistant" { "model" } else { "user" },
                    "parts": [{"text": message.content}]
                })).collect::<Vec<_>>()
            }),
        ),
        "grok" => (
            client.post(if uses_grok_oauth {
                "https://cli-chat-proxy.grok.com/v1/responses"
            } else {
                "https://api.x.ai/v1/chat/completions"
            }),
            if uses_grok_oauth {
                serde_json::json!({"model": model_id, "input": messages, "stream": true, "store": false})
            } else {
                serde_json::json!({
                    "model": model_id, "stream": true,
                    "messages": messages
                })
            },
        ),
        "deepseek" => (
            client.post("https://api.deepseek.com/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "openrouter" => (
            client.post("https://openrouter.ai/api/v1/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "kimi" => (
            client.post("https://api.kimi.com/coding/v1/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "meta" => (
            client.post("https://api.meta.ai/v1/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "ollama" => (
            client.post("http://127.0.0.1:11434/api/chat"),
            serde_json::json!({
                "model": model_id,
                "stream": true,
                "messages": messages,
                "options": {
                    "num_predict": 8192
                }
            }),
        ),
        _ => return Err("this Provider cannot stream through AI Center yet".to_owned()),
    };
    if let Some(credential) = secret {
        request = authenticate_provider_request(request, spec.auth, credential);
    }
    ai_center_requests()
        .lock()
        .map_err(|_| "AI Center request state is unavailable".to_owned())?
        .insert(operation_id.clone(), token.clone());

    let response = tokio::select! {
        _ = token.cancelled() => {
            ai_center_requests().lock().ok().map(|mut map| map.remove(&operation_id));
            let _ = app.emit("ai-center://done", AiCenterDoneEvent {
                operation_id: operation_id.clone(),
                cancelled: true,
                provider_id: None,
                model_id: None,
                metadata: None,
            });
            return Ok(());
        }
        response = request.json(&body).send() => {
            match response {
                Ok(response) => response,
                Err(_) => {
                    ai_center_requests().lock().ok().map(|mut map| map.remove(&operation_id));
                    let message = "AI Center could not reach the selected Provider".to_owned();
                    let _ = app.emit("ai-center://error", AiCenterErrorEvent {
                        operation_id: operation_id.clone(),
                        message: message.clone(),
                    });
                    return Err(message);
                }
            }
        }
    };
    if !response.status().is_success() {
        let message = match response.status().as_u16() {
            401 | 403 => "The selected Provider needs to be reconnected",
            429 => "The selected Provider is busy or rate limited",
            _ => "The selected Provider rejected the streaming request",
        }
        .to_owned();
        let _ = app.emit(
            "ai-center://error",
            AiCenterErrorEvent {
                operation_id: operation_id.clone(),
                message: message.clone(),
            },
        );
        ai_center_requests()
            .lock()
            .ok()
            .map(|mut map| map.remove(&operation_id));
        return Err(message);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut cancelled = false;
    loop {
        let next = tokio::select! {
            _ = token.cancelled() => {
                cancelled = true;
                None
            }
            item = stream.next() => item
        };
        let Some(item) = next else { break };
        let bytes = match item {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("OLLAMA STREAM ERROR: {:?}", err);
                let _ = app.emit(
                    "ai-center://error",
                    AiCenterErrorEvent {
                        operation_id: operation_id.clone(),
                        message: "The Provider stream was interrupted".to_owned(),
                    },
                );
                break;
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(position) = buffer.find('\n') {
            let line = buffer[..position].trim().to_owned();
            buffer.drain(..=position);
            let data = if provider_id == "ollama" {
                line.as_str()
            } else {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                data.trim()
            };
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                if let Some(text) =
                    stream_text(provider_id, uses_openai_codex || uses_grok_oauth, &value)
                {
                    let _ = app.emit(
                        "ai-center://chunk",
                        AiCenterChunkEvent {
                            operation_id: operation_id.clone(),
                            text,
                        },
                    );
                }
            }
        }
    }
    ai_center_requests()
        .lock()
        .ok()
        .map(|mut map| map.remove(&operation_id));
    let _ = app.emit(
        "ai-center://done",
        AiCenterDoneEvent {
            operation_id,
            cancelled,
            provider_id: None,
            model_id: None,
            metadata: None,
        },
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn cancel_provider_response_stream(operation_id: String) -> Result<(), String> {
    let operation_id = validate_instance_id(operation_id.trim())?;
    if let Some(token) = ai_center_requests()
        .lock()
        .map_err(|_| "AI Center request state is unavailable".to_owned())?
        .get(operation_id)
        .cloned()
    {
        token.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_instance_ids_are_bounded_and_path_free() {
        assert_eq!(
            validate_instance_id("grok-personal").unwrap(),
            "grok-personal"
        );
        assert!(validate_instance_id("../secret").is_err());
        assert!(validate_instance_id("").is_err());
    }

    #[test]
    fn parses_openai_and_ollama_model_lists() {
        let openai =
            parse_models("openai", serde_json::json!({"data": [{"id": "gpt-test"}]})).unwrap();
        assert_eq!(openai[0].id, "gpt-test");

        let codex = parse_models(
            "openai",
            serde_json::json!({
                "models": [{"slug": "gpt-codex", "display_name": "GPT Codex"}]
            }),
        )
        .unwrap();
        assert_eq!(codex[0].id, "gpt-codex");
        assert_eq!(codex[0].display_name, "GPT Codex");

        let ollama = parse_models(
            "ollama",
            serde_json::json!({"models": [{"name": "qwen:test"}]}),
        )
        .unwrap();
        assert_eq!(ollama[0].id, "qwen:test");
    }

    #[test]
    fn parses_openrouter_display_names() {
        let models = parse_models(
            "openrouter",
            serde_json::json!({"data": [{"id": "vendor/model", "name": "Vendor Model"}]}),
        )
        .unwrap();

        assert_eq!(models[0].id, "vendor/model");
        assert_eq!(models[0].display_name, "Vendor Model");
    }

    #[test]
    fn pkce_rejects_insecure_provider_endpoints() {
        assert!(validate_https_url("http://example.com/oauth", "OAuth URL").is_err());
    }

    #[test]
    fn oauth_credential_metadata_uses_real_token_values() {
        let token = serde_json::json!({
            "access_token": "access",
            "refresh_token": "refresh",
            "_aios": {"expiresAt": "2026-08-02T00:00:00Z"}
        });

        assert_eq!(
            oauth_credential_metadata(&token),
            (Some("2026-08-02T00:00:00Z".to_owned()), true)
        );
    }

    #[test]
    fn oauth_credential_metadata_does_not_invent_refresh_support() {
        for token in [
            serde_json::json!({"access_token": "access"}),
            serde_json::json!({"access_token": "access", "refresh_token": null}),
            serde_json::json!({"access_token": "access", "refresh_token": "  "}),
        ] {
            assert_eq!(oauth_credential_metadata(&token), (None, false));
        }
    }

    #[test]
    fn google_authentication_distinguishes_oauth_from_api_keys() {
        let client = reqwest::Client::new();
        let oauth = authenticate_provider_request(
            client.get("https://example.com/models"),
            AuthStyle::Google,
            CurrentProviderCredential {
                value: "oauth-access".to_owned(),
                oauth: true,
                resource_project_id: Some("ai-os-test".to_owned()),
                route_kind: None,
                account_id: None,
            },
        )
        .build()
        .unwrap();
        assert_eq!(oauth.headers()["authorization"], "Bearer oauth-access");
        assert_eq!(oauth.headers()["x-goog-user-project"], "ai-os-test");
        assert!(!oauth.headers().contains_key("x-goog-api-key"));

        let api_key = authenticate_provider_request(
            client.get("https://example.com/models"),
            AuthStyle::Google,
            CurrentProviderCredential {
                value: "api-key".to_owned(),
                oauth: false,
                resource_project_id: None,
                route_kind: None,
                account_id: None,
            },
        )
        .build()
        .unwrap();
        assert_eq!(api_key.headers()["x-goog-api-key"], "api-key");
        assert!(!api_key.headers().contains_key("authorization"));
    }

    #[test]
    fn openai_codex_authentication_is_account_scoped() {
        let request = authenticate_provider_request(
            reqwest::Client::new().get("https://chatgpt.com/backend-api/codex/models"),
            AuthStyle::Bearer,
            CurrentProviderCredential {
                value: "codex-access".to_owned(),
                oauth: true,
                resource_project_id: None,
                route_kind: Some("openai-codex".to_owned()),
                account_id: Some("account-123".to_owned()),
            },
        )
        .build()
        .unwrap();

        assert_eq!(request.headers()["authorization"], "Bearer codex-access");
        assert_eq!(request.headers()["ChatGPT-Account-ID"], "account-123");
        assert_eq!(request.headers()["OpenAI-Beta"], "codex-1");
        assert_eq!(request.headers()["originator"], "ai-os");
    }

    #[test]
    fn extracts_chatgpt_account_id_from_id_token() {
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "account-123"
                }
            }))
            .unwrap(),
        );
        let token = format!("header.{payload}.signature");

        assert_eq!(
            chatgpt_account_id_from_jwt(&token).as_deref(),
            Some("account-123")
        );
        assert!(chatgpt_account_id_from_jwt("invalid").is_none());
    }

    #[test]
    fn refreshed_oauth_token_preserves_local_metadata_and_existing_refresh_token() {
        let current = serde_json::json!({
            "access_token": "old-access",
            "refresh_token": "old-refresh",
            "_aios": {
                "clientId": "trusted-client",
                "tokenUrl": "https://trusted.example/token",
                "expiresAt": "2026-08-01T00:00:00Z"
            }
        });
        let refreshed = serde_json::json!({
            "access_token": "new-access",
            "refresh_token": null,
            "expires_in": 3600,
            "_aios": {
                "clientId": "attacker-client",
                "tokenUrl": "https://attacker.example/token"
            }
        });

        let merged = merge_refreshed_oauth_token(&current, &refreshed).unwrap();

        assert_eq!(merged["access_token"], "new-access");
        assert_eq!(merged["refresh_token"], "old-refresh");
        assert_eq!(merged["_aios"]["clientId"], "trusted-client");
        assert_eq!(merged["_aios"]["tokenUrl"], "https://trusted.example/token");
        assert!(merged["_aios"]["expiresAt"].is_string());
    }

    #[test]
    fn refreshed_oauth_token_requires_a_real_access_token() {
        let current = serde_json::json!({"access_token": "old-access"});

        assert!(
            merge_refreshed_oauth_token(&current, &serde_json::json!({"expires_in": 3600}))
                .is_err()
        );
        assert!(
            merge_refreshed_oauth_token(&current, &serde_json::json!({"access_token": "  "}))
                .is_err()
        );
    }

    #[test]
    fn extracts_text_from_responses_api_output() {
        let body = serde_json::json!({
            "output": [{"content": [{"type": "output_text", "text": "hello"}]}]
        });
        assert_eq!(
            extract_openai_response_text(&body).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn extracts_stream_text_for_supported_protocols() {
        assert_eq!(
            stream_text(
                "openai",
                true,
                &serde_json::json!({
                    "type": "response.output_text.delta",
                    "delta": "A"
                })
            )
            .as_deref(),
            Some("A")
        );
        assert_eq!(
            stream_text(
                "ollama",
                false,
                &serde_json::json!({"message": {"content": "B"}})
            )
            .as_deref(),
            Some("B")
        );
    }

    fn provider_fixture() -> ProviderInstance {
        ProviderInstance {
            id: "openai-default".to_owned(),
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            credential: ProviderCredentialRef {
                kind: ProviderCredentialKind::ApiKey,
                keychain_account: Some("openai-default".to_owned()),
                expires_at: None,
                refreshable: false,
            },
            connection_state: ProviderConnectionState::Connected,
            models: vec![ProviderModelEntry {
                id: "openai-default:gpt-test".to_owned(),
                provider_instance_id: "openai-default".to_owned(),
                remote_model_id: "gpt-test".to_owned(),
                display_name: "GPT Test".to_owned(),
                capabilities: vec!["chat".to_owned(), "tool-use".to_owned()],
                context_window: Some(128_000),
                enabled: true,
                is_default: true,
            }],
            created_at: "2026-08-01T00:00:00Z".to_owned(),
            updated_at: "2026-08-01T00:00:00Z".to_owned(),
            last_tested_at: Some("2026-08-01T00:00:00Z".to_owned()),
        }
    }

    #[test]
    fn execution_agent_candidates_follow_ai_center_order_and_context_requirement() {
        let config = serde_json::json!({
            "agents": {
                "list": [
                    {
                        "id": "ai-os-files",
                        "model": {
                            "primary": "ollama-ai-os/qwen3:4b-instruct"
                        },
                        "params": {
                            "num_ctx": 65536
                        }
                    },
                    {
                        "id": "ai-os-exec-too-small",
                        "model": {
                            "primary": "ollama/qwen2.5:7b"
                        },
                        "params": {
                            "num_ctx": 32768
                        }
                    },
                    {
                        "id": "ai-os-exec-cloud",
                        "model": {
                            "primary": "openai/gpt-test"
                        },
                        "params": {
                            "num_ctx": 128000
                        }
                    },
                    {
                        "id": "ai-os-exec-standard",
                        "model": {
                            "primary": "ollama/qwen3:8b"
                        },
                        "params": {
                            "num_ctx": 65536
                        }
                    }
                ]
            }
        });

        let route_candidates = auto_route_candidates(
            &[provider_fixture()],
            &["qwen3:8b".to_owned(), "qwen2.5:7b".to_owned()],
        );

        let candidates =
            execution_agent_candidates_from_config("download.start", &config, &route_candidates)
                .unwrap();

        assert_eq!(
            candidates,
            vec![
                "ai-os-exec-standard".to_owned(),
                "ai-os-exec-cloud".to_owned(),
            ]
        );
    }

    #[test]
    fn auto_route_candidates_preserve_local_first_and_default_model_order() {
        let mut instance = provider_fixture();
        let mut secondary = instance.models[0].clone();
        secondary.id = "openai-default:gpt-secondary".to_owned();
        secondary.remote_model_id = "gpt-secondary".to_owned();
        secondary.display_name = "GPT Secondary".to_owned();
        secondary.is_default = false;
        instance.models.insert(0, secondary);

        let candidates = auto_route_candidates(&[instance], &["qwen3:8b".to_owned()]);
        let order = candidates
            .iter()
            .map(|candidate| candidate.model_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(order, vec!["qwen3:8b", "gpt-test", "gpt-secondary"]);
    }

    #[test]
    fn route_selection_allows_only_auto_pre_output_fallback() {
        let candidates = auto_route_candidates(&[provider_fixture()], &["qwen3:8b".to_owned()]);
        let input =
            |route_mode, attempted_count, emitted_output, cancelled| ResolveAiCenterRouteInput {
                route_mode,
                manual_candidate: Some(candidates[0].clone()),
                attempted_count,
                emitted_output,
                cancelled,
            };

        assert_eq!(
            select_route_candidate(
                &input(AiCenterRouteMode::Auto, 1, false, false),
                &candidates
            )
            .unwrap(),
            Some(candidates[1].clone())
        );
        assert_eq!(
            select_route_candidate(
                &input(AiCenterRouteMode::Manual, 1, false, false),
                &candidates
            )
            .unwrap(),
            None
        );
        assert_eq!(
            select_route_candidate(&input(AiCenterRouteMode::Auto, 1, true, false), &candidates)
                .unwrap(),
            None
        );
        assert_eq!(
            select_route_candidate(&input(AiCenterRouteMode::Auto, 1, false, true), &candidates)
                .unwrap(),
            None
        );
    }

    #[test]
    fn auto_route_falls_back_to_cloud_when_no_local_models_are_available() {
        let candidates = auto_route_candidates(&[provider_fixture()], &[]);
        let input = ResolveAiCenterRouteInput {
            route_mode: AiCenterRouteMode::Auto,
            manual_candidate: None,
            attempted_count: 0,
            emitted_output: false,
            cancelled: false,
        };

        let selected = select_route_candidate(&input, &candidates)
            .unwrap()
            .expect("connected cloud candidate");
        assert_eq!(selected.provider_id, "openai");
        assert_eq!(selected.model_id, "gpt-test");
    }

    #[test]
    fn manual_route_executes_only_the_requested_candidate() {
        let candidates = auto_route_candidates(&[provider_fixture()], &["qwen2.5:7b".to_owned()]);
        let requested = candidates[1].clone();
        let input = |attempted_count| ResolveAiCenterRouteInput {
            route_mode: AiCenterRouteMode::Manual,
            manual_candidate: Some(requested.clone()),
            attempted_count,
            emitted_output: false,
            cancelled: false,
        };

        assert_eq!(
            select_route_candidate(&input(0), &candidates).unwrap(),
            Some(requested.clone())
        );
        assert_eq!(
            select_route_candidate(&input(1), &candidates).unwrap(),
            None
        );
    }

    #[test]
    fn canonical_metadata_preserves_attempt_order_tokens_and_privacy() {
        let local = AiCenterRouteCandidate {
            provider_id: "ollama".to_owned(),
            provider_instance_id: "ollama-local".to_owned(),
            model_id: "qwen-test".to_owned(),
        };
        let cloud = AiCenterRouteCandidate {
            provider_id: "openai".to_owned(),
            provider_instance_id: "openai-default".to_owned(),
            model_id: "gpt-test".to_owned(),
        };
        let failed = complete_attempt(
            &local,
            chrono::Utc::now().to_rfc3339(),
            Instant::now(),
            AiCenterAttemptOutcome::Failed,
            Some("Provider returned 401"),
        );
        let success = complete_attempt(
            &cloud,
            chrono::Utc::now().to_rfc3339(),
            Instant::now(),
            AiCenterAttemptOutcome::Success,
            None,
        );
        let metadata = canonical_metadata(
            "test-invocation".to_owned(),
            AiCenterRouteMode::Auto,
            &cloud,
            chrono::Utc::now().to_rfc3339(),
            Instant::now(),
            "private prompt",
            "private output",
            vec![failed, success],
        );

        assert_eq!(metadata.route_mode, AiCenterRouteMode::Auto);
        assert_eq!(metadata.source, AiCenterExecutionSource::Cloud);
        assert_eq!(metadata.input_tokens, 4);
        assert_eq!(metadata.output_tokens, 4);
        assert_eq!(metadata.token_accuracy, "estimated");
        assert!(metadata.fallback_occurred);
        assert_eq!(
            metadata.attempts[0].error_category.as_deref(),
            Some("authentication")
        );
        assert_eq!(
            metadata.attempts[1].outcome,
            AiCenterAttemptOutcome::Success
        );
        let serialized = serde_json::to_string(&metadata).unwrap().to_lowercase();
        assert!(!serialized.contains("private prompt"));
        assert!(!serialized.contains("private output"));
        assert!(!serialized.contains("credential"));
        assert!(!serialized.contains("api key"));
        assert!(!serialized.contains("access token"));
        assert!(!serialized.contains("refresh token"));
    }

    #[test]
    fn canonical_error_categories_match_existing_semantics() {
        assert_eq!(safe_attempt_error("cancelled"), "cancelled");
        assert_eq!(
            safe_attempt_error("Provider returned 403"),
            "authentication"
        );
        assert_eq!(safe_attempt_error("rate limit 429"), "rate-limited");
        assert_eq!(safe_attempt_error("request timeout"), "timeout");
        assert_eq!(safe_attempt_error("returned no text"), "empty-response");
        assert_eq!(
            safe_attempt_error("network could not reach host"),
            "unavailable"
        );
        assert_eq!(safe_attempt_error("unexpected"), "provider-error");
    }

    #[test]
    fn provider_instance_serialization_matches_frontend_contract() {
        let value = serde_json::to_value(provider_fixture()).unwrap();

        assert_eq!(value["id"], "openai-default");
        assert_eq!(value["providerId"], "openai");
        assert_eq!(value["connectionState"], "connected");
        assert_eq!(value["credential"]["kind"], "api-key");
        assert_eq!(value["models"][0]["providerInstanceId"], "openai-default");
        assert_eq!(value["models"][0]["isDefault"], true);
    }

    #[test]
    fn provider_instance_round_trips_without_secret_material() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(PROVIDER_INSTANCES_FILE);
        let fixture = provider_fixture();

        write_provider_instances_at(&path, std::slice::from_ref(&fixture)).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();

        assert!(!contents.to_lowercase().contains("api key value"));
        assert!(!contents.to_lowercase().contains("access_token"));
        assert!(!contents.to_lowercase().contains("refresh_token"));

        assert_eq!(read_provider_instances_at(&path).unwrap(), vec![fixture]);
    }

    #[test]
    fn provider_instance_rejects_secret_fields_in_migration_input() {
        let value = serde_json::json!({
            "id": "openai-default",
            "providerId": "openai",
            "displayName": "OpenAI",
            "credential": {
                "kind": "api-key",
                "keychainAccount": "openai-default",
                "refreshable": false,
                "secret": "must-not-be-accepted"
            },
            "connectionState": "connected",
            "models": [],
            "createdAt": "2026-08-01T00:00:00Z",
            "updatedAt": "2026-08-01T00:00:00Z"
        });

        assert!(serde_json::from_value::<ProviderInstance>(value).is_err());
    }

    #[test]
    fn provider_validation_rejects_foreign_and_duplicate_models() {
        let mut foreign = provider_fixture();
        foreign.models[0].provider_instance_id = "another-provider".to_owned();

        assert!(validate_provider_instance(foreign).is_err());

        let mut duplicate = provider_fixture();
        duplicate.models.push(duplicate.models[0].clone());

        assert!(validate_provider_instance(duplicate).is_err());
    }
    #[test]
    fn provider_adapter_registry_has_stable_order_and_ids() {
        let adapters = list_provider_adapters();

        assert_eq!(
            adapters
                .iter()
                .map(|adapter| adapter.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "openai",
                "anthropic",
                "google",
                "grok",
                "deepseek",
                "openrouter",
                "doubao",
                "kimi",
                "meta",
                "compatible",
                "ollama",
            ]
        );
    }

    #[test]
    fn provider_adapter_serialization_matches_frontend_contract() {
        let descriptor = get_provider_adapter("openai".to_owned()).unwrap();
        let value = serde_json::to_value(descriptor).unwrap();

        assert_eq!(value["providerId"], "openai");
        assert_eq!(value["displayName"], "OpenAI");
        assert_eq!(value["adapterKind"], "native");
        assert_eq!(value["supportsModelDiscovery"], true);
        assert_eq!(value["supportsTokenRefresh"], true);
        assert_eq!(
            value["credentialKinds"],
            serde_json::json!(["oauth", "api-key"])
        );
        assert!(value["capabilities"].is_array());
    }

    #[test]
    fn native_specs_and_public_registry_share_the_same_provider_ids() {
        for provider_id in [
            "openai",
            "anthropic",
            "google",
            "grok",
            "deepseek",
            "openrouter",
            "kimi",
            "meta",
            "ollama",
        ] {
            let descriptor = get_provider_adapter(provider_id.to_owned()).unwrap();
            let native = adapter_spec(provider_id).unwrap();

            assert_eq!(descriptor.provider_id, native.id);
            assert_eq!(descriptor.adapter_kind, ProviderAdapterKind::Native);
        }
    }

    #[test]
    fn grok_and_deepseek_use_current_official_api_key_model_catalogs() {
        let grok = adapter_spec("grok").unwrap();
        let deepseek = adapter_spec("deepseek").unwrap();

        assert_eq!(grok.models_url, "https://api.x.ai/v1/language-models");
        assert_eq!(deepseek.models_url, "https://api.deepseek.com/models");
        assert!(matches!(grok.auth, AuthStyle::Bearer));
        assert!(matches!(deepseek.auth, AuthStyle::Bearer));
    }

    #[test]
    fn grok_advertises_device_login_and_api_key_without_generic_oauth() {
        let grok = get_provider_adapter("grok".to_owned()).unwrap();

        assert_eq!(
            grok.authentication_methods,
            vec![
                ProviderAuthenticationMethod::DeviceCode,
                ProviderAuthenticationMethod::ApiKey,
            ]
        );
        assert!(grok.supports_token_refresh);
    }

    #[test]
    fn kimi_code_advertises_official_device_login_and_api_key() {
        let kimi = get_provider_adapter("kimi".to_owned()).unwrap();

        assert_eq!(
            kimi.authentication_methods,
            vec![
                ProviderAuthenticationMethod::DeviceCode,
                ProviderAuthenticationMethod::ApiKey,
            ]
        );
        assert_eq!(kimi.adapter_kind, ProviderAdapterKind::Native);
        assert!(kimi.supports_token_refresh);
        assert_eq!(
            adapter_spec("kimi").unwrap().models_url,
            "https://api.kimi.com/coding/v1/models"
        );
    }

    #[test]
    fn openrouter_advertises_pkce_account_connection_and_api_key() {
        let openrouter = get_provider_adapter("openrouter".to_owned()).unwrap();

        assert_eq!(
            openrouter.authentication_methods,
            vec![
                ProviderAuthenticationMethod::OAuthPkce,
                ProviderAuthenticationMethod::OAuthLoopback,
                ProviderAuthenticationMethod::ApiKey,
            ]
        );
        assert_eq!(openrouter.adapter_kind, ProviderAdapterKind::Native);
        assert!(!openrouter.supports_token_refresh);
    }

    #[test]
    fn jwt_subject_extracts_only_a_bounded_subject() {
        let token = "header.eyJzdWIiOiJ4YWktdXNlci0xMjMifQ.signature";
        assert_eq!(jwt_subject(token).as_deref(), Some("xai-user-123"));
        assert_eq!(jwt_subject("invalid"), None);
    }

    #[test]
    fn remaining_catalog_provider_is_visible_but_not_claimed_as_native() {
        let descriptor = get_provider_adapter("doubao".to_owned()).unwrap();

        assert_eq!(descriptor.adapter_kind, ProviderAdapterKind::Catalog);
        assert!(adapter_spec("doubao").is_err());
    }

    #[test]
    fn authentication_methods_only_claim_operational_flows() {
        let openai = get_provider_adapter("openai".to_owned()).unwrap();

        assert_eq!(
            openai.authentication_methods,
            vec![
                ProviderAuthenticationMethod::OAuthPkce,
                ProviderAuthenticationMethod::OAuthLoopback,
                ProviderAuthenticationMethod::ApiKey,
            ]
        );
        assert!(openai.supports_token_refresh);
        assert!(!openai.supports_multiple_credentials);

        let anthropic = get_provider_adapter("anthropic".to_owned()).unwrap();
        assert_eq!(
            anthropic.authentication_methods,
            vec![
                ProviderAuthenticationMethod::ApiKey,
                ProviderAuthenticationMethod::CliAccount,
            ]
        );
        assert_eq!(
            anthropic.credential_kinds,
            vec![
                ProviderCredentialKind::ApiKey,
                ProviderCredentialKind::Local
            ]
        );

        let google = get_provider_adapter("google".to_owned()).unwrap();
        assert_eq!(
            google.authentication_methods,
            vec![
                ProviderAuthenticationMethod::OAuthPkce,
                ProviderAuthenticationMethod::OAuthLoopback,
                ProviderAuthenticationMethod::ApiKey,
            ]
        );
        assert!(google.supports_token_refresh);

        let ollama = get_provider_adapter("ollama".to_owned()).unwrap();

        assert_eq!(
            ollama.authentication_methods,
            vec![ProviderAuthenticationMethod::Local]
        );
    }

    #[test]
    fn oauth_session_is_consumed_exactly_once() {
        let state = format!("state-{}", uuid::Uuid::new_v4().simple());
        let now = unix_timestamp_seconds().unwrap();
        let session = OAuthSession {
            provider_id: "openai".to_owned(),
            provider_instance_id: "openai-default".to_owned(),
            client_id: "client".to_owned(),
            client_secret: Some("must-not-appear".to_owned()),
            token_url: "https://example.com/token".to_owned(),
            resource_project_id: None,
            route_kind: None,
            redirect_uri: "http://127.0.0.1/callback".to_owned(),
            verifier: "verifier".to_owned(),
            created_at: now,
            expires_at: now + 60,
        };

        let debug = format!("{session:?}");
        assert!(debug.contains("client_secret_configured"));
        assert!(!debug.contains("must-not-appear"));
        assert!(!debug.contains("verifier"));

        store_oauth_session(&state, session.clone()).unwrap();

        assert_eq!(consume_oauth_session("openai", &state).unwrap(), session);
        assert!(consume_oauth_session("openai", &state).is_err());
    }

    #[test]
    fn oauth_session_cancellation_is_provider_scoped_and_consumes_state() {
        let state = format!("cancel-{}", uuid::Uuid::new_v4().simple());
        let now = unix_timestamp_seconds().unwrap();
        let session = OAuthSession {
            provider_id: "openai".to_owned(),
            provider_instance_id: "openai-default".to_owned(),
            client_id: "client".to_owned(),
            client_secret: None,
            token_url: "https://example.com/token".to_owned(),
            resource_project_id: None,
            route_kind: None,
            redirect_uri: "http://127.0.0.1/callback".to_owned(),
            verifier: "verifier".to_owned(),
            created_at: now,
            expires_at: now + 60,
        };
        let token = CancellationToken::new();

        store_oauth_session(&state, session).unwrap();
        oauth_cancellations()
            .lock()
            .unwrap()
            .insert(oauth_session_key("openai", &state), token.clone());

        assert!(!discard_oauth_session("google", &state).unwrap());
        assert!(!token.is_cancelled());
        assert!(discard_oauth_session("openai", &state).unwrap());
        assert!(token.is_cancelled());
        assert!(consume_oauth_session("openai", &state).is_err());
        assert!(!discard_oauth_session("openai", &state).unwrap());
    }

    #[test]
    fn expired_oauth_session_fails_closed_and_is_consumed() {
        let state = format!("expired-{}", uuid::Uuid::new_v4().simple());
        let now = unix_timestamp_seconds().unwrap();
        let session = OAuthSession {
            provider_id: "google".to_owned(),
            provider_instance_id: "google-default".to_owned(),
            client_id: "client".to_owned(),
            client_secret: None,
            token_url: "https://example.com/token".to_owned(),
            resource_project_id: None,
            route_kind: None,
            redirect_uri: "http://127.0.0.1/callback".to_owned(),
            verifier: "verifier".to_owned(),
            created_at: now.saturating_sub(120),
            expires_at: now,
        };

        store_oauth_session(&state, session).unwrap();

        assert!(consume_oauth_session("google", &state).is_err());
        assert!(consume_oauth_session("google", &state).is_err());
    }

    #[test]
    fn oauth_session_provider_identity_is_part_of_lookup_key() {
        let state = format!("provider-{}", uuid::Uuid::new_v4().simple());
        let now = unix_timestamp_seconds().unwrap();
        let session = OAuthSession {
            provider_id: "openai".to_owned(),
            provider_instance_id: "openai-default".to_owned(),
            client_id: "client".to_owned(),
            client_secret: None,
            token_url: "https://example.com/token".to_owned(),
            resource_project_id: None,
            route_kind: None,
            redirect_uri: "http://127.0.0.1/callback".to_owned(),
            verifier: "verifier".to_owned(),
            created_at: now,
            expires_at: now + 60,
        };

        store_oauth_session(&state, session.clone()).unwrap();

        assert!(consume_oauth_session("google", &state).is_err());
        assert_eq!(consume_oauth_session("openai", &state).unwrap(), session);
    }

    #[test]
    fn authentication_method_serialization_is_frontend_compatible() {
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::ApiKey).unwrap(),
            serde_json::json!("api-key")
        );
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::OAuthPkce).unwrap(),
            serde_json::json!("oauth-pkce")
        );
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::OAuthLoopback).unwrap(),
            serde_json::json!("oauth-loopback")
        );
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::DeviceCode).unwrap(),
            serde_json::json!("device-code")
        );
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::ImportedCredential).unwrap(),
            serde_json::json!("imported-credential")
        );
        assert_eq!(
            serde_json::to_value(ProviderAuthenticationMethod::Local).unwrap(),
            serde_json::json!("local")
        );
    }

    #[test]
    fn credential_kind_serialization_is_frontend_compatible() {
        for (kind, expected) in [
            (ProviderCredentialKind::OAuth, "oauth"),
            (ProviderCredentialKind::ApiKey, "api-key"),
            (ProviderCredentialKind::Local, "local"),
        ] {
            assert_eq!(serde_json::to_value(&kind).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<ProviderCredentialKind>(serde_json::json!(expected))
                    .unwrap(),
                kind
            );
        }
    }

    #[test]
    fn oauth_loopback_parser_accepts_matching_callback() {
        let request = "GET /oauth/callback?code=abc123&state=expected HTTP/1.1\r\n\
Host: 127.0.0.1\r\n\r\n";

        assert_eq!(
            parse_oauth_loopback_request(request, "expected").unwrap(),
            "abc123"
        );
    }

    #[test]
    fn oauth_token_rejection_exposes_only_a_bounded_error_code() {
        let payload = serde_json::json!({
            "error": "invalid_grant",
            "error_description": "authorization code and token details must stay private"
        });

        assert_eq!(
            oauth_token_rejection(400, Some(&payload)),
            "OAuth provider rejected the authorization code (invalid_grant)"
        );

        let unsafe_payload = serde_json::json!({
            "error": "invalid_grant<script>alert(1)</script>"
        });
        assert_eq!(
            oauth_token_rejection(400, Some(&unsafe_payload)),
            "OAuth provider rejected the authorization code (HTTP 400)"
        );
    }

    #[test]
    fn oauth_loopback_parser_rejects_wrong_state() {
        let request = "GET /oauth/callback?code=abc123&state=wrong HTTP/1.1\r\n\
Host: 127.0.0.1\r\n\r\n";

        assert!(parse_oauth_loopback_request(request, "expected").is_err());
    }

    #[test]
    fn oauth_loopback_parser_rejects_wrong_method_and_path() {
        let post = "POST /oauth/callback?code=abc&state=expected HTTP/1.1\r\n\r\n";
        let wrong_path = "GET /other?code=abc&state=expected HTTP/1.1\r\n\r\n";

        assert!(parse_oauth_loopback_request(post, "expected").is_err());

        assert!(parse_oauth_loopback_request(wrong_path, "expected").is_err());
    }

    #[test]
    fn oauth_loopback_parser_rejects_provider_error() {
        let request = "GET /oauth/callback?error=access_denied&state=expected HTTP/1.1\r\n\r\n";

        assert!(parse_oauth_loopback_request(request, "expected").is_err());
    }

    #[test]
    fn unknown_provider_adapter_is_rejected() {
        assert!(get_provider_adapter("unknown".to_owned()).is_err());
        assert!(get_provider_adapter("   ".to_owned()).is_err());
    }
}
