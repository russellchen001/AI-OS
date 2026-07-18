use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeAdapterKind {
    Openclaw,
    Ollama,
    DockerDesktop,
    OpenWebui,
    CherryStudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimePlatform {
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLocation {
    Local,
    Remote,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCapability {
    Discover,
    Health,
    Start,
    Stop,
    Restart,
    Open,
    Progress,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeAvailability {
    Unknown,
    Available,
    Unavailable,
    NotInstalled,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLifecycle {
    Unknown,
    Stopped,
    Starting,
    Running,
    Stopping,
    Restarting,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeHealth {
    Unknown,
    Checking,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeReadiness {
    Unknown,
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeErrorCode {
    AuthenticationRequired,
    PairingRequired,
    ConnectionUnavailable,
    ConfigurationUnavailable,
    InvalidConfiguration,
    ProbeFailed,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedRuntimeError {
    pub code: RuntimeErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDefinition {
    pub id: String,
    pub adapter_kind: RuntimeAdapterKind,
    pub display_key: String,
    pub icon_key: String,
    pub supported_platforms: Vec<RuntimePlatform>,
    pub location: RuntimeLocation,
    pub dependencies: Vec<String>,
    pub capabilities: Vec<RuntimeCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub id: String,
    pub adapter_kind: RuntimeAdapterKind,
    pub supported_platform: RuntimePlatform,
    pub location: RuntimeLocation,
    pub dependencies: Vec<String>,
    pub capabilities: Vec<RuntimeCapability>,
    pub availability: RuntimeAvailability,
    pub lifecycle: RuntimeLifecycle,
    pub health: RuntimeHealth,
    pub readiness: RuntimeReadiness,
    pub observed_at: String,
    pub error: Option<NormalizedRuntimeError>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusRequest {
    pub ollama_url: Option<String>,
    pub open_web_ui_url: Option<String>,
}
