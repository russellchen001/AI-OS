use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

const KEYCHAIN_SERVICE: &str = "com.ai-os.provider";

const PROVIDER_INSTANCES_FILE: &str = "provider-instances.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderCredentialKind {
    OAuth,
    ApiKey,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderAuthenticationMethod {
    ApiKey,

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

fn ai_center_requests() -> &'static Mutex<HashMap<String, CancellationToken>> {
    AI_CENTER_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn oauth_sessions() -> &'static Mutex<HashMap<String, OAuthSession>> {
    OAUTH_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn oauth_cancellations() -> &'static Mutex<HashMap<String, CancellationToken>> {
    OAUTH_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeginOAuthResult {
    authorization_url: String,
    redirect_uri: String,
    state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OAuthSession {
    provider_id: String,
    provider_instance_id: String,
    client_id: String,
    token_url: String,
    redirect_uri: String,
    verifier: String,
    created_at: u64,
    expires_at: u64,
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
    expires_at: Option<String>,
    refreshable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOAuthErrorEvent {
    provider_id: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamProviderResponseInput {
    operation_id: String,
    provider_id: String,
    provider_instance_id: String,
    model_id: String,
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
    vec![
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
                ProviderCredentialKind::OAuth,
                ProviderCredentialKind::ApiKey,
            ],
            &["chat", "reasoning", "vision", "tool-use"],
            true,
            true,
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
                models_url: "https://api.x.ai/v1/models",
                auth: AuthStyle::Bearer,
            }),
        ),
        provider_adapter(
            "deepseek",
            "DeepSeek",
            ProviderAdapterKind::Native,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "tool-use"],
            true,
            false,
            Some(ProviderAdapterSpec {
                id: "deepseek",
                models_url: "https://api.deepseek.com/v1/models",
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
            "Kimi",
            ProviderAdapterKind::Catalog,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "tool-use"],
            true,
            false,
            None,
        ),
        provider_adapter(
            "meta",
            "Meta",
            ProviderAdapterKind::Catalog,
            &[ProviderCredentialKind::ApiKey],
            &["chat", "tool-use"],
            true,
            false,
            None,
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
    ]
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
    validate_https_url(token_url, "OAuth token URL")?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "AI-OS could not initialize OAuth refresh".to_owned())?
        .post(token_url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
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
    let mut merged = token.clone();
    let object = merged
        .as_object_mut()
        .ok_or_else(|| "OAuth token record is invalid".to_owned())?;
    if let Some(refreshed_object) = refreshed.as_object() {
        for (key, value) in refreshed_object {
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
    store_secret(
        account,
        &serde_json::to_vec(&merged)
            .map_err(|_| "AI-OS could not secure the refreshed OAuth token".to_owned())?,
    )?;
    Ok(merged)
}

async fn read_current_access_token(account: &str) -> Result<String, String> {
    let secret = read_secret(account)?;
    let Ok(mut token) = serde_json::from_slice::<Value>(&secret) else {
        return String::from_utf8(secret)
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
    token
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "OAuth response did not contain an access token".to_owned())
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
                item.get("id").and_then(Value::as_str)
            }?;
            let display_name = item
                .get("display_name")
                .or_else(|| item.get("displayName"))
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
    let mut request = client.get(spec.models_url);

    if !matches!(spec.auth, AuthStyle::None) {
        let secret = read_current_access_token(instance_id).await?;
        request = match spec.auth {
            AuthStyle::Bearer => request.bearer_auth(secret),
            AuthStyle::Anthropic => request
                .header("x-api-key", secret)
                .header("anthropic-version", "2023-06-01"),
            AuthStyle::Google => request.header("x-goog-api-key", secret),
            AuthStyle::None => request,
        };
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
    parse_models(spec.id, body)
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

    if callback_url.path() != OAUTH_LOOPBACK_PATH {
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

    let response = client
        .post(&session.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", session.client_id.as_str()),
            ("code", code.trim()),
            ("redirect_uri", session.redirect_uri.as_str()),
            ("code_verifier", session.verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|_| "OAuth token service could not be reached".to_owned())?;

    if !response.status().is_success() {
        return Err("OAuth provider rejected the authorization code".to_owned());
    }

    let mut token: Value = response
        .json()
        .await
        .map_err(|_| "OAuth provider returned an invalid token".to_owned())?;

    token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth response did not contain an access token".to_owned())?;

    let expires_at = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .map(|seconds| (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339());

    token
        .as_object_mut()
        .ok_or_else(|| "OAuth provider returned an invalid token".to_owned())?
        .insert(
            "_aios".to_owned(),
            serde_json::json!({
                "clientId": session.client_id,
                "tokenUrl": session.token_url,
                "expiresAt": expires_at.clone(),
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

async fn run_oauth_loopback_listener(
    app: AppHandle,
    listener: TcpListener,
    provider_id: String,
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
                        "AI-OS could not complete the account connection.",
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

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| "AI-OS could not open a local OAuth callback listener".to_owned())?;

    let address = listener
        .local_addr()
        .map_err(|_| "AI-OS could not determine the OAuth callback address".to_owned())?;

    let redirect_uri = format!("http://127.0.0.1:{}{}", address.port(), OAUTH_LOOPBACK_PATH);

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
        token_url: input.token_url,
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

    authorization_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &session.client_id)
        .append_pair("redirect_uri", &session.redirect_uri)
        .append_pair("scope", &input.scopes.join(" "))
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    tauri::async_runtime::spawn(run_oauth_loopback_listener(
        app,
        listener,
        provider_id.to_owned(),
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
        Some(read_current_access_token(instance_id).await?)
    };

    let (mut request, body) = match provider_id {
        "openai" => (
            client.post("https://api.openai.com/v1/responses"),
            serde_json::json!({"model": model_id, "input": prompt}),
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
            client.post("https://api.x.ai/v1/chat/completions"),
            serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        "deepseek" => (
            client.post("https://api.deepseek.com/v1/chat/completions"),
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

    if let Some(secret) = secret {
        request = match spec.auth {
            AuthStyle::Bearer => request.bearer_auth(secret),
            AuthStyle::Anthropic => request
                .header("x-api-key", secret)
                .header("anthropic-version", "2023-06-01"),
            AuthStyle::Google => request.header("x-goog-api-key", secret),
            AuthStyle::None => request,
        };
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

fn stream_text(provider_id: &str, value: &Value) -> Option<String> {
    match provider_id {
        "openai" => value
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
            !matches!(message.role.as_str(), "user" | "assistant")
                || message.content.trim().is_empty()
        })
    {
        return Err("conversation contains invalid messages".to_owned());
    }
    let messages = &input.messages;
    let spec = adapter_spec(provider_id)?;
    let token = CancellationToken::new();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| "AI-OS could not initialize AI Center".to_owned())?;
    let secret = if matches!(spec.auth, AuthStyle::None) {
        None
    } else {
        Some(read_current_access_token(instance_id).await?)
    };
    let (mut request, body) = match provider_id {
        "openai" => (
            client.post("https://api.openai.com/v1/responses"),
            serde_json::json!({"model": model_id, "input": messages, "stream": true}),
        ),
        "anthropic" => (
            client.post("https://api.anthropic.com/v1/messages"),
            serde_json::json!({
                "model": model_id, "max_tokens": 2048, "stream": true,
                "messages": messages
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
            client.post("https://api.x.ai/v1/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "deepseek" => (
            client.post("https://api.deepseek.com/v1/chat/completions"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        "ollama" => (
            client.post("http://127.0.0.1:11434/api/chat"),
            serde_json::json!({
                "model": model_id, "stream": true,
                "messages": messages
            }),
        ),
        _ => return Err("this Provider cannot stream through AI Center yet".to_owned()),
    };
    if let Some(secret) = secret {
        request = match spec.auth {
            AuthStyle::Bearer => request.bearer_auth(secret),
            AuthStyle::Anthropic => request
                .header("x-api-key", secret)
                .header("anthropic-version", "2023-06-01"),
            AuthStyle::Google => request.header("x-goog-api-key", secret),
            AuthStyle::None => request,
        };
    }
    ai_center_requests()
        .lock()
        .map_err(|_| "AI Center request state is unavailable".to_owned())?
        .insert(operation_id.clone(), token.clone());

    let response = tokio::select! {
        _ = token.cancelled() => {
            ai_center_requests().lock().ok().map(|mut map| map.remove(&operation_id));
            let _ = app.emit("ai-center://done", AiCenterDoneEvent {
                operation_id: operation_id.clone(), cancelled: true
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
            Err(_) => {
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
                if let Some(text) = stream_text(provider_id, &value) {
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

        let ollama = parse_models(
            "ollama",
            serde_json::json!({"models": [{"name": "qwen:test"}]}),
        )
        .unwrap();
        assert_eq!(ollama[0].id, "qwen:test");
    }

    #[test]
    fn pkce_rejects_insecure_provider_endpoints() {
        assert!(validate_https_url("http://example.com/oauth", "OAuth URL").is_err());
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
                &serde_json::json!({
                    "type": "response.output_text.delta",
                    "delta": "A"
                })
            )
            .as_deref(),
            Some("A")
        );
        assert_eq!(
            stream_text("ollama", &serde_json::json!({"message": {"content": "B"}})).as_deref(),
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
        assert!(value["credentialKinds"].is_array());
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
            "ollama",
        ] {
            let descriptor = get_provider_adapter(provider_id.to_owned()).unwrap();
            let native = adapter_spec(provider_id).unwrap();

            assert_eq!(descriptor.provider_id, native.id);
            assert_eq!(descriptor.adapter_kind, ProviderAdapterKind::Native);
        }
    }

    #[test]
    fn catalog_provider_is_visible_but_not_claimed_as_native() {
        let descriptor = get_provider_adapter("kimi".to_owned()).unwrap();

        assert_eq!(descriptor.adapter_kind, ProviderAdapterKind::Catalog);
        assert!(adapter_spec("kimi").is_err());
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
        assert!(!openai.supports_multiple_credentials);

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
            token_url: "https://example.com/token".to_owned(),
            redirect_uri: "http://127.0.0.1/callback".to_owned(),
            verifier: "verifier".to_owned(),
            created_at: now,
            expires_at: now + 60,
        };

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
            token_url: "https://example.com/token".to_owned(),
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
            token_url: "https://example.com/token".to_owned(),
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
            token_url: "https://example.com/token".to_owned(),
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
    fn oauth_loopback_parser_accepts_matching_callback() {
        let request = "GET /oauth/callback?code=abc123&state=expected HTTP/1.1\r\n\
Host: 127.0.0.1\r\n\r\n";

        assert_eq!(
            parse_oauth_loopback_request(request, "expected").unwrap(),
            "abc123"
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
