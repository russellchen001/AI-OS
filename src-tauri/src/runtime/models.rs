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
    RuntimeNotFound,
    OperationNotFound,
    UnsupportedOperation,
    OperationConflict,
    OperationCapacityExceeded,
    CancellationUnsupported,
    CancellationTooLate,
    OperationFailed,
    OperationTaskFailed,
    DependencyUnavailable,
    DependencyNotInstalled,
    InvalidRuntimeLocation,
    ContainerNotFound,
    ContainerAmbiguous,
    ReadinessTimeout,
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
    pub observed_at: Option<String>,
    pub error: Option<NormalizedRuntimeError>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusRequest {
    pub ollama_url: Option<String>,
    pub open_web_ui_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeOperationAction {
    Start,
    Stop,
    Restart,
    Open,
}

impl RuntimeOperationAction {
    pub(crate) fn reserves_lifecycle_slot(self) -> bool {
        matches!(self, Self::Start | Self::Stop | Self::Restart)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeOperationState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl RuntimeOperationState {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationProgress {
    pub phase: String,
    pub completed_units: Option<u32>,
    pub total_units: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationResult {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bulk: Option<RuntimeBulkResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBulkResult {
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub outcomes: Vec<RuntimeBulkOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBulkOutcome {
    pub runtime_id: String,
    pub succeeded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<NormalizedRuntimeError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationSnapshot {
    pub operation_id: String,
    pub runtime_id: String,
    pub action: RuntimeOperationAction,
    pub state: RuntimeOperationState,
    pub revision: u64,
    pub accepted_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub progress: Option<RuntimeOperationProgress>,
    pub cancellable: bool,
    pub result: Option<RuntimeOperationResult>,
    pub error: Option<NormalizedRuntimeError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum RuntimeOperationAdmission {
    Accepted {
        operation: RuntimeOperationSnapshot,
    },
    Conflict {
        #[serde(rename = "existingOperation")]
        existing_operation: RuntimeOperationSnapshot,
    },
    Rejected {
        error: NormalizedRuntimeError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationEvent {
    pub version: u8,
    pub operation: RuntimeOperationSnapshot,
}
