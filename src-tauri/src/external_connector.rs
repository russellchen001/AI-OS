use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::Manager;

const CONTRACT_VERSION: &str = "1";
const SENSITIVE_FIELDS: &[&str] = &[
    "clientsecret",
    "certid",
    "accesstoken",
    "refreshtoken",
    "password",
    "cookie",
    "sessiontoken",
    "apisecret",
    "privatekey",
    "authorizationcode",
    "token",
    "secret",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ConnectorSource {
    BuiltIn,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ProviderKind {
    LocalApplication,
    WebsiteLogin,
    ExternalApiConnector,
    UnsupportedRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ConnectorState {
    NotConfigured,
    ConfigurationInvalid,
    ConfigurationValidating,
    Disconnected,
    Connecting,
    WaitingForUser,
    Connected,
    Expired,
    LoginRequired,
    AuthorizationRequired,
    DeveloperApprovalRequired,
    BackendBrokerRequired,
    CapabilityPartiallyAvailable,
    VerificationAdapterRequired,
    AdapterRequired,
    UntrustedConnector,
    AppNotInstalled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ConnectorEnvironment {
    Sandbox,
    Production,
    Development,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicConfigurationField {
    name: String,
    label: String,
    required: bool,
    allowed_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConnectorCapabilityDefinition {
    capability_id: String,
    required_scopes: Vec<String>,
    confirmation_required: bool,
    approval_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConnectorManifest {
    connector_id: String,
    display_name: String,
    connector_version: String,
    interface_kind: String,
    environments: Vec<ConnectorEnvironment>,
    public_configuration_schema: Vec<PublicConfigurationField>,
    broker_contract_version: String,
    authorization_kind: String,
    authorization_start_mode: String,
    official_authorization_hosts: Vec<String>,
    callback_strategy: String,
    capability_definitions: Vec<ConnectorCapabilityDefinition>,
    required_scopes: Vec<String>,
    approval_requirements: Vec<String>,
    confirmation_policy: String,
    disconnect_policy: String,
    health_check_policy: String,
    error_mapping: BTreeMap<String, String>,
    sensitive_field_denylist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConnectorCapabilityState {
    capability_id: String,
    available: bool,
    environment: ConnectorEnvironment,
    authorization_state: String,
    approval_state: String,
    required_scopes: Vec<String>,
    confirmation_required: bool,
    unavailable_reason: Option<String>,
    last_verified_at: Option<String>,
    evidence_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CustomConnectionProvider {
    provider_instance_id: String,
    provider_definition_id: String,
    source: ConnectorSource,
    provider_kind: ProviderKind,
    display_name: String,
    icon_reference: Option<String>,
    environment: ConnectorEnvironment,
    configuration_state: ConnectorState,
    connection_state: ConnectorState,
    authorization_kind: String,
    opaque_authorization_reference: Option<String>,
    pending_authorization_url: Option<String>,
    browser_profile_reference: Option<String>,
    capabilities: Vec<ConnectorCapabilityState>,
    public_configuration: BTreeMap<String, String>,
    official_website_url: Option<String>,
    official_login_url: Option<String>,
    broker_url: Option<String>,
    local_bundle_id: Option<String>,
    local_bundle_path: Option<String>,
    local_bundle_version: Option<String>,
    manifest: Option<ConnectorManifest>,
    created_at: String,
    updated_at: String,
    last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AddCustomProviderInput {
    provider_kind: ProviderKind,
    display_name: String,
    environment: ConnectorEnvironment,
    official_website_url: Option<String>,
    official_login_url: Option<String>,
    broker_url: Option<String>,
    application_path: Option<String>,
    requested_capabilities: Vec<String>,
    public_configuration: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfigureBuiltinEbayInput {
    environment: ConnectorEnvironment,
    broker_mode: String,
    client_id: String,
    ru_name: String,
    broker_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrokerEnvelope {
    connector_id: String,
    contract_version: String,
    environment: ConnectorEnvironment,
    authorization_reference: Option<String>,
    authorization_url: Option<String>,
    granted_scopes: Vec<String>,
    approval_status: String,
    capabilities: Vec<ConnectorCapabilityState>,
    identity_validated: bool,
    error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisconnectResult {
    local_cleanup_complete: bool,
    remote_revoke_complete: bool,
    connection_state: ConnectorState,
    message: String,
}

fn providers_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("custom-connection-providers.json"))
        .map_err(|_| "Custom Provider storage is unavailable".to_owned())
}

fn read_at(path: &Path) -> Result<Vec<CustomConnectionProvider>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)
        .map_err(|_| "Custom Provider configuration could not be read".to_owned())?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "Custom Provider configuration is invalid".to_owned())
}

fn write_at(path: &Path, providers: &[CustomConnectionProvider]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Custom Provider storage is unavailable".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "Custom Provider storage could not be created".to_owned())?;
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(providers)
        .map_err(|_| "Custom Provider configuration could not be encoded".to_owned())?;
    std::fs::write(&temp, bytes)
        .map_err(|_| "Custom Provider configuration could not be saved".to_owned())?;
    std::fs::rename(temp, path)
        .map_err(|_| "Custom Provider configuration could not be committed".to_owned())
}

fn contains_sensitive_name(name: &str) -> bool {
    let normalized = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    SENSITIVE_FIELDS
        .iter()
        .any(|field| normalized.contains(field))
}

fn reject_sensitive_value(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if contains_sensitive_name(key) {
                    return Err(format!("Sensitive field is forbidden: {key}"));
                }
                reject_sensitive_value(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                reject_sensitive_value(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_url(value: &str, environment: &ConnectorEnvironment) -> Result<url::Url, String> {
    let parsed = url::Url::parse(value).map_err(|_| "Connector URL is invalid".to_owned())?;
    let localhost = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1"));
    if parsed.scheme() != "https"
        && !(matches!(
            environment,
            ConnectorEnvironment::Development | ConnectorEnvironment::Sandbox
        ) && parsed.scheme() == "http"
            && localhost)
    {
        return Err(
            "Connector URL must use HTTPS; HTTP is limited to localhost development".to_owned(),
        );
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Connector URL must not contain credentials".to_owned());
    }
    Ok(parsed)
}

fn validate_manifest(manifest: &ConnectorManifest) -> Result<(), String> {
    if manifest.connector_id.trim().is_empty()
        || manifest.connector_version.trim().is_empty()
        || manifest.broker_contract_version != CONTRACT_VERSION
    {
        return Err("Connector manifest identity or contract version is invalid".to_owned());
    }
    if manifest.environments.is_empty()
        || manifest.official_authorization_hosts.is_empty()
        || manifest.capability_definitions.is_empty()
    {
        return Err("Connector manifest is incomplete".to_owned());
    }
    let mut fields = BTreeSet::new();
    for field in &manifest.public_configuration_schema {
        if contains_sensitive_name(&field.name) || !fields.insert(field.name.to_ascii_lowercase()) {
            return Err(
                "Connector public configuration schema contains a forbidden or duplicate field"
                    .to_owned(),
            );
        }
    }
    if manifest
        .sensitive_field_denylist
        .iter()
        .any(|field| !contains_sensitive_name(field))
    {
        return Err("Connector sensitive field denylist is incomplete".to_owned());
    }
    for host in &manifest.official_authorization_hosts {
        if host.contains('/') || host.contains(':') || host.trim().is_empty() {
            return Err("Connector authorization host allowlist is invalid".to_owned());
        }
    }
    Ok(())
}

fn validate_public_configuration(
    manifest: &ConnectorManifest,
    configuration: &BTreeMap<String, String>,
) -> Result<(), String> {
    for key in configuration.keys() {
        if contains_sensitive_name(key) {
            return Err(format!("Sensitive configuration is forbidden: {key}"));
        }
    }
    for field in &manifest.public_configuration_schema {
        let value = configuration
            .get(&field.name)
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if field.required && value.is_empty() {
            return Err(format!(
                "Required public configuration is missing: {}",
                field.label
            ));
        }
        if !value.is_empty()
            && !field.allowed_values.is_empty()
            && !field
                .allowed_values
                .iter()
                .any(|candidate| candidate == value)
        {
            return Err(format!(
                "Public configuration value is invalid: {}",
                field.label
            ));
        }
    }
    Ok(())
}

fn capability_may_execute(
    capability: &ConnectorCapabilityState,
    confirmed: bool,
) -> Result<(), String> {
    if !capability.available {
        return Err("Connector capability is not currently available".to_owned());
    }
    if capability.confirmation_required && !confirmed {
        return Err("User Confirmation is required for this Connector capability".to_owned());
    }
    Ok(())
}

fn ebay_manifest() -> ConnectorManifest {
    let capability = |id: &str, approval: bool, confirmation: bool| ConnectorCapabilityDefinition {
        capability_id: id.to_owned(),
        required_scopes: vec!["buy.api".to_owned()],
        confirmation_required: confirmation,
        approval_required: approval,
    };
    ConnectorManifest {
        connector_id: "ebay-buy".to_owned(),
        display_name: "eBay".to_owned(),
        connector_version: "1.0.0".to_owned(),
        interface_kind: "BACKEND_BROKER".to_owned(),
        environments: vec![
            ConnectorEnvironment::Sandbox,
            ConnectorEnvironment::Production,
        ],
        public_configuration_schema: vec![
            PublicConfigurationField {
                name: "environment".to_owned(),
                label: "Environment".to_owned(),
                required: true,
                allowed_values: vec!["SANDBOX".to_owned(), "PRODUCTION".to_owned()],
            },
            PublicConfigurationField {
                name: "clientId".to_owned(),
                label: "App ID / Client ID".to_owned(),
                required: true,
                allowed_values: vec![],
            },
            PublicConfigurationField {
                name: "ruName".to_owned(),
                label: "RuName".to_owned(),
                required: true,
                allowed_values: vec![],
            },
        ],
        broker_contract_version: CONTRACT_VERSION.to_owned(),
        authorization_kind: "OAUTH_VIA_BROKER".to_owned(),
        authorization_start_mode: "BROKER".to_owned(),
        official_authorization_hosts: vec![
            "auth.ebay.com".to_owned(),
            "auth.sandbox.ebay.com".to_owned(),
        ],
        callback_strategy: "EBAY_RUNAME".to_owned(),
        capability_definitions: vec![
            capability("ebay.browse.search", false, false),
            capability("ebay.browse.item.read", false, false),
            capability("ebay.cart.read", true, false),
            capability("ebay.cart.add", true, false),
            capability("ebay.cart.update", true, false),
            capability("ebay.cart.remove", true, false),
            capability("ebay.checkout.prepare", true, false),
            capability("ebay.checkout.confirm", true, true),
            capability("ebay.order.list", true, false),
            capability("ebay.order.read", true, false),
        ],
        required_scopes: vec!["buy.api".to_owned()],
        approval_requirements: vec!["Production Buy API approval".to_owned()],
        confirmation_policy: "MANIFEST_CAPABILITY".to_owned(),
        disconnect_policy: "BROKER_REVOKE_THEN_LOCAL".to_owned(),
        health_check_policy: "IDENTITY_REQUIRED".to_owned(),
        error_mapping: BTreeMap::new(),
        sensitive_field_denylist: SENSITIVE_FIELDS.iter().map(|v| (*v).to_owned()).collect(),
    }
}

fn builtin_manifest(id: &str) -> Option<ConnectorManifest> {
    match id {
        "ebay-buy" => Some(ebay_manifest()),
        _ => None,
    }
}

async fn discover_trusted_manifest(
    broker_url: &str,
    environment: &ConnectorEnvironment,
    expected: &ConnectorManifest,
) -> Result<ConnectorManifest, String> {
    let base = validate_url(broker_url, environment)?;
    let url = base
        .join("connectors/ebay-buy/manifest")
        .map_err(|_| "Backend Broker discovery endpoint is invalid".to_owned())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| "Backend Broker discovery client could not start".to_owned())?;
    let response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() {
            "Backend Broker discovery timed out".to_owned()
        } else {
            "Backend Broker discovery could not be reached".to_owned()
        }
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "Backend Broker discovery failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Backend Broker discovery manifest schema is invalid".to_owned())?;
    reject_sensitive_value(&value)?;
    let manifest: ConnectorManifest = serde_json::from_value(value)
        .map_err(|_| "Backend Broker discovery manifest schema is invalid".to_owned())?;
    validate_manifest(&manifest)?;
    if &manifest != expected {
        return Err(
            "Broker discovery manifest is not the reviewed AI-OS Connector manifest".to_owned(),
        );
    }
    Ok(manifest)
}

fn inspect_application(path: &str) -> Result<(String, String, String, String), String> {
    let supplied = Path::new(path);
    if supplied.extension().and_then(|value| value.to_str()) != Some("app") {
        return Err("Local Application selection must be an .app bundle".to_owned());
    }
    if std::fs::symlink_metadata(supplied)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Local Application symlinks are not accepted".to_owned());
    }
    let canonical = std::fs::canonicalize(supplied)
        .map_err(|_| "Local Application bundle does not exist".to_owned())?;
    if canonical.extension().and_then(|value| value.to_str()) != Some("app")
        || !canonical.join("Contents/Info.plist").is_file()
    {
        return Err("Local Application bundle is invalid".to_owned());
    }
    let read = |key: &str| -> Result<String, String> {
        let output = std::process::Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}")])
            .arg(canonical.join("Contents/Info.plist"))
            .output()
            .map_err(|_| "Application metadata could not be read".to_owned())?;
        if !output.status.success() {
            return Err(format!("Application metadata is missing: {key}"));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let bundle_id = read("CFBundleIdentifier")?;
    let display = read("CFBundleDisplayName").or_else(|_| read("CFBundleName"))?;
    let version = read("CFBundleShortVersionString").unwrap_or_else(|_| "unknown".to_owned());
    if bundle_id.trim().is_empty() || display.trim().is_empty() {
        return Err("Application bundle identity is invalid".to_owned());
    }
    Ok((
        canonical.to_string_lossy().into_owned(),
        bundle_id,
        display,
        version,
    ))
}

fn find_bundle(bundle_id: &str) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            let candidate = entry.path();
            if candidate.extension().and_then(|v| v.to_str()) != Some("app") {
                continue;
            }
            if inspect_application(candidate.to_string_lossy().as_ref())
                .ok()
                .is_some_and(|(_, id, _, _)| id == bundle_id)
            {
                return Some(candidate);
            }
        }
    }
    None
}

async fn broker_call(
    provider: &CustomConnectionProvider,
    operation: &str,
    body: Option<Value>,
) -> Result<BrokerEnvelope, String> {
    let base = provider
        .broker_url
        .as_deref()
        .ok_or_else(|| "Backend Broker URL is required".to_owned())?;
    let base = validate_url(base, &provider.environment)?;
    let manifest = provider
        .manifest
        .as_ref()
        .ok_or_else(|| "Connector manifest is unavailable".to_owned())?;
    let endpoint = if operation == "health" {
        "health".to_owned()
    } else {
        format!("connectors/{}/{operation}", manifest.connector_id)
    };
    let mut url = base
        .join(&endpoint)
        .map_err(|_| "Backend Broker endpoint is invalid".to_owned())?;
    if matches!(operation, "authorization/status" | "capabilities") {
        let reference = provider
            .opaque_authorization_reference
            .as_deref()
            .ok_or_else(|| "Authorization reference is required".to_owned())?;
        url.query_pairs_mut()
            .append_pair("authorizationReference", reference);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| "Backend Broker client could not start".to_owned())?;
    let request = if let Some(value) = body {
        client.post(url).json(&value)
    } else {
        client.get(url)
    };
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "Backend Broker timed out".to_owned()
        } else {
            "Backend Broker could not be reached".to_owned()
        }
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "Backend Broker request failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Backend Broker response schema is invalid".to_owned())?;
    reject_sensitive_value(&value)?;
    let envelope: BrokerEnvelope = serde_json::from_value(value)
        .map_err(|_| "Backend Broker response schema is invalid".to_owned())?;
    if envelope.connector_id != manifest.connector_id
        || envelope.contract_version != manifest.broker_contract_version
        || envelope.environment != provider.environment
    {
        return Err("Backend Broker identity, contract, or environment does not match".to_owned());
    }
    if let Some(reference) = &envelope.authorization_reference {
        let expected_prefix = format!(
            "{}:{}:",
            manifest.connector_id,
            match provider.environment {
                ConnectorEnvironment::Sandbox => "sandbox",
                ConnectorEnvironment::Production => "production",
                ConnectorEnvironment::Development => "development",
            }
        );
        if !reference.starts_with(&expected_prefix) {
            return Err(
                "Authorization reference is not isolated to this Connector environment".to_owned(),
            );
        }
    }
    if let Some(url) = &envelope.authorization_url {
        let parsed = validate_url(url, &provider.environment)?;
        if !manifest
            .official_authorization_hosts
            .iter()
            .any(|host| parsed.host_str() == Some(host))
        {
            return Err(
                "Authorization URL host is not approved by the Connector manifest".to_owned(),
            );
        }
    }
    Ok(envelope)
}

#[tauri::command]
pub(crate) fn list_custom_connection_providers(
    app: tauri::AppHandle,
) -> Result<Vec<CustomConnectionProvider>, String> {
    Ok(read_at(&providers_path(&app)?)?
        .into_iter()
        .filter(|provider| {
            provider.source == ConnectorSource::Custom
                && provider
                    .manifest
                    .as_ref()
                    .is_none_or(|manifest| manifest.connector_id != "ebay-buy")
        })
        .collect())
}

fn new_builtin_ebay() -> CustomConnectionProvider {
    let now = chrono::Utc::now().to_rfc3339();
    CustomConnectionProvider {
        provider_instance_id: "builtin-ebay-default".to_owned(),
        provider_definition_id: "ebay-buy".to_owned(),
        source: ConnectorSource::BuiltIn,
        provider_kind: ProviderKind::ExternalApiConnector,
        display_name: "eBay".to_owned(),
        icon_reference: None,
        environment: ConnectorEnvironment::Sandbox,
        configuration_state: ConnectorState::NotConfigured,
        connection_state: ConnectorState::NotConfigured,
        authorization_kind: "OAUTH_VIA_BROKER".to_owned(),
        opaque_authorization_reference: None,
        pending_authorization_url: None,
        browser_profile_reference: None,
        capabilities: vec![],
        public_configuration: BTreeMap::from([
            ("connectorId".to_owned(), "ebay-buy".to_owned()),
            ("brokerMode".to_owned(), "MANAGED".to_owned()),
        ]),
        official_website_url: Some("https://www.ebay.com".to_owned()),
        official_login_url: None,
        broker_url: None,
        local_bundle_id: None,
        local_bundle_path: None,
        local_bundle_version: None,
        manifest: Some(ebay_manifest()),
        created_at: now.clone(),
        updated_at: now,
        last_checked_at: None,
    }
}

fn load_or_migrate_builtin_ebay(path: &Path) -> Result<CustomConnectionProvider, String> {
    let mut providers = read_at(path)?;
    if let Some(existing) = providers
        .iter()
        .find(|provider| provider.provider_instance_id == "builtin-ebay-default")
        .cloned()
    {
        return Ok(existing);
    }
    let legacy_index = providers.iter().position(|provider| {
        provider.source == ConnectorSource::Custom
            && provider
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.connector_id == "ebay-buy")
    });
    let mut builtin = new_builtin_ebay();
    if let Some(index) = legacy_index {
        let legacy = providers.remove(index);
        builtin.environment = legacy.environment.clone();
        builtin.public_configuration = legacy.public_configuration;
        builtin.broker_url = legacy.broker_url;
        builtin.public_configuration.insert(
            "brokerMode".to_owned(),
            if builtin.broker_url.is_some() {
                "SELF_HOSTED"
            } else {
                "MANAGED"
            }
            .to_owned(),
        );
        builtin.configuration_state = legacy.configuration_state;
        builtin.connection_state = legacy.connection_state;
        builtin.capabilities = legacy.capabilities;
        let reference_matches =
            legacy
                .opaque_authorization_reference
                .as_deref()
                .is_none_or(|reference| {
                    let environment = match legacy.environment {
                        ConnectorEnvironment::Sandbox => "sandbox",
                        ConnectorEnvironment::Production => "production",
                        ConnectorEnvironment::Development => "development",
                    };
                    reference.starts_with(&format!("ebay-buy:{environment}:"))
                });
        if reference_matches {
            builtin.opaque_authorization_reference = legacy.opaque_authorization_reference;
        } else {
            builtin.connection_state = ConnectorState::ConfigurationInvalid;
            builtin.public_configuration.insert(
                "migrationStatus".to_owned(),
                "MIGRATION_REQUIRED".to_owned(),
            );
        }
    }
    providers.push(builtin.clone());
    write_at(path, &providers)?;
    Ok(builtin)
}

#[tauri::command]
pub(crate) fn get_builtin_ebay_connection(
    app: tauri::AppHandle,
) -> Result<CustomConnectionProvider, String> {
    load_or_migrate_builtin_ebay(&providers_path(&app)?)
}

#[tauri::command]
pub(crate) async fn configure_builtin_ebay_connection(
    app: tauri::AppHandle,
    input: ConfigureBuiltinEbayInput,
) -> Result<CustomConnectionProvider, String> {
    let path = providers_path(&app)?;
    let mut builtin = load_or_migrate_builtin_ebay(&path)?;
    let mode = input.broker_mode.trim().to_ascii_uppercase();
    if !matches!(mode.as_str(), "MANAGED" | "SELF_HOSTED") {
        return Err("eBay Broker mode is invalid".to_owned());
    }
    builtin.environment = input.environment.clone();
    builtin.public_configuration = BTreeMap::from([
        ("connectorId".to_owned(), "ebay-buy".to_owned()),
        ("brokerMode".to_owned(), mode.clone()),
        (
            "environment".to_owned(),
            match input.environment {
                ConnectorEnvironment::Sandbox => "SANDBOX",
                ConnectorEnvironment::Production => "PRODUCTION",
                ConnectorEnvironment::Development => "DEVELOPMENT",
            }
            .to_owned(),
        ),
    ]);
    builtin.broker_url = if mode == "MANAGED" {
        std::env::var("AI_OS_MANAGED_EBAY_BROKER_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
    } else {
        if input.client_id.trim().is_empty() || input.ru_name.trim().is_empty() {
            return Err("Self-hosted eBay requires public App ID and RuName".to_owned());
        }
        builtin
            .public_configuration
            .insert("clientId".to_owned(), input.client_id.trim().to_owned());
        builtin
            .public_configuration
            .insert("ruName".to_owned(), input.ru_name.trim().to_owned());
        input.broker_url.filter(|value| !value.trim().is_empty())
    };
    let Some(broker_url) = builtin.broker_url.clone() else {
        builtin.configuration_state = ConnectorState::NotConfigured;
        builtin.connection_state = ConnectorState::BackendBrokerRequired;
        builtin.public_configuration.insert(
            "managedBrokerStatus".to_owned(),
            "NOT_PROVISIONED".to_owned(),
        );
        replace_builtin_ebay(&path, builtin.clone())?;
        return Ok(builtin);
    };
    let reviewed = ebay_manifest();
    let manifest = discover_trusted_manifest(&broker_url, &builtin.environment, &reviewed).await?;
    builtin.manifest = Some(manifest);
    builtin.configuration_state = ConnectorState::ConfigurationValidating;
    replace_builtin_ebay(&path, builtin.clone())?;
    broker_call(&builtin, "health", None).await?;
    let validated = broker_call(
        &builtin,
        "configuration/validate",
        Some(serde_json::json!({
            "publicConfiguration": builtin.public_configuration,
            "environment": builtin.environment,
        })),
    )
    .await?;
    builtin.configuration_state = if validated.error_code.is_some() {
        ConnectorState::ConfigurationInvalid
    } else {
        ConnectorState::Disconnected
    };
    builtin.connection_state = builtin.configuration_state.clone();
    builtin.capabilities = validated.capabilities;
    builtin.updated_at = chrono::Utc::now().to_rfc3339();
    replace_builtin_ebay(&path, builtin.clone())?;
    Ok(builtin)
}

fn replace_builtin_ebay(path: &Path, builtin: CustomConnectionProvider) -> Result<(), String> {
    let mut providers = read_at(path)?;
    providers.retain(|provider| provider.provider_instance_id != "builtin-ebay-default");
    providers.push(builtin);
    write_at(path, &providers)
}

#[tauri::command]
pub(crate) async fn add_custom_connection_provider(
    app: tauri::AppHandle,
    input: AddCustomProviderInput,
) -> Result<CustomConnectionProvider, String> {
    if input.display_name.trim().is_empty() {
        return Err("Provider display name is required".to_owned());
    }
    for key in input.public_configuration.keys() {
        if contains_sensitive_name(key) {
            return Err(format!("Sensitive configuration is forbidden: {key}"));
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("custom-{}", uuid::Uuid::new_v4().simple());
    let mut provider = CustomConnectionProvider {
        provider_instance_id: id.clone(),
        provider_definition_id: id,
        source: ConnectorSource::Custom,
        provider_kind: input.provider_kind.clone(),
        display_name: input.display_name.trim().to_owned(),
        icon_reference: None,
        environment: input.environment.clone(),
        configuration_state: ConnectorState::NotConfigured,
        connection_state: ConnectorState::Disconnected,
        authorization_kind: "NONE".to_owned(),
        opaque_authorization_reference: None,
        pending_authorization_url: None,
        browser_profile_reference: None,
        capabilities: vec![],
        public_configuration: input.public_configuration,
        official_website_url: input.official_website_url,
        official_login_url: input.official_login_url,
        broker_url: input.broker_url,
        local_bundle_id: None,
        local_bundle_path: None,
        local_bundle_version: None,
        manifest: None,
        created_at: now.clone(),
        updated_at: now,
        last_checked_at: None,
    };
    match provider.provider_kind {
        ProviderKind::LocalApplication => {
            let (path, bundle, display, version) = inspect_application(
                input
                    .application_path
                    .as_deref()
                    .ok_or_else(|| "Application selection is required".to_owned())?,
            )?;
            provider.display_name = display;
            provider.local_bundle_path = Some(path);
            provider.local_bundle_id = Some(bundle);
            provider.local_bundle_version = Some(version);
            provider.configuration_state = ConnectorState::Disconnected;
            provider.connection_state = ConnectorState::AuthorizationRequired;
            provider.authorization_kind = "SYSTEM_PERMISSION".to_owned();
            provider.capabilities = ["automation", "files", "accessibility"]
                .into_iter()
                .map(|permission| ConnectorCapabilityState {
                    capability_id: format!("local.permission.{permission}"),
                    available: false,
                    environment: provider.environment.clone(),
                    authorization_state: "NOT_DETERMINED".to_owned(),
                    approval_state: "NOT_REQUIRED".to_owned(),
                    required_scopes: vec![],
                    confirmation_required: false,
                    unavailable_reason: Some("System permission has not been verified".to_owned()),
                    last_verified_at: None,
                    evidence_reference: None,
                })
                .collect();
        }
        ProviderKind::WebsiteLogin => {
            let website = validate_url(
                provider
                    .official_website_url
                    .as_deref()
                    .ok_or_else(|| "Official website URL is required".to_owned())?,
                &provider.environment,
            )?;
            let login = validate_url(
                provider
                    .official_login_url
                    .as_deref()
                    .ok_or_else(|| "Official login URL is required".to_owned())?,
                &provider.environment,
            )?;
            if website.origin() != login.origin() {
                return Err("Official login URL must use the declared website origin".to_owned());
            }
            provider.configuration_state = ConnectorState::Disconnected;
            provider.connection_state = ConnectorState::VerificationAdapterRequired;
            provider.authorization_kind = "AUTHENTICATED_BROWSER".to_owned();
            provider.browser_profile_reference = Some(format!(
                "browser-profile:{}:{}",
                provider.provider_definition_id,
                match provider.environment {
                    ConnectorEnvironment::Sandbox => "sandbox",
                    ConnectorEnvironment::Production => "production",
                    ConnectorEnvironment::Development => "development",
                }
            ));
        }
        ProviderKind::ExternalApiConnector => {
            let connector_id = provider
                .public_configuration
                .get("connectorId")
                .ok_or_else(|| "Built-in Connector selection is required".to_owned())?;
            if connector_id == "ebay-buy" {
                return Err("eBay is built in; configure the eBay row in Connections".to_owned());
            }
            let reviewed_manifest = builtin_manifest(connector_id)
                .ok_or_else(|| "Connector manifest is not trusted".to_owned())?;
            validate_manifest(&reviewed_manifest)?;
            let manifest = discover_trusted_manifest(
                provider
                    .broker_url
                    .as_deref()
                    .ok_or_else(|| "Backend Broker URL is required".to_owned())?,
                &provider.environment,
                &reviewed_manifest,
            )
            .await?;
            validate_public_configuration(&manifest, &provider.public_configuration)?;
            validate_url(
                provider
                    .broker_url
                    .as_deref()
                    .ok_or_else(|| "Backend Broker URL is required".to_owned())?,
                &provider.environment,
            )?;
            provider.manifest = Some(manifest.clone());
            provider.authorization_kind = manifest.authorization_kind.clone();
            provider.configuration_state = ConnectorState::Disconnected;
            provider.connection_state = ConnectorState::BackendBrokerRequired;
            provider.capabilities = manifest
                .capability_definitions
                .iter()
                .map(|cap| ConnectorCapabilityState {
                    capability_id: cap.capability_id.clone(),
                    available: false,
                    environment: provider.environment.clone(),
                    authorization_state: "DISCONNECTED".to_owned(),
                    approval_state: if cap.approval_required {
                        "REQUIRED"
                    } else {
                        "NOT_REQUIRED"
                    }
                    .to_owned(),
                    required_scopes: cap.required_scopes.clone(),
                    confirmation_required: cap.confirmation_required,
                    unavailable_reason: Some("Authorization required".to_owned()),
                    last_verified_at: None,
                    evidence_reference: None,
                })
                .collect();
        }
        ProviderKind::UnsupportedRequest => {
            if input.requested_capabilities.is_empty() {
                return Err("Requested Provider capabilities are required".to_owned());
            }
            provider.configuration_state = ConnectorState::Disconnected;
            provider.connection_state = ConnectorState::AdapterRequired;
        }
    }
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    providers.push(provider.clone());
    write_at(&path, &providers)?;
    Ok(provider)
}

#[tauri::command]
pub(crate) async fn connect_custom_connection_provider(
    app: tauri::AppHandle,
    provider_instance_id: String,
) -> Result<CustomConnectionProvider, String> {
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    let index = providers
        .iter()
        .position(|p| p.provider_instance_id == provider_instance_id)
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    match providers[index].provider_kind {
        ProviderKind::WebsiteLogin => {
            providers[index].connection_state = ConnectorState::WaitingForUser
        }
        ProviderKind::LocalApplication => {
            providers[index].connection_state = ConnectorState::AuthorizationRequired
        }
        ProviderKind::UnsupportedRequest => {
            providers[index].connection_state = ConnectorState::AdapterRequired
        }
        ProviderKind::ExternalApiConnector => {
            providers[index].connection_state = ConnectorState::Connecting;
            let envelope = broker_call(&providers[index], "authorization/begin", Some(serde_json::json!({ "publicConfiguration": providers[index].public_configuration, "environment": providers[index].environment }))).await?;
            providers[index].opaque_authorization_reference = envelope.authorization_reference;
            providers[index].pending_authorization_url = envelope.authorization_url;
            providers[index].connection_state = if envelope.identity_validated {
                ConnectorState::Connected
            } else {
                ConnectorState::WaitingForUser
            };
            providers[index].capabilities = envelope.capabilities;
        }
    }
    providers[index].updated_at = chrono::Utc::now().to_rfc3339();
    let result = providers[index].clone();
    write_at(&path, &providers)?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn test_custom_connection_provider(
    app: tauri::AppHandle,
    provider_instance_id: String,
) -> Result<CustomConnectionProvider, String> {
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    let index = providers
        .iter()
        .position(|provider| provider.provider_instance_id == provider_instance_id)
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    if !matches!(
        providers[index].provider_kind,
        ProviderKind::ExternalApiConnector
    ) {
        return Err(
            "Only External API Connectors use Backend Broker configuration tests".to_owned(),
        );
    }
    providers[index].configuration_state = ConnectorState::ConfigurationValidating;
    broker_call(&providers[index], "health", None).await?;
    let validated = broker_call(
        &providers[index],
        "configuration/validate",
        Some(serde_json::json!({
            "publicConfiguration": providers[index].public_configuration,
            "environment": providers[index].environment,
        })),
    )
    .await?;
    if validated.error_code.is_some() {
        providers[index].configuration_state = ConnectorState::ConfigurationInvalid;
        write_at(&path, &providers)?;
        return Err("Backend Broker rejected the public Connector configuration".to_owned());
    }
    providers[index].configuration_state = ConnectorState::Disconnected;
    providers[index].updated_at = chrono::Utc::now().to_rfc3339();
    let result = providers[index].clone();
    write_at(&path, &providers)?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn refresh_custom_connection_provider(
    app: tauri::AppHandle,
    provider_instance_id: String,
) -> Result<CustomConnectionProvider, String> {
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    let index = providers
        .iter()
        .position(|p| p.provider_instance_id == provider_instance_id)
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    match providers[index].provider_kind {
        ProviderKind::LocalApplication => {
            let found = providers[index]
                .local_bundle_id
                .as_deref()
                .and_then(find_bundle);
            providers[index].local_bundle_path = found
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned());
            providers[index].connection_state = if found.is_some() {
                ConnectorState::AuthorizationRequired
            } else {
                ConnectorState::AppNotInstalled
            };
        }
        ProviderKind::ExternalApiConnector => {
            let authorization =
                broker_call(&providers[index], "authorization/status", None).await?;
            let envelope = broker_call(&providers[index], "capabilities", None).await?;
            providers[index].capabilities = envelope.capabilities;
            providers[index].connection_state =
                if authorization.identity_validated && envelope.identity_validated {
                    if providers[index]
                        .capabilities
                        .iter()
                        .all(|cap| cap.available)
                    {
                        ConnectorState::Connected
                    } else {
                        ConnectorState::CapabilityPartiallyAvailable
                    }
                } else {
                    ConnectorState::LoginRequired
                };
        }
        ProviderKind::WebsiteLogin => {
            providers[index].connection_state = ConnectorState::VerificationAdapterRequired
        }
        ProviderKind::UnsupportedRequest => {
            providers[index].connection_state = ConnectorState::AdapterRequired
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    providers[index].last_checked_at = Some(now.clone());
    providers[index].updated_at = now;
    let result = providers[index].clone();
    write_at(&path, &providers)?;
    Ok(result)
}

#[tauri::command]
pub(crate) async fn execute_external_connector_capability(
    app: tauri::AppHandle,
    provider_instance_id: String,
    capability_id: String,
    confirmed: bool,
) -> Result<String, String> {
    let providers = read_at(&providers_path(&app)?)?;
    let provider = providers
        .iter()
        .find(|provider| provider.provider_instance_id == provider_instance_id)
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    let capability = provider
        .capabilities
        .iter()
        .find(|capability| capability.capability_id == capability_id)
        .ok_or_else(|| {
            "Connector capability is not declared by the reviewed manifest".to_owned()
        })?;
    capability_may_execute(capability, confirmed)?;
    let envelope = broker_call(
        provider,
        "capability/execute",
        Some(serde_json::json!({
            "authorizationReference": provider.opaque_authorization_reference,
            "capabilityId": capability_id,
            "confirmed": confirmed,
        })),
    )
    .await?;
    if let Some(code) = envelope.error_code {
        return Err(format!("Connector capability execution failed: {code}"));
    }
    Ok(
        "Connector capability execution was accepted and validated by the Backend Broker"
            .to_owned(),
    )
}

#[tauri::command]
pub(crate) async fn disconnect_custom_connection_provider(
    app: tauri::AppHandle,
    provider_instance_id: String,
) -> Result<DisconnectResult, String> {
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    let index = providers
        .iter()
        .position(|p| p.provider_instance_id == provider_instance_id)
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    let remote_required = matches!(
        providers[index].provider_kind,
        ProviderKind::ExternalApiConnector
    ) && providers[index].opaque_authorization_reference.is_some();
    let mut remote_revoke_complete = true;
    if remote_required {
        let envelope = broker_call(&providers[index], "authorization/revoke", Some(serde_json::json!({ "authorizationReference": providers[index].opaque_authorization_reference }))).await?;
        if envelope.error_code.as_deref() == Some("REMOTE_REVOKE_UNSUPPORTED_TOKEN_DELETED") {
            remote_revoke_complete = false;
        } else if envelope.error_code.is_some() || envelope.authorization_reference.is_some() {
            return Ok(DisconnectResult {
                local_cleanup_complete: false,
                remote_revoke_complete: false,
                connection_state: providers[index].connection_state.clone(),
                message: "Remote authorization revoke failed; local tracking was retained"
                    .to_owned(),
            });
        }
    }
    providers[index].opaque_authorization_reference = None;
    providers[index].pending_authorization_url = None;
    providers[index].browser_profile_reference = None;
    for capability in &mut providers[index].capabilities {
        capability.available = false;
        capability.authorization_state = "DISCONNECTED".to_owned();
        capability.evidence_reference = None;
    }
    providers[index].connection_state = ConnectorState::Disconnected;
    providers[index].updated_at = chrono::Utc::now().to_rfc3339();
    write_at(&path, &providers)?;
    Ok(DisconnectResult {
        local_cleanup_complete: true,
        remote_revoke_complete,
        connection_state: ConnectorState::Disconnected,
        message: if remote_revoke_complete {
            "Account disconnected and local authorization references removed".to_owned()
        } else {
            "Broker token and desktop authorization reference were deleted; eBay provides no remote token revoke endpoint".to_owned()
        },
    })
}

#[tauri::command]
pub(crate) async fn remove_custom_connection_provider(
    app: tauri::AppHandle,
    provider_instance_id: String,
) -> Result<bool, String> {
    let path = providers_path(&app)?;
    let mut providers = read_at(&path)?;
    let provider = providers
        .iter()
        .find(|p| p.provider_instance_id == provider_instance_id)
        .cloned()
        .ok_or_else(|| "Custom Provider was not found".to_owned())?;
    if provider.source == ConnectorSource::BuiltIn {
        return Err("Built-in Provider definitions cannot be removed".to_owned());
    }
    if !matches!(
        provider.connection_state,
        ConnectorState::Disconnected
            | ConnectorState::NotConfigured
            | ConnectorState::AdapterRequired
            | ConnectorState::VerificationAdapterRequired
            | ConnectorState::AppNotInstalled
    ) {
        return Err("Disconnect Account must complete before removing this Provider".to_owned());
    }
    providers.retain(|p| p.provider_instance_id != provider_instance_id);
    write_at(&path, &providers)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_input(kind: ProviderKind) -> AddCustomProviderInput {
        AddCustomProviderInput {
            provider_kind: kind,
            display_name: "Example".to_owned(),
            environment: ConnectorEnvironment::Development,
            official_website_url: None,
            official_login_url: None,
            broker_url: None,
            application_path: None,
            requested_capabilities: vec![],
            public_configuration: BTreeMap::new(),
        }
    }

    #[test]
    fn manifest_and_all_three_layers_reject_sensitive_configuration() {
        let mut manifest = ebay_manifest();
        validate_manifest(&manifest).unwrap();
        manifest
            .public_configuration_schema
            .push(PublicConfigurationField {
                name: "clientSecret".to_owned(),
                label: "Secret".to_owned(),
                required: true,
                allowed_values: vec![],
            });
        assert!(validate_manifest(&manifest).is_err());
        let mut config = BTreeMap::new();
        config.insert("accessToken".to_owned(), "raw".to_owned());
        assert!(validate_public_configuration(&ebay_manifest(), &config).is_err());
        assert!(
            reject_sensitive_value(&serde_json::json!({"data":{"refreshToken":"raw"}})).is_err()
        );
    }

    #[test]
    fn production_http_and_unapproved_authorization_hosts_are_rejected() {
        assert!(validate_url(
            "http://broker.example.com",
            &ConnectorEnvironment::Production
        )
        .is_err());
        assert!(validate_url("http://localhost:3000", &ConnectorEnvironment::Development).is_ok());
        let manifest = ebay_manifest();
        let authorization = url::Url::parse("https://evil.example/oauth").unwrap();
        assert!(!manifest
            .official_authorization_hosts
            .iter()
            .any(|host| authorization.host_str() == Some(host)));
    }

    #[test]
    fn ebay_capability_approval_and_confirmation_are_independent() {
        let manifest = ebay_manifest();
        let browse = manifest
            .capability_definitions
            .iter()
            .find(|cap| cap.capability_id == "ebay.browse.search")
            .unwrap();
        let checkout = manifest
            .capability_definitions
            .iter()
            .find(|cap| cap.capability_id == "ebay.checkout.confirm")
            .unwrap();
        assert!(!browse.approval_required);
        assert!(checkout.approval_required);
        assert!(checkout.confirmation_required);
    }

    #[test]
    fn persistence_reload_and_atomic_removal_do_not_affect_other_providers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("providers.json");
        let now = chrono::Utc::now().to_rfc3339();
        let provider = |id: &str| CustomConnectionProvider {
            provider_instance_id: id.to_owned(),
            provider_definition_id: id.to_owned(),
            source: ConnectorSource::Custom,
            provider_kind: ProviderKind::UnsupportedRequest,
            display_name: id.to_owned(),
            icon_reference: None,
            environment: ConnectorEnvironment::Sandbox,
            configuration_state: ConnectorState::Disconnected,
            connection_state: ConnectorState::AdapterRequired,
            authorization_kind: "NONE".to_owned(),
            opaque_authorization_reference: None,
            pending_authorization_url: None,
            browser_profile_reference: None,
            capabilities: vec![],
            public_configuration: BTreeMap::new(),
            official_website_url: None,
            official_login_url: None,
            broker_url: None,
            local_bundle_id: None,
            local_bundle_path: None,
            local_bundle_version: None,
            manifest: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            last_checked_at: None,
        };
        write_at(&path, &[provider("one"), provider("two")]).unwrap();
        let mut loaded = read_at(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        loaded.retain(|p| p.provider_instance_id != "one");
        write_at(&path, &loaded).unwrap();
        let restarted = read_at(&path).unwrap();
        assert_eq!(restarted.len(), 1);
        assert_eq!(restarted[0].provider_instance_id, "two");
    }

    #[test]
    fn website_login_never_becomes_connected_without_verification_adapter() {
        let mut input = sample_input(ProviderKind::WebsiteLogin);
        input.official_website_url = Some("https://example.com".to_owned());
        input.official_login_url = Some("https://example.com/login".to_owned());
        let website = validate_url(
            input.official_website_url.as_deref().unwrap(),
            &input.environment,
        )
        .unwrap();
        let login = validate_url(
            input.official_login_url.as_deref().unwrap(),
            &input.environment,
        )
        .unwrap();
        assert_eq!(website.origin(), login.origin());
        assert_ne!(ConnectorState::WaitingForUser, ConnectorState::Connected);
        assert_eq!(
            ConnectorState::VerificationAdapterRequired,
            ConnectorState::VerificationAdapterRequired
        );
    }

    #[tokio::test]
    async fn broker_discovery_requires_the_exact_reviewed_manifest() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let manifest = ebay_manifest();
        let body = serde_json::to_string(&manifest).unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let size = stream.read(&mut request).await.unwrap();
            assert!(String::from_utf8_lossy(&request[..size])
                .starts_with("GET /connectors/ebay-buy/manifest"));
            stream.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        });
        let discovered = discover_trusted_manifest(
            &format!("http://{address}/"),
            &ConnectorEnvironment::Development,
            &manifest,
        )
        .await
        .unwrap();
        assert_eq!(discovered, manifest);
    }

    #[test]
    fn high_risk_capability_cannot_execute_without_confirmation() {
        let capability = ConnectorCapabilityState {
            capability_id: "ebay.checkout.confirm".to_owned(),
            available: true,
            environment: ConnectorEnvironment::Sandbox,
            authorization_state: "GRANTED".to_owned(),
            approval_state: "APPROVED".to_owned(),
            required_scopes: vec!["buy.api".to_owned()],
            confirmation_required: true,
            unavailable_reason: None,
            last_verified_at: None,
            evidence_reference: None,
        };
        assert!(capability_may_execute(&capability, false).is_err());
        assert!(capability_may_execute(&capability, true).is_ok());
    }

    #[test]
    fn builtin_ebay_is_single_non_removable_configuration_required_definition() {
        let ebay = new_builtin_ebay();
        assert_eq!(ebay.source, ConnectorSource::BuiltIn);
        assert_eq!(ebay.provider_instance_id, "builtin-ebay-default");
        assert_eq!(ebay.configuration_state, ConnectorState::NotConfigured);
        assert_eq!(ebay.connection_state, ConnectorState::NotConfigured);
        assert_eq!(ebay.manifest.unwrap().interface_kind, "BACKEND_BROKER");
    }

    #[test]
    fn legacy_custom_ebay_migrates_once_and_unsafe_reference_requires_migration() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("providers.json");
        let mut legacy = new_builtin_ebay();
        legacy.provider_instance_id = "custom-old-ebay".to_owned();
        legacy.provider_definition_id = "custom-old-ebay".to_owned();
        legacy.source = ConnectorSource::Custom;
        legacy.environment = ConnectorEnvironment::Production;
        legacy.opaque_authorization_reference = Some("ebay-buy:sandbox:wrong".to_owned());
        write_at(&path, &[legacy]).unwrap();
        let migrated = load_or_migrate_builtin_ebay(&path).unwrap();
        assert_eq!(migrated.provider_instance_id, "builtin-ebay-default");
        assert!(migrated.opaque_authorization_reference.is_none());
        assert_eq!(
            migrated.connection_state,
            ConnectorState::ConfigurationInvalid
        );
        assert_eq!(
            migrated
                .public_configuration
                .get("migrationStatus")
                .map(String::as_str),
            Some("MIGRATION_REQUIRED")
        );
        let stored = read_at(&path).unwrap();
        assert_eq!(
            stored
                .iter()
                .filter(|provider| provider
                    .manifest
                    .as_ref()
                    .is_some_and(|manifest| manifest.connector_id == "ebay-buy"))
                .count(),
            1
        );
    }
}
