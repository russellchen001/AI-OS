use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use url::{Host, Url};

use crate::openclaw;

use super::models::{
    NormalizedRuntimeError, RuntimeErrorCode, RuntimeLocation, RuntimeOperationAction,
    RuntimeOperationProgress,
};

const OPENCLAW_SERVICE: &str = "ai.openclaw.gateway";
const DOCKER_CLI: &str = "/Applications/Docker.app/Contents/Resources/bin/docker";
const CURL: &str = "/usr/bin/curl";
const OPEN: &str = "/usr/bin/open";
const LAUNCHCTL: &str = "/bin/launchctl";
const OSASCRIPT: &str = "/usr/bin/osascript";
const PGREP: &str = "/usr/bin/pgrep";
const CHERRY_APP: &str = "Cherry Studio";
const CHERRY_QUIT_SCRIPT: &str = "tell application \"Cherry Studio\" to quit";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const VERIFICATION_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(350);
const CLEANUP_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_CAPTURE_BYTES: u64 = 64 * 1024;
const DOCKER_ENV_REMOVALS: &[&str] = &[
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    "DOCKER_CONFIG",
];

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ValidatedEndpoint {
    url: String,
    location: RuntimeLocation,
}

impl ValidatedEndpoint {
    pub(crate) fn as_str(&self) -> &str {
        &self.url
    }

    pub(crate) fn location(&self) -> RuntimeLocation {
        self.location
    }
}

impl std::fmt::Debug for ValidatedEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedEndpoint")
            .field("location", &self.location)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct OpenClawGatewayEndpoint {
    url: Url,
    location: RuntimeLocation,
}

impl OpenClawGatewayEndpoint {
    fn browser_endpoint(&self) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
        if !matches!(self.url.scheme(), "http" | "https") {
            return Err(unsupported());
        }
        if self.url.query().is_some() || self.url.fragment().is_some() {
            return Err(invalid_configuration());
        }
        Ok(ValidatedEndpoint {
            url: self.url.to_string(),
            location: self.location,
        })
    }
}

impl std::fmt::Debug for OpenClawGatewayEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenClawGatewayEndpoint")
            .field("location", &self.location)
            .finish()
    }
}

pub(crate) fn classify_endpoint(value: &str) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
    let parsed = Url::parse(value.trim()).map_err(|_| invalid_configuration())?;
    if !matches!(parsed.scheme(), "http" | "https") || has_credentials(&parsed) {
        return Err(invalid_configuration());
    }
    let location = classify_url_host(&parsed).ok_or_else(invalid_configuration)?;
    Ok(ValidatedEndpoint {
        url: parsed.to_string(),
        location,
    })
}

fn classify_openclaw_gateway(
    value: &str,
) -> Result<OpenClawGatewayEndpoint, NormalizedRuntimeError> {
    let parsed = Url::parse(value.trim()).map_err(|_| invalid_configuration())?;
    if !matches!(parsed.scheme(), "ws" | "wss" | "http" | "https") || has_credentials(&parsed) {
        return Err(invalid_configuration());
    }
    let location = classify_url_host(&parsed).ok_or_else(invalid_configuration)?;
    Ok(OpenClawGatewayEndpoint {
        url: parsed,
        location,
    })
}

pub(crate) fn classify_runtime_url(value: &str) -> Option<RuntimeLocation> {
    Url::parse(value).ok().as_ref().and_then(classify_url_host)
}

fn has_credentials(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

fn classify_url_host(parsed: &Url) -> Option<RuntimeLocation> {
    match parsed.host() {
        Some(Host::Domain(host)) if host.eq_ignore_ascii_case("localhost") => {
            Some(RuntimeLocation::Local)
        }
        Some(Host::Ipv4(address)) if address.is_loopback() => Some(RuntimeLocation::Local),
        Some(Host::Ipv6(address)) if address.is_loopback() => Some(RuntimeLocation::Local),
        Some(_) => Some(RuntimeLocation::Remote),
        None => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeLifecycleRequest {
    pub runtime_id: String,
    pub action: RuntimeOperationAction,
    pub endpoint_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OllamaInstallation {
    NotInstalled,
    HomebrewFormulaInstalled { brew_path: &'static str },
    HomebrewServiceManaged { brew_path: &'static str },
    OtherInstallation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenWebUiDependency {
    DockerNotInstalled,
    DockerInstalledStopped,
    DockerProcessPresentDaemonUnavailable,
    DockerInspectionFailed,
    ContainerMissing,
    ContainerStopped {
        id: String,
    },
    ContainerRunning {
        id: String,
    },
    ContainerRunningEndpointUnavailable {
        id: String,
    },
    ContainerReady {
        id: String,
    },
    ContainerUnsupported {
        id: String,
        state: DockerContainerState,
    },
    ContainerAmbiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DockerContainerState {
    Running,
    Exited,
    Created,
    Paused,
    Restarting,
    Removing,
    Dead,
    Unknown,
}

#[derive(Clone, PartialEq, Eq)]
enum OpenClawContext {
    Open {
        gateway: OpenClawGatewayEndpoint,
    },
    Manage {
        gateway: OpenClawGatewayEndpoint,
        launchctl_domain: String,
        service_state: LaunchServiceState,
        bootstrap_plist: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchServiceState {
    Loaded,
    NotLoaded,
    InspectionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OllamaContext {
    endpoint: ValidatedEndpoint,
    installation: OllamaInstallation,
}

#[derive(Clone, PartialEq, Eq)]
struct LocalDockerTarget {
    host: String,
}

#[derive(Clone, PartialEq, Eq)]
struct DockerContext {
    target: Option<LocalDockerTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenWebUiContext {
    Open {
        endpoint: ValidatedEndpoint,
    },
    Manage {
        endpoint: ValidatedEndpoint,
        dependency: OpenWebUiDependency,
        target: Option<LocalDockerTarget>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CherryContext;

#[derive(Clone, PartialEq, Eq)]
enum RuntimePlanningContext {
    OpenClaw(OpenClawContext),
    Ollama(OllamaContext),
    Docker(DockerContext),
    OpenWebUi(OpenWebUiContext),
    Cherry(CherryContext),
}

impl std::fmt::Debug for OpenClawContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Open { .. } => "Open",
            Self::Manage { .. } => "Manage",
        };
        formatter
            .debug_tuple("OpenClawContext")
            .field(&kind)
            .finish()
    }
}

impl std::fmt::Debug for DockerContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DockerContext")
            .field("has_target", &self.target.is_some())
            .finish()
    }
}

impl std::fmt::Debug for RuntimePlanningContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::OpenClaw(_) => "OpenClaw",
            Self::Ollama(_) => "Ollama",
            Self::Docker(_) => "Docker",
            Self::OpenWebUi(_) => "OpenWebUi",
            Self::Cherry(_) => "Cherry",
        };
        formatter
            .debug_tuple("RuntimePlanningContext")
            .field(&kind)
            .finish()
    }
}

trait ContextSource {
    fn openclaw(
        &self,
        action: RuntimeOperationAction,
    ) -> Result<OpenClawContext, NormalizedRuntimeError>;
    fn ollama(&self, endpoint: ValidatedEndpoint, inspect_ownership: bool) -> OllamaContext;
    fn docker(
        &self,
        action: RuntimeOperationAction,
    ) -> Result<DockerContext, NormalizedRuntimeError>;
    fn open_webui(&self, endpoint: ValidatedEndpoint, inspect_dependency: bool)
        -> OpenWebUiContext;
    fn cherry(&self) -> CherryContext;
}

struct NativeContextSource;

impl ContextSource for NativeContextSource {
    fn openclaw(
        &self,
        action: RuntimeOperationAction,
    ) -> Result<OpenClawContext, NormalizedRuntimeError> {
        collect_openclaw_context(action)
    }

    fn ollama(&self, endpoint: ValidatedEndpoint, inspect_ownership: bool) -> OllamaContext {
        OllamaContext {
            endpoint,
            installation: if inspect_ownership {
                detect_ollama_installation()
            } else {
                OllamaInstallation::OtherInstallation
            },
        }
    }

    fn docker(
        &self,
        action: RuntimeOperationAction,
    ) -> Result<DockerContext, NormalizedRuntimeError> {
        let target = match action {
            RuntimeOperationAction::Open => None,
            RuntimeOperationAction::Start => Some(expected_local_docker_target()?),
            RuntimeOperationAction::Stop => Some(establish_local_docker_target()?),
            RuntimeOperationAction::Restart => None,
        };
        Ok(DockerContext { target })
    }

    fn open_webui(
        &self,
        endpoint: ValidatedEndpoint,
        inspect_dependency: bool,
    ) -> OpenWebUiContext {
        if inspect_dependency {
            let (dependency, target) = inspect_open_webui_dependency(&endpoint);
            OpenWebUiContext::Manage {
                endpoint,
                dependency,
                target,
            }
        } else {
            OpenWebUiContext::Open { endpoint }
        }
    }

    fn cherry(&self) -> CherryContext {
        CherryContext
    }
}

// M1B2B will use this single entry point so context cannot be mixed across requests.
#[allow(dead_code)]
pub(crate) fn prepare_execution_plan(
    request: &RuntimeLifecycleRequest,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    prepare_with_source(request, &NativeContextSource)
}

fn prepare_with_source(
    request: &RuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let context = collect_context_with(request, source)?;
    build_execution_plan(request, context)
}

fn collect_context_with(
    request: &RuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimePlanningContext, NormalizedRuntimeError> {
    match request.runtime_id.as_str() {
        "openclaw" => source
            .openclaw(request.action)
            .map(RuntimePlanningContext::OpenClaw),
        "ollama" => {
            let endpoint = explicit_endpoint(request)?;
            Ok(RuntimePlanningContext::Ollama(source.ollama(
                endpoint,
                request.action != RuntimeOperationAction::Open,
            )))
        }
        "docker-desktop" => source
            .docker(request.action)
            .map(RuntimePlanningContext::Docker),
        "open-webui" => {
            let endpoint = explicit_endpoint(request)?;
            Ok(RuntimePlanningContext::OpenWebUi(source.open_webui(
                endpoint,
                request.action != RuntimeOperationAction::Open,
            )))
        }
        "cherry-studio" => Ok(RuntimePlanningContext::Cherry(source.cherry())),
        _ => Err(runtime_not_found()),
    }
}

fn explicit_endpoint(
    request: &RuntimeLifecycleRequest,
) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
    classify_endpoint(
        request
            .endpoint_url
            .as_deref()
            .ok_or_else(invalid_configuration)?,
    )
}

fn collect_openclaw_context(
    action: RuntimeOperationAction,
) -> Result<OpenClawContext, NormalizedRuntimeError> {
    if action == RuntimeOperationAction::Open {
        let endpoint = openclaw::active_runtime_endpoint()
            .map_err(|_| configuration_unavailable())?
            .ok_or_else(configuration_unavailable)?;
        return Ok(OpenClawContext::Open {
            gateway: classify_openclaw_gateway(&endpoint)?,
        });
    }
    let endpoint = openclaw::active_runtime_endpoint()
        .map_err(|_| configuration_unavailable())?
        .ok_or_else(configuration_unavailable)?;
    let gateway = classify_openclaw_gateway(&endpoint)?;
    let launchctl_domain = current_launchctl_domain()?;
    let target = format!("{launchctl_domain}/{OPENCLAW_SERVICE}");
    let service_state = inspect_launch_service(&target, PROBE_TIMEOUT);
    let bootstrap_plist = dirs::home_dir()
        .map(|home| home.join("Library/LaunchAgents/ai.openclaw.gateway.plist"))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned());
    Ok(OpenClawContext::Manage {
        gateway,
        launchctl_domain,
        service_state,
        bootstrap_plist,
    })
}

fn inspect_launch_service(target: &str, timeout: Duration) -> LaunchServiceState {
    classify_launch_service_result(probe_status(LAUNCHCTL, &["print", target], timeout))
}

fn classify_launch_service_result(
    result: Result<bool, NormalizedRuntimeError>,
) -> LaunchServiceState {
    match result {
        Ok(true) => LaunchServiceState::Loaded,
        Ok(false) => LaunchServiceState::NotLoaded,
        Err(_) => LaunchServiceState::InspectionFailed,
    }
}

fn current_launchctl_domain() -> Result<String, NormalizedRuntimeError> {
    let output = capture_command("/usr/bin/id", &["-u"], PROBE_TIMEOUT)?;
    let uid = output.trim();
    if !uid.is_empty() && uid.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(format!("gui/{uid}"))
    } else {
        Err(operation_failed())
    }
}

fn detect_ollama_installation() -> OllamaInstallation {
    for brew in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if !Path::new(brew).is_file() {
            continue;
        }
        let formula =
            probe_status(brew, &["list", "--formula", "ollama"], PROBE_TIMEOUT).unwrap_or(false);
        if !formula {
            continue;
        }
        let services =
            capture_command(brew, &["services", "list"], PROBE_TIMEOUT).unwrap_or_default();
        if homebrew_service_managed(&services) {
            return OllamaInstallation::HomebrewServiceManaged { brew_path: brew };
        }
        return OllamaInstallation::HomebrewFormulaInstalled { brew_path: brew };
    }
    if Path::new("/opt/homebrew/bin/ollama").is_file()
        || Path::new("/usr/local/bin/ollama").is_file()
        || Path::new("/Applications/Ollama.app").exists()
    {
        OllamaInstallation::OtherInstallation
    } else {
        OllamaInstallation::NotInstalled
    }
}

fn homebrew_service_managed(output: &str) -> bool {
    output.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.first() == Some(&"ollama")
            && fields
                .get(1)
                .is_some_and(|state| matches!(*state, "started" | "stopped" | "error"))
    })
}

fn inspect_open_webui_dependency(
    endpoint: &ValidatedEndpoint,
) -> (OpenWebUiDependency, Option<LocalDockerTarget>) {
    let first_process = probe_status(
        PGREP,
        &["-f", "/Docker.app/Contents/MacOS/Docker"],
        PROBE_TIMEOUT,
    );
    let second_process = probe_status(PGREP, &["-x", "Docker Desktop"], PROBE_TIMEOUT);
    let docker_process_running = match (first_process, second_process) {
        (Ok(first), Ok(second)) => first || second,
        _ => return (OpenWebUiDependency::DockerInspectionFailed, None),
    };
    let docker_installed = docker_process_running || Path::new("/Applications/Docker.app").exists();
    if !docker_installed {
        return (
            classify_open_webui_inspection(false, false, false, false, &[], None),
            None,
        );
    }
    if !docker_process_running {
        return (OpenWebUiDependency::DockerInstalledStopped, None);
    }
    let target = match establish_local_docker_target() {
        Ok(target) => target,
        Err(_) => return (OpenWebUiDependency::DockerInspectionFailed, None),
    };
    let daemon_ready = match probe_native_status(&docker_command(&target, ["info"]), PROBE_TIMEOUT)
    {
        Ok(ready) => ready,
        Err(_) => return (OpenWebUiDependency::DockerInspectionFailed, Some(target)),
    };
    if !daemon_ready {
        return (
            OpenWebUiDependency::DockerProcessPresentDaemonUnavailable,
            Some(target),
        );
    }
    let output = capture_native_command(
        &docker_command(
            &target,
            [
                "ps",
                "-a",
                "--format",
                "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.State}}",
            ],
        ),
        PROBE_TIMEOUT,
    );
    let Ok(output) = output else {
        return (OpenWebUiDependency::DockerInspectionFailed, Some(target));
    };
    let candidates = output
        .lines()
        .filter_map(parse_open_webui_candidate)
        .collect::<Vec<_>>();
    let endpoint_ready = probe_status(
        CURL,
        &["-fsS", "--max-time", "2", endpoint.as_str()],
        PROBE_TIMEOUT,
    )
    .ok();
    (
        classify_open_webui_inspection(
            true,
            docker_process_running,
            true,
            true,
            &candidates,
            endpoint_ready,
        ),
        Some(target),
    )
}

fn establish_local_docker_target() -> Result<LocalDockerTarget, NormalizedRuntimeError> {
    let target = expected_local_docker_target()?;
    if probe_native_status(&docker_command(&target, ["info"]), PROBE_TIMEOUT)? {
        Ok(target)
    } else {
        Err(error(
            RuntimeErrorCode::DependencyUnavailable,
            "The local Docker Desktop target could not be verified.",
            true,
        ))
    }
}

fn expected_local_docker_target() -> Result<LocalDockerTarget, NormalizedRuntimeError> {
    let home = dirs::home_dir().ok_or_else(|| {
        error(
            RuntimeErrorCode::DependencyUnavailable,
            "The local Docker Desktop target could not be determined.",
            true,
        )
    })?;
    Ok(LocalDockerTarget {
        host: format!("unix://{}", home.join(".docker/run/docker.sock").display()),
    })
}

#[cfg(test)]
fn is_expected_local_docker_target(target: &LocalDockerTarget, home: &Path) -> bool {
    target.host == format!("unix://{}", home.join(".docker/run/docker.sock").display())
}

fn parse_open_webui_candidate(line: &str) -> Option<(String, DockerContainerState)> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != 4 {
        return None;
    }
    let searchable = format!("{} {}", fields[1], fields[2]).to_ascii_lowercase();
    if !(searchable.contains("open-webui")
        || searchable.contains("open_webui")
        || searchable.contains("openwebui"))
    {
        return None;
    }
    validate_container_id(fields[0])
        .ok()
        .map(|id| (id, parse_container_state(fields[3])))
}

fn parse_container_state(value: &str) -> DockerContainerState {
    match value.trim().to_ascii_lowercase().as_str() {
        "running" => DockerContainerState::Running,
        "exited" => DockerContainerState::Exited,
        "created" => DockerContainerState::Created,
        "paused" => DockerContainerState::Paused,
        "restarting" => DockerContainerState::Restarting,
        "removing" => DockerContainerState::Removing,
        "dead" => DockerContainerState::Dead,
        _ => DockerContainerState::Unknown,
    }
}

#[derive(Clone, PartialEq, Eq)]
struct NativeCommand {
    program: &'static str,
    args: Vec<String>,
    env_remove: Vec<&'static str>,
}

impl std::fmt::Debug for NativeCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCommand")
            .field("program", &self.program)
            .field("argument_count", &self.args.len())
            .field("removed_environment_count", &self.env_remove.len())
            .finish()
    }
}

impl std::fmt::Debug for LocalDockerTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocalDockerTarget([redacted])")
    }
}

impl NativeCommand {
    fn new(program: &'static str, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program,
            args: args.into_iter().map(Into::into).collect(),
            env_remove: Vec::new(),
        }
    }

    fn with_env_removals(mut self, names: &[&'static str]) -> Self {
        self.env_remove = names.to_vec();
        self
    }
}

fn docker_command(
    target: &LocalDockerTarget,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> NativeCommand {
    let mut command_args = vec!["--host".to_string(), target.host.clone()];
    command_args.extend(args.into_iter().map(Into::into));
    NativeCommand::new(DOCKER_CLI, command_args).with_env_removals(DOCKER_ENV_REMOVALS)
}

#[derive(Clone, PartialEq, Eq)]
enum Verification {
    None,
    HttpReady(ValidatedEndpoint),
    HttpStopped(ValidatedEndpoint),
    DockerReady(LocalDockerTarget),
    DockerStopped(LocalDockerTarget),
    ProcessPresent(&'static str),
    ProcessAbsent(&'static str),
    LaunchServiceLoaded(String),
    LaunchServiceNotLoaded(String),
    ContainerStopped {
        id: String,
        target: LocalDockerTarget,
    },
}

impl Verification {
    fn kind(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::HttpReady(_) => "HttpReady",
            Self::HttpStopped(_) => "HttpStopped",
            Self::DockerReady(_) => "DockerReady",
            Self::DockerStopped(_) => "DockerStopped",
            Self::ProcessPresent(_) => "ProcessPresent",
            Self::ProcessAbsent(_) => "ProcessAbsent",
            Self::LaunchServiceLoaded(_) => "LaunchServiceLoaded",
            Self::LaunchServiceNotLoaded(_) => "LaunchServiceNotLoaded",
            Self::ContainerStopped { .. } => "ContainerStopped",
        }
    }
}

impl std::fmt::Debug for Verification {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("Verification")
            .field(&self.kind())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
enum RuntimeAdapterPlan {
    OpenClawLocal {
        commands: Vec<NativeCommand>,
        verification: Verification,
    },
    OpenClawRemoteOpen {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
    },
    OllamaHomebrewLifecycle {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
        verification: Verification,
    },
    OllamaLocalOpen {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
    },
    OllamaRemoteOpen {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
    },
    DockerDesktop {
        commands: Vec<NativeCommand>,
        verification: Verification,
    },
    OpenWebUiLocalOpen {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
    },
    OpenWebUiRemoteOpen {
        endpoint: ValidatedEndpoint,
        commands: Vec<NativeCommand>,
    },
    OpenWebUiContainer {
        endpoint: ValidatedEndpoint,
        container_id: String,
        commands: Vec<NativeCommand>,
        verification: Verification,
    },
    OpenWebUiNoOp {
        endpoint: ValidatedEndpoint,
        container_id: String,
        verification: Verification,
    },
    CherryStudio {
        commands: Vec<NativeCommand>,
        verification: Verification,
    },
}

impl RuntimeAdapterPlan {
    fn kind(&self) -> &'static str {
        match self {
            Self::OpenClawLocal { .. } => "OpenClawLocal",
            Self::OpenClawRemoteOpen { .. } => "OpenClawRemoteOpen",
            Self::OllamaHomebrewLifecycle { .. } => "OllamaHomebrewLifecycle",
            Self::OllamaLocalOpen { .. } => "OllamaLocalOpen",
            Self::OllamaRemoteOpen { .. } => "OllamaRemoteOpen",
            Self::DockerDesktop { .. } => "DockerDesktop",
            Self::OpenWebUiLocalOpen { .. } => "OpenWebUiLocalOpen",
            Self::OpenWebUiRemoteOpen { .. } => "OpenWebUiRemoteOpen",
            Self::OpenWebUiContainer { .. } => "OpenWebUiContainer",
            Self::OpenWebUiNoOp { .. } => "OpenWebUiNoOp",
            Self::CherryStudio { .. } => "CherryStudio",
        }
    }
}

impl std::fmt::Debug for RuntimeAdapterPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (commands, verification) = commands_and_verification(self);
        formatter
            .debug_struct("RuntimeAdapterPlan")
            .field("kind", &self.kind())
            .field("command_count", &commands.len())
            .field("verification", &verification.kind())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RuntimeExecutionPlan {
    runtime_id: String,
    action: RuntimeOperationAction,
    effective_location: RuntimeLocation,
    adapter: RuntimeAdapterPlan,
    progress: Vec<RuntimeOperationProgress>,
}

impl std::fmt::Debug for RuntimeExecutionPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeExecutionPlan")
            .field("runtime_id", &self.runtime_id)
            .field("action", &self.action)
            .field("effective_location", &self.effective_location)
            .field("adapter", &self.adapter)
            .finish()
    }
}

fn build_execution_plan(
    request: &RuntimeLifecycleRequest,
    context: RuntimePlanningContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    match (request.runtime_id.as_str(), context) {
        ("openclaw", RuntimePlanningContext::OpenClaw(context)) => {
            plan_openclaw(request.action, context)
        }
        ("ollama", RuntimePlanningContext::Ollama(context)) => plan_ollama(request.action, context),
        ("docker-desktop", RuntimePlanningContext::Docker(context)) => {
            plan_docker(request.action, context)
        }
        ("open-webui", RuntimePlanningContext::OpenWebUi(context)) => {
            plan_open_webui(request.action, context)
        }
        ("cherry-studio", RuntimePlanningContext::Cherry(_)) => plan_cherry(request.action),
        _ => Err(error(
            RuntimeErrorCode::InvalidConfiguration,
            "The runtime context does not match the request.",
            false,
        )),
    }
}

fn plan_openclaw(
    action: RuntimeOperationAction,
    context: OpenClawContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (gateway, managed) = match context {
        OpenClawContext::Open { gateway } => (gateway, None),
        OpenClawContext::Manage {
            gateway,
            launchctl_domain,
            service_state,
            bootstrap_plist,
        } => (
            gateway,
            Some((launchctl_domain, service_state, bootstrap_plist)),
        ),
    };
    let location = gateway.location;
    if location == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let endpoint = gateway.browser_endpoint()?;
        let commands = vec![open_url(&endpoint)];
        return Ok(plan(
            "openclaw",
            action,
            location,
            RuntimeAdapterPlan::OpenClawRemoteOpen { endpoint, commands },
            &["validating", "opening", "complete"],
        ));
    }

    if action == RuntimeOperationAction::Open {
        let endpoint = gateway.browser_endpoint()?;
        return Ok(plan(
            "openclaw",
            action,
            location,
            RuntimeAdapterPlan::OpenClawLocal {
                commands: vec![open_url(&endpoint)],
                verification: Verification::None,
            },
            &["validating", "opening", "complete"],
        ));
    }
    let (launchctl_domain, service_state, bootstrap_plist) =
        managed.ok_or_else(invalid_configuration)?;
    if service_state == LaunchServiceState::InspectionFailed {
        return Err(error(
            RuntimeErrorCode::ProbeFailed,
            "The OpenClaw launch service could not be inspected.",
            true,
        ));
    }
    let service_target = format!("{launchctl_domain}/{OPENCLAW_SERVICE}");
    let (commands, verification, phases) = match action {
        RuntimeOperationAction::Start => {
            let mut commands = Vec::new();
            if service_state == LaunchServiceState::NotLoaded {
                let plist = bootstrap_plist.ok_or_else(|| {
                    error(
                        RuntimeErrorCode::ConfigurationUnavailable,
                        "The OpenClaw launch service is not installed.",
                        false,
                    )
                })?;
                commands.push(NativeCommand::new(
                    LAUNCHCTL,
                    ["bootstrap", launchctl_domain.as_str(), plist.as_str()],
                ));
            }
            commands.push(NativeCommand::new(
                LAUNCHCTL,
                ["kickstart", "-k", service_target.as_str()],
            ));
            (
                commands,
                Verification::LaunchServiceLoaded(service_target),
                &[
                    "validating",
                    "starting-application",
                    "verifying",
                    "complete",
                ][..],
            )
        }
        RuntimeOperationAction::Stop if service_state == LaunchServiceState::NotLoaded => (
            Vec::new(),
            Verification::LaunchServiceNotLoaded(service_target),
            &["validating", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Stop => (
            vec![NativeCommand::new(
                LAUNCHCTL,
                ["bootout", service_target.as_str()],
            )],
            Verification::LaunchServiceNotLoaded(service_target),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => unreachable!(),
        RuntimeOperationAction::Restart => return Err(unsupported()),
    };
    Ok(plan(
        "openclaw",
        action,
        location,
        RuntimeAdapterPlan::OpenClawLocal {
            commands,
            verification,
        },
        phases,
    ))
}

fn plan_ollama(
    action: RuntimeOperationAction,
    context: OllamaContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let endpoint = context.endpoint;
    let location = endpoint.location();
    if location == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let commands = vec![open_url(&endpoint)];
        return Ok(plan(
            "ollama",
            action,
            location,
            RuntimeAdapterPlan::OllamaRemoteOpen { endpoint, commands },
            &["validating", "opening", "complete"],
        ));
    }
    if action == RuntimeOperationAction::Open {
        let commands = vec![open_url(&endpoint)];
        return Ok(plan(
            "ollama",
            action,
            location,
            RuntimeAdapterPlan::OllamaLocalOpen { endpoint, commands },
            &["validating", "opening", "complete"],
        ));
    }
    if action == RuntimeOperationAction::Restart {
        return Err(unsupported());
    }
    let brew = match (action, context.installation) {
        (
            RuntimeOperationAction::Start,
            OllamaInstallation::HomebrewFormulaInstalled { brew_path }
            | OllamaInstallation::HomebrewServiceManaged { brew_path },
        ) => brew_path,
        (
            RuntimeOperationAction::Stop,
            OllamaInstallation::HomebrewServiceManaged { brew_path },
        ) => brew_path,
        (_, OllamaInstallation::NotInstalled) => {
            return Err(error(
                RuntimeErrorCode::DependencyNotInstalled,
                "Ollama is not installed.",
                false,
            ))
        }
        _ => return Err(unsupported()),
    };
    let (verb, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            "start",
            Verification::HttpReady(ollama_tags_endpoint(&endpoint)?),
            &[
                "validating",
                "starting-application",
                "waiting-for-readiness",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            "stop",
            Verification::HttpStopped(ollama_tags_endpoint(&endpoint)?),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        _ => unreachable!(),
    };
    let commands = vec![NativeCommand::new(brew, ["services", verb, "ollama"])];
    Ok(plan(
        "ollama",
        action,
        location,
        RuntimeAdapterPlan::OllamaHomebrewLifecycle {
            endpoint,
            commands,
            verification,
        },
        phases,
    ))
}

fn ollama_tags_endpoint(
    endpoint: &ValidatedEndpoint,
) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
    let mut url = Url::parse(endpoint.as_str()).map_err(|_| invalid_configuration())?;
    url.set_path("/api/tags");
    url.set_query(None);
    url.set_fragment(None);
    Ok(ValidatedEndpoint {
        url: url.to_string(),
        location: endpoint.location(),
    })
}

fn plan_docker(
    action: RuntimeOperationAction,
    context: DockerContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    if action == RuntimeOperationAction::Restart {
        return Err(unsupported());
    }
    let (commands, verification, phases) = match action {
        RuntimeOperationAction::Start => {
            let target = context.target.ok_or_else(invalid_configuration)?;
            (
                vec![NativeCommand::new(OPEN, ["-a", "Docker"])],
                Verification::DockerReady(target),
                &[
                    "validating",
                    "starting-application",
                    "waiting-for-readiness",
                    "complete",
                ][..],
            )
        }
        RuntimeOperationAction::Stop => {
            let target = context.target.ok_or_else(invalid_configuration)?;
            (
                vec![docker_command(&target, ["desktop", "stop"])],
                Verification::DockerStopped(target),
                &["validating", "stopping-service", "verifying", "complete"][..],
            )
        }
        RuntimeOperationAction::Open => (
            vec![NativeCommand::new(OPEN, ["-a", "Docker"])],
            Verification::None,
            &["validating", "opening", "complete"][..],
        ),
        RuntimeOperationAction::Restart => unreachable!(),
    };
    Ok(plan(
        "docker-desktop",
        action,
        RuntimeLocation::Local,
        RuntimeAdapterPlan::DockerDesktop {
            commands,
            verification,
        },
        phases,
    ))
}

fn plan_open_webui(
    action: RuntimeOperationAction,
    context: OpenWebUiContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (endpoint, dependency, target) = match context {
        OpenWebUiContext::Open { endpoint } => {
            if action != RuntimeOperationAction::Open {
                return Err(invalid_configuration());
            }
            let location = endpoint.location();
            let commands = vec![open_url(&endpoint)];
            let adapter = if location == RuntimeLocation::Local {
                RuntimeAdapterPlan::OpenWebUiLocalOpen { endpoint, commands }
            } else {
                RuntimeAdapterPlan::OpenWebUiRemoteOpen { endpoint, commands }
            };
            return Ok(plan(
                "open-webui",
                action,
                location,
                adapter,
                &["validating", "opening", "complete"],
            ));
        }
        OpenWebUiContext::Manage {
            endpoint,
            dependency,
            target,
        } => (endpoint, dependency, target),
    };
    if endpoint.location() == RuntimeLocation::Remote {
        return Err(invalid_location());
    }
    let rejected = match dependency {
        OpenWebUiDependency::DockerNotInstalled => Some(error(
            RuntimeErrorCode::DependencyNotInstalled,
            "Docker Desktop is not installed.",
            false,
        )),
        OpenWebUiDependency::DockerInstalledStopped
        | OpenWebUiDependency::DockerProcessPresentDaemonUnavailable => Some(error(
            RuntimeErrorCode::DependencyUnavailable,
            "Docker must be running before managing Open WebUI.",
            true,
        )),
        OpenWebUiDependency::DockerInspectionFailed => Some(error(
            RuntimeErrorCode::ProbeFailed,
            "Docker container inspection failed.",
            true,
        )),
        OpenWebUiDependency::ContainerMissing => Some(error(
            RuntimeErrorCode::ContainerNotFound,
            "The Open WebUI container was not found.",
            false,
        )),
        OpenWebUiDependency::ContainerAmbiguous => Some(error(
            RuntimeErrorCode::ContainerAmbiguous,
            "Multiple Open WebUI containers were found.",
            false,
        )),
        OpenWebUiDependency::ContainerUnsupported { .. } => Some(error(
            RuntimeErrorCode::UnsupportedOperation,
            "The Open WebUI container is in an unsupported state.",
            true,
        )),
        _ => None,
    };
    if let Some(error) = rejected {
        return Err(error);
    }
    let target = target.ok_or_else(|| {
        error(
            RuntimeErrorCode::ProbeFailed,
            "The local Docker Desktop target was not verified.",
            true,
        )
    })?;

    let (id, running, ready) = match dependency {
        OpenWebUiDependency::ContainerStopped { id } => (validate_container_id(&id)?, false, false),
        OpenWebUiDependency::ContainerRunning { id }
        | OpenWebUiDependency::ContainerRunningEndpointUnavailable { id } => {
            (validate_container_id(&id)?, true, false)
        }
        OpenWebUiDependency::ContainerReady { id } => (validate_container_id(&id)?, true, true),
        _ => unreachable!(),
    };

    let (adapter, phases) = match action {
        RuntimeOperationAction::Start if !running => (
            RuntimeAdapterPlan::OpenWebUiContainer {
                endpoint: endpoint.clone(),
                container_id: id.clone(),
                commands: vec![docker_command(&target, ["start", id.as_str()])],
                verification: Verification::HttpReady(endpoint),
            },
            &[
                "validating",
                "checking-dependency",
                "starting-container",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Start if ready => (
            RuntimeAdapterPlan::OpenWebUiNoOp {
                endpoint: endpoint.clone(),
                container_id: id,
                verification: Verification::HttpReady(endpoint),
            },
            &["validating", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Start => (
            RuntimeAdapterPlan::OpenWebUiNoOp {
                endpoint: endpoint.clone(),
                container_id: id,
                verification: Verification::HttpReady(endpoint),
            },
            &["validating", "waiting-for-readiness", "complete"][..],
        ),
        RuntimeOperationAction::Stop if !running => (
            RuntimeAdapterPlan::OpenWebUiNoOp {
                endpoint,
                container_id: id.clone(),
                verification: Verification::ContainerStopped {
                    id,
                    target: target.clone(),
                },
            },
            &["validating", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Stop => (
            RuntimeAdapterPlan::OpenWebUiContainer {
                endpoint,
                container_id: id.clone(),
                commands: vec![docker_command(&target, ["stop", id.as_str()])],
                verification: Verification::ContainerStopped {
                    id,
                    target: target.clone(),
                },
            },
            &[
                "validating",
                "checking-dependency",
                "stopping-container",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Restart if running => (
            RuntimeAdapterPlan::OpenWebUiContainer {
                endpoint: endpoint.clone(),
                container_id: id.clone(),
                commands: vec![docker_command(&target, ["restart", id.as_str()])],
                verification: Verification::HttpReady(endpoint),
            },
            &[
                "validating",
                "checking-dependency",
                "restarting-container",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Restart => return Err(unsupported()),
        RuntimeOperationAction::Open => return Err(invalid_configuration()),
    };
    Ok(plan(
        "open-webui",
        action,
        RuntimeLocation::Local,
        adapter,
        phases,
    ))
}

fn plan_cherry(
    action: RuntimeOperationAction,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (commands, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            vec![NativeCommand::new(OPEN, ["-a", CHERRY_APP])],
            Verification::ProcessPresent(CHERRY_APP),
            &[
                "validating",
                "starting-application",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            vec![NativeCommand::new(OSASCRIPT, ["-e", CHERRY_QUIT_SCRIPT])],
            Verification::ProcessAbsent(CHERRY_APP),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => (
            vec![NativeCommand::new(OPEN, ["-a", CHERRY_APP])],
            Verification::None,
            &["validating", "opening", "complete"][..],
        ),
        RuntimeOperationAction::Restart => return Err(unsupported()),
    };
    Ok(plan(
        "cherry-studio",
        action,
        RuntimeLocation::Local,
        RuntimeAdapterPlan::CherryStudio {
            commands,
            verification,
        },
        phases,
    ))
}

fn plan(
    runtime_id: &str,
    action: RuntimeOperationAction,
    effective_location: RuntimeLocation,
    adapter: RuntimeAdapterPlan,
    phases: &[&str],
) -> RuntimeExecutionPlan {
    RuntimeExecutionPlan {
        runtime_id: runtime_id.to_string(),
        action,
        effective_location,
        adapter,
        progress: phases
            .iter()
            .map(|phase| RuntimeOperationProgress {
                phase: (*phase).to_string(),
                completed_units: None,
                total_units: None,
                message: progress_message(phase).to_string(),
            })
            .collect(),
    }
}

fn progress_message(phase: &str) -> &'static str {
    match phase {
        "validating" => "Validating runtime operation.",
        "starting-application" => "Starting the runtime.",
        "stopping-service" => "Stopping the runtime.",
        "waiting-for-readiness" => "Waiting for runtime readiness.",
        "checking-dependency" => "Checking runtime dependencies.",
        "starting-container" => "Starting the runtime container.",
        "stopping-container" => "Stopping the runtime container.",
        "restarting-container" => "Restarting the runtime container.",
        "opening" => "Opening the runtime.",
        "verifying" => "Verifying the runtime state.",
        "complete" => "Runtime operation complete.",
        _ => "Updating runtime operation.",
    }
}

fn open_url(endpoint: &ValidatedEndpoint) -> NativeCommand {
    NativeCommand::new(OPEN, [endpoint.as_str()])
}

fn validate_container_id(id: &str) -> Result<String, NormalizedRuntimeError> {
    if (12..=64).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(id.to_ascii_lowercase())
    } else {
        Err(error(
            RuntimeErrorCode::InvalidConfiguration,
            "The Open WebUI container identifier is invalid.",
            false,
        ))
    }
}

// M1B2B will invoke this only after an operation has atomically accepted its frozen plan.
#[allow(dead_code)]
pub(crate) fn execute_plan(
    plan: &RuntimeExecutionPlan,
    mut report: impl FnMut(RuntimeOperationProgress),
) -> Result<(), NormalizedRuntimeError> {
    let command_phase = plan
        .progress
        .iter()
        .position(|update| {
            matches!(
                update.phase.as_str(),
                "starting-application"
                    | "stopping-service"
                    | "starting-container"
                    | "stopping-container"
                    | "restarting-container"
                    | "opening"
            )
        })
        .unwrap_or(0);
    for update in plan.progress.iter().take(command_phase + 1) {
        report(update.clone());
    }
    let (commands, verification) = commands_and_verification(&plan.adapter);
    for command in commands {
        run_native_command(command, COMMAND_TIMEOUT)?;
    }
    for update in plan
        .progress
        .iter()
        .skip(command_phase + 1)
        .take_while(|update| update.phase != "complete")
    {
        report(update.clone());
    }
    verify(verification, VERIFICATION_TIMEOUT)?;
    if let Some(complete) = plan.progress.last() {
        report(complete.clone());
    }
    Ok(())
}

fn commands_and_verification(adapter: &RuntimeAdapterPlan) -> (&[NativeCommand], &Verification) {
    match adapter {
        RuntimeAdapterPlan::OpenClawLocal {
            commands,
            verification,
        }
        | RuntimeAdapterPlan::OllamaHomebrewLifecycle {
            commands,
            verification,
            ..
        }
        | RuntimeAdapterPlan::DockerDesktop {
            commands,
            verification,
        }
        | RuntimeAdapterPlan::OpenWebUiContainer {
            commands,
            verification,
            ..
        }
        | RuntimeAdapterPlan::CherryStudio {
            commands,
            verification,
        } => (commands, verification),
        RuntimeAdapterPlan::OpenWebUiNoOp { verification, .. } => (&[], verification),
        RuntimeAdapterPlan::OpenClawRemoteOpen { commands, .. }
        | RuntimeAdapterPlan::OllamaLocalOpen { commands, .. }
        | RuntimeAdapterPlan::OllamaRemoteOpen { commands, .. }
        | RuntimeAdapterPlan::OpenWebUiLocalOpen { commands, .. }
        | RuntimeAdapterPlan::OpenWebUiRemoteOpen { commands, .. } => {
            static NONE: Verification = Verification::None;
            (commands, &NONE)
        }
    }
}

fn run_native_command(
    command: &NativeCommand,
    timeout: Duration,
) -> Result<(), NormalizedRuntimeError> {
    let mut process = Command::new(command.program);
    process.args(&command.args);
    for name in &command.env_remove {
        process.env_remove(name);
    }
    let mut child = process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| operation_failed())?;
    let status = wait_for_child(&mut child, timeout)?;
    if status {
        Ok(())
    } else {
        Err(operation_failed())
    }
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<bool, NormalizedRuntimeError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|_| operation_failed())? {
            return Ok(status.success());
        }
        if Instant::now() >= deadline {
            if child.kill().is_ok() {
                let cleanup_deadline = Instant::now() + CLEANUP_TIMEOUT;
                while Instant::now() < cleanup_deadline {
                    match child.try_wait() {
                        Ok(Some(_)) | Err(_) => break,
                        Ok(None) => thread::sleep(POLL_INTERVAL),
                    }
                }
            } else {
                let _ = child.try_wait();
            }
            return Err(command_timeout());
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn capture_command(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, NormalizedRuntimeError> {
    let mut file = tempfile::tempfile().map_err(|_| operation_failed())?;
    let stdout = file.try_clone().map_err(|_| operation_failed())?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| operation_failed())?;
    if !wait_for_child(&mut child, timeout)? {
        return Err(operation_failed());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| operation_failed())?;
    let mut output = String::new();
    file.take(MAX_CAPTURE_BYTES)
        .read_to_string(&mut output)
        .map_err(|_| operation_failed())?;
    Ok(output)
}

fn capture_native_command(
    command: &NativeCommand,
    timeout: Duration,
) -> Result<String, NormalizedRuntimeError> {
    let mut file = tempfile::tempfile().map_err(|_| operation_failed())?;
    let stdout = file.try_clone().map_err(|_| operation_failed())?;
    let mut process = Command::new(command.program);
    process.args(&command.args);
    for name in &command.env_remove {
        process.env_remove(name);
    }
    let mut child = process
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| operation_failed())?;
    if !wait_for_child(&mut child, timeout)? {
        return Err(operation_failed());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| operation_failed())?;
    let mut output = String::new();
    file.take(MAX_CAPTURE_BYTES)
        .read_to_string(&mut output)
        .map_err(|_| operation_failed())?;
    Ok(output)
}

fn probe_status(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<bool, NormalizedRuntimeError> {
    let command = NativeCommand::new(program_path(program)?, args.iter().copied());
    probe_native_status(&command, timeout)
}

fn probe_native_status(
    command: &NativeCommand,
    timeout: Duration,
) -> Result<bool, NormalizedRuntimeError> {
    let mut process = Command::new(command.program);
    process.args(&command.args);
    for name in &command.env_remove {
        process.env_remove(name);
    }
    let mut child = process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| operation_failed())?;
    wait_for_child(&mut child, timeout)
}

fn program_path(program: &str) -> Result<&'static str, NormalizedRuntimeError> {
    match program {
        CURL => Ok(CURL),
        OPEN => Ok(OPEN),
        LAUNCHCTL => Ok(LAUNCHCTL),
        OSASCRIPT => Ok(OSASCRIPT),
        PGREP => Ok(PGREP),
        DOCKER_CLI => Ok(DOCKER_CLI),
        "/usr/bin/id" => Ok("/usr/bin/id"),
        "/opt/homebrew/bin/brew" => Ok("/opt/homebrew/bin/brew"),
        "/usr/local/bin/brew" => Ok("/usr/local/bin/brew"),
        _ => Err(operation_failed()),
    }
}

fn verify(verification: &Verification, timeout: Duration) -> Result<(), NormalizedRuntimeError> {
    if *verification == Verification::None {
        return Ok(());
    }
    verify_with_deadline(timeout, |probe_timeout| {
        verification_satisfied(verification, probe_timeout)
    })
}

fn verify_with_deadline(
    timeout: Duration,
    mut probe: impl FnMut(Duration) -> Result<bool, NormalizedRuntimeError>,
) -> Result<(), NormalizedRuntimeError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(readiness_timeout());
        }
        if probe(remaining.min(PROBE_TIMEOUT))? {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(readiness_timeout());
        }
        thread::sleep(READINESS_POLL_INTERVAL.min(remaining));
    }
}

fn verification_satisfied(
    verification: &Verification,
    probe_timeout: Duration,
) -> Result<bool, NormalizedRuntimeError> {
    match verification {
        Verification::None => Ok(true),
        Verification::HttpReady(endpoint) => probe_status(
            CURL,
            &["-fsS", "--max-time", "2", endpoint.as_str()],
            probe_timeout,
        ),
        Verification::HttpStopped(endpoint) => Ok(!probe_status(
            CURL,
            &["-fsS", "--max-time", "2", endpoint.as_str()],
            probe_timeout,
        )?),
        Verification::DockerReady(target) => {
            probe_native_status(&docker_command(target, ["info"]), probe_timeout)
        }
        Verification::DockerStopped(target) => Ok(!probe_native_status(
            &docker_command(target, ["info"]),
            probe_timeout,
        )?),
        Verification::ProcessPresent(name) => probe_status(PGREP, &["-x", name], probe_timeout),
        Verification::ProcessAbsent(name) => {
            Ok(!probe_status(PGREP, &["-x", name], probe_timeout)?)
        }
        Verification::LaunchServiceLoaded(target) => {
            match inspect_launch_service(target, probe_timeout) {
                LaunchServiceState::Loaded => Ok(true),
                LaunchServiceState::NotLoaded => Ok(false),
                LaunchServiceState::InspectionFailed => Err(error(
                    RuntimeErrorCode::ProbeFailed,
                    "The OpenClaw launch service could not be inspected.",
                    true,
                )),
            }
        }
        Verification::LaunchServiceNotLoaded(target) => {
            match inspect_launch_service(target, probe_timeout) {
                LaunchServiceState::Loaded => Ok(false),
                LaunchServiceState::NotLoaded => Ok(true),
                LaunchServiceState::InspectionFailed => Err(error(
                    RuntimeErrorCode::ProbeFailed,
                    "The OpenClaw launch service could not be inspected.",
                    true,
                )),
            }
        }
        Verification::ContainerStopped { id, target } => Ok(matches!(
            docker_container_state(target, id, probe_timeout)?,
            Some(DockerContainerState::Exited | DockerContainerState::Created)
        )),
    }
}

fn docker_container_state(
    target: &LocalDockerTarget,
    id: &str,
    timeout: Duration,
) -> Result<Option<DockerContainerState>, NormalizedRuntimeError> {
    let output = capture_native_command(
        &docker_command(target, ["inspect", "--format", "{{.State.Status}}", id]),
        timeout,
    )?;
    let state = parse_container_state(output.trim());
    Ok((state != DockerContainerState::Unknown).then_some(state))
}

fn classify_open_webui_inspection(
    docker_installed: bool,
    docker_process_running: bool,
    daemon_ready: bool,
    inspection_succeeded: bool,
    candidates: &[(String, DockerContainerState)],
    endpoint_ready: Option<bool>,
) -> OpenWebUiDependency {
    if !docker_installed {
        return OpenWebUiDependency::DockerNotInstalled;
    }
    if !daemon_ready {
        return if docker_process_running {
            OpenWebUiDependency::DockerProcessPresentDaemonUnavailable
        } else {
            OpenWebUiDependency::DockerInstalledStopped
        };
    }
    if !inspection_succeeded {
        return OpenWebUiDependency::DockerInspectionFailed;
    }
    match candidates {
        [] => OpenWebUiDependency::ContainerMissing,
        [(id, DockerContainerState::Exited | DockerContainerState::Created)] => {
            OpenWebUiDependency::ContainerStopped { id: id.clone() }
        }
        [(id, DockerContainerState::Running)] if endpoint_ready == Some(true) => {
            OpenWebUiDependency::ContainerReady { id: id.clone() }
        }
        [(id, DockerContainerState::Running)] if endpoint_ready == Some(false) => {
            OpenWebUiDependency::ContainerRunningEndpointUnavailable { id: id.clone() }
        }
        [(id, DockerContainerState::Running)] => {
            OpenWebUiDependency::ContainerRunning { id: id.clone() }
        }
        [(id, state)] => OpenWebUiDependency::ContainerUnsupported {
            id: id.clone(),
            state: *state,
        },
        _ => OpenWebUiDependency::ContainerAmbiguous,
    }
}

fn invalid_configuration() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::InvalidConfiguration,
        "A valid runtime endpoint is required.",
        false,
    )
}

fn configuration_unavailable() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::ConfigurationUnavailable,
        "OpenClaw configuration could not be read.",
        true,
    )
}

fn invalid_location() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::InvalidRuntimeLocation,
        "Local lifecycle actions are unavailable for remote runtimes.",
        false,
    )
}

fn unsupported() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::UnsupportedOperation,
        "The requested operation is not supported for this runtime.",
        false,
    )
}

fn runtime_not_found() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::RuntimeNotFound,
        "The requested runtime is not available.",
        false,
    )
}

fn operation_failed() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::OperationFailed,
        "The native runtime operation failed.",
        true,
    )
}

fn command_timeout() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::ReadinessTimeout,
        "The native runtime command did not finish in time.",
        true,
    )
}

fn readiness_timeout() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::ReadinessTimeout,
        "The runtime did not reach the expected state in time.",
        true,
    )
}

fn error(code: RuntimeErrorCode, message: &str, retryable: bool) -> NormalizedRuntimeError {
    NormalizedRuntimeError {
        code,
        message: message.to_string(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const CONTAINER_ID: &str = "0123456789abcdef";

    fn request(
        runtime_id: &str,
        action: RuntimeOperationAction,
        endpoint: Option<&str>,
    ) -> RuntimeLifecycleRequest {
        RuntimeLifecycleRequest {
            runtime_id: runtime_id.to_string(),
            action,
            endpoint_url: endpoint.map(str::to_string),
        }
    }

    fn endpoint(value: &str) -> ValidatedEndpoint {
        classify_endpoint(value).unwrap()
    }

    fn docker_target() -> LocalDockerTarget {
        LocalDockerTarget {
            host: "unix:///Users/test/.docker/run/docker.sock".to_string(),
        }
    }

    fn openclaw_context(value: &str, loaded: bool) -> OpenClawContext {
        OpenClawContext::Manage {
            gateway: classify_openclaw_gateway(value).unwrap(),
            launchctl_domain: "gui/501".to_string(),
            service_state: if loaded {
                LaunchServiceState::Loaded
            } else {
                LaunchServiceState::NotLoaded
            },
            bootstrap_plist: Some(
                "/Users/test/Library/LaunchAgents/ai.openclaw.gateway.plist".to_string(),
            ),
        }
    }

    fn ollama_context(value: &str, installation: OllamaInstallation) -> OllamaContext {
        OllamaContext {
            endpoint: endpoint(value),
            installation,
        }
    }

    fn webui_context(
        action: RuntimeOperationAction,
        dependency: OpenWebUiDependency,
    ) -> OpenWebUiContext {
        if action == RuntimeOperationAction::Open {
            OpenWebUiContext::Open {
                endpoint: endpoint("http://localhost:3000"),
            }
        } else {
            OpenWebUiContext::Manage {
                endpoint: endpoint("http://localhost:3000"),
                dependency,
                target: Some(docker_target()),
            }
        }
    }

    fn build(
        runtime: &str,
        action: RuntimeOperationAction,
        context: RuntimePlanningContext,
    ) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
        build_execution_plan(&request(runtime, action, None), context)
    }

    #[test]
    fn endpoint_safety_classifies_local_remote_and_rejects_unsafe_urls() {
        for value in [
            "http://localhost:1",
            "http://127.0.0.1:1",
            "http://127.4.5.6:1",
            "http://[::1]:1",
        ] {
            assert_eq!(endpoint(value).location(), RuntimeLocation::Local);
        }
        for value in ["https://example.com", "http://192.168.1.10"] {
            assert_eq!(endpoint(value).location(), RuntimeLocation::Remote);
        }
        for value in [
            "not a url",
            "file:///tmp/socket",
            "custom://host",
            "https://user:secret@example.com",
        ] {
            assert_eq!(
                classify_endpoint(value).unwrap_err().code,
                RuntimeErrorCode::InvalidConfiguration
            );
        }
    }

    #[test]
    fn openclaw_gateway_and_browser_schemes_are_separate_and_safe() {
        let ws = classify_openclaw_gateway("ws://localhost:18789/gateway").unwrap();
        assert_eq!(ws.location, RuntimeLocation::Local);
        assert_eq!(
            ws.browser_endpoint().unwrap_err().code,
            RuntimeErrorCode::UnsupportedOperation
        );
        let wss = classify_openclaw_gateway("wss://gateway.example.com/path").unwrap();
        assert_eq!(wss.location, RuntimeLocation::Remote);
        assert_eq!(
            wss.browser_endpoint().unwrap_err().code,
            RuntimeErrorCode::UnsupportedOperation
        );
        let browser = classify_openclaw_gateway("https://gateway.example.com/dashboard").unwrap();
        assert_eq!(
            browser.browser_endpoint().unwrap().as_str(),
            "https://gateway.example.com/dashboard"
        );
        assert!(
            classify_openclaw_gateway("https://gateway.example.com/?token=secret")
                .unwrap()
                .browser_endpoint()
                .is_err()
        );
        assert!(classify_openclaw_gateway("wss://user:secret@example.com").is_err());
    }

    #[test]
    fn local_ws_openclaw_supports_start_stop_and_remote_wss_denies_lifecycle() {
        for action in [RuntimeOperationAction::Start, RuntimeOperationAction::Stop] {
            assert!(build(
                "openclaw",
                action,
                RuntimePlanningContext::OpenClaw(openclaw_context("ws://localhost:18789", true))
            )
            .is_ok());
        }
        assert_eq!(
            build(
                "openclaw",
                RuntimeOperationAction::Start,
                RuntimePlanningContext::OpenClaw(openclaw_context(
                    "wss://gateway.example.com",
                    true
                ))
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::InvalidRuntimeLocation
        );
    }

    #[test]
    fn openclaw_start_steps_are_ordered_for_loaded_and_unloaded_services() {
        let unloaded = build(
            "openclaw",
            RuntimeOperationAction::Start,
            RuntimePlanningContext::OpenClaw(openclaw_context("ws://localhost:18789", false)),
        )
        .unwrap();
        let (commands, verification) = commands_and_verification(&unloaded.adapter);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].args[0], "bootstrap");
        assert_eq!(commands[1].args[0], "kickstart");
        assert!(matches!(verification, Verification::LaunchServiceLoaded(_)));
        assert!(commands.iter().all(|command| command.program == LAUNCHCTL));
        assert!(commands
            .iter()
            .flat_map(|command| &command.args)
            .all(|arg| !arg.starts_with("ws://") && !arg.starts_with("wss://")));

        let loaded = build(
            "openclaw",
            RuntimeOperationAction::Start,
            RuntimePlanningContext::OpenClaw(openclaw_context("wss://localhost:18789", true)),
        )
        .unwrap();
        let commands = commands_and_verification(&loaded.adapter).0;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].args[0], "kickstart");
    }

    #[test]
    fn launchctl_failures_are_typed_and_never_assumed_not_loaded() {
        assert_eq!(
            classify_launch_service_result(Ok(true)),
            LaunchServiceState::Loaded
        );
        assert_eq!(
            classify_launch_service_result(Ok(false)),
            LaunchServiceState::NotLoaded
        );
        assert_eq!(
            classify_launch_service_result(Err(command_timeout())),
            LaunchServiceState::InspectionFailed
        );
        assert_eq!(
            classify_launch_service_result(Err(operation_failed())),
            LaunchServiceState::InspectionFailed
        );
        let failed = OpenClawContext::Manage {
            gateway: classify_openclaw_gateway("ws://localhost:18789").unwrap(),
            launchctl_domain: "gui/501".to_string(),
            service_state: LaunchServiceState::InspectionFailed,
            bootstrap_plist: None,
        };
        for action in [RuntimeOperationAction::Start, RuntimeOperationAction::Stop] {
            assert_eq!(
                build(
                    "openclaw",
                    action,
                    RuntimePlanningContext::OpenClaw(failed.clone())
                )
                .unwrap_err()
                .code,
                RuntimeErrorCode::ProbeFailed
            );
        }
    }

    struct RecordingSource {
        openclaw_calls: Cell<u32>,
        openclaw_service_inspections: Cell<u32>,
        ollama_calls: Cell<u32>,
        ollama_ownership_inspections: Cell<u32>,
        docker_calls: Cell<u32>,
        webui_calls: Cell<u32>,
        webui_dependency_inspections: Cell<u32>,
        cherry_calls: Cell<u32>,
    }

    impl RecordingSource {
        fn new() -> Self {
            Self {
                openclaw_calls: Cell::new(0),
                openclaw_service_inspections: Cell::new(0),
                ollama_calls: Cell::new(0),
                ollama_ownership_inspections: Cell::new(0),
                docker_calls: Cell::new(0),
                webui_calls: Cell::new(0),
                webui_dependency_inspections: Cell::new(0),
                cherry_calls: Cell::new(0),
            }
        }
    }

    impl ContextSource for RecordingSource {
        fn openclaw(
            &self,
            action: RuntimeOperationAction,
        ) -> Result<OpenClawContext, NormalizedRuntimeError> {
            self.openclaw_calls.set(self.openclaw_calls.get() + 1);
            if action == RuntimeOperationAction::Open {
                Ok(OpenClawContext::Open {
                    gateway: classify_openclaw_gateway("http://localhost:18789").unwrap(),
                })
            } else {
                self.openclaw_service_inspections
                    .set(self.openclaw_service_inspections.get() + 1);
                Err(configuration_unavailable())
            }
        }

        fn ollama(&self, endpoint: ValidatedEndpoint, inspect_ownership: bool) -> OllamaContext {
            self.ollama_calls.set(self.ollama_calls.get() + 1);
            if inspect_ownership {
                self.ollama_ownership_inspections
                    .set(self.ollama_ownership_inspections.get() + 1);
            }
            OllamaContext {
                endpoint,
                installation: OllamaInstallation::OtherInstallation,
            }
        }

        fn docker(
            &self,
            action: RuntimeOperationAction,
        ) -> Result<DockerContext, NormalizedRuntimeError> {
            self.docker_calls.set(self.docker_calls.get() + 1);
            Ok(DockerContext {
                target: (action != RuntimeOperationAction::Open).then(docker_target),
            })
        }

        fn open_webui(
            &self,
            endpoint: ValidatedEndpoint,
            inspect_dependency: bool,
        ) -> OpenWebUiContext {
            self.webui_calls.set(self.webui_calls.get() + 1);
            if inspect_dependency {
                self.webui_dependency_inspections
                    .set(self.webui_dependency_inspections.get() + 1);
                OpenWebUiContext::Manage {
                    endpoint,
                    dependency: OpenWebUiDependency::ContainerMissing,
                    target: Some(docker_target()),
                }
            } else {
                OpenWebUiContext::Open { endpoint }
            }
        }

        fn cherry(&self) -> CherryContext {
            self.cherry_calls.set(self.cherry_calls.get() + 1);
            CherryContext
        }
    }

    #[test]
    fn context_collection_is_isolated_per_requested_runtime() {
        let source = RecordingSource::new();
        assert!(collect_context_with(
            &request("docker-desktop", RuntimeOperationAction::Start, None),
            &source
        )
        .is_ok());
        assert!(collect_context_with(
            &request("cherry-studio", RuntimeOperationAction::Open, None),
            &source
        )
        .is_ok());
        assert_eq!(source.openclaw_calls.get(), 0);
        assert_eq!(source.docker_calls.get(), 1);
        assert_eq!(source.cherry_calls.get(), 1);

        assert!(collect_context_with(
            &request(
                "ollama",
                RuntimeOperationAction::Open,
                Some("http://localhost:11434")
            ),
            &source
        )
        .is_ok());
        assert_eq!(source.ollama_calls.get(), 1);
        assert_eq!(source.webui_calls.get(), 0);
    }

    #[test]
    fn atomic_preparation_keeps_open_actions_free_of_ownership_inspection() {
        let source = RecordingSource::new();
        let openclaw = request("openclaw", RuntimeOperationAction::Open, None);
        assert!(prepare_with_source(&openclaw, &source).is_ok());
        let ollama = request(
            "ollama",
            RuntimeOperationAction::Open,
            Some("http://localhost:11434"),
        );
        assert!(prepare_with_source(&ollama, &source).is_ok());
        let webui = request(
            "open-webui",
            RuntimeOperationAction::Open,
            Some("http://localhost:3000"),
        );
        assert!(prepare_with_source(&webui, &source).is_ok());
        assert_eq!(source.openclaw_service_inspections.get(), 0);
        assert_eq!(source.ollama_ownership_inspections.get(), 0);
        assert_eq!(source.webui_dependency_inspections.get(), 0);

        let mismatched = build_execution_plan(
            &request("cherry-studio", RuntimeOperationAction::Open, None),
            RuntimePlanningContext::Docker(DockerContext { target: None }),
        );
        assert_eq!(
            mismatched.unwrap_err().code,
            RuntimeErrorCode::InvalidConfiguration
        );
    }

    #[test]
    fn openclaw_plan_retains_no_token_or_historical_health_probe() {
        for (action, expected_loaded) in [
            (RuntimeOperationAction::Start, true),
            (RuntimeOperationAction::Stop, false),
        ] {
            let plan = build(
                "openclaw",
                action,
                RuntimePlanningContext::OpenClaw(openclaw_context(
                    "ws://localhost:18789?token=secret-token",
                    expected_loaded,
                )),
            )
            .unwrap();
            let verification = commands_and_verification(&plan.adapter).1;
            assert!(matches!(
                (action, verification),
                (
                    RuntimeOperationAction::Start,
                    Verification::LaunchServiceLoaded(_)
                ) | (
                    RuntimeOperationAction::Stop,
                    Verification::LaunchServiceNotLoaded(_)
                )
            ));
            let debug = format!("{plan:?}");
            assert!(!debug.contains("secret-token"));
            assert!(!debug.contains("ws://"));
        }
        let source = include_str!("lifecycle.rs");
        let historical_snapshot = ["runtime_", "snapshot()"].concat();
        assert!(!source.contains(&historical_snapshot));
        let websocket_probe = ["test_frozen_", "runtime_profile"].concat();
        assert!(!source.contains(&websocket_probe));
        let token_profile = ["FrozenOpenClaw", "Profile"].concat();
        assert!(!source.contains(&token_profile));
    }

    #[test]
    fn typed_open_variants_do_not_claim_false_ownership() {
        let ollama = build(
            "ollama",
            RuntimeOperationAction::Open,
            RuntimePlanningContext::Ollama(ollama_context(
                "http://localhost:11434",
                OllamaInstallation::OtherInstallation,
            )),
        )
        .unwrap();
        assert!(matches!(
            ollama.adapter,
            RuntimeAdapterPlan::OllamaLocalOpen { .. }
        ));
        let webui = build(
            "open-webui",
            RuntimeOperationAction::Open,
            RuntimePlanningContext::OpenWebUi(webui_context(
                RuntimeOperationAction::Open,
                OpenWebUiDependency::ContainerMissing,
            )),
        )
        .unwrap();
        assert!(matches!(
            webui.adapter,
            RuntimeAdapterPlan::OpenWebUiLocalOpen { .. }
        ));
        assert!(validate_container_id("").is_err());
        assert!(validate_container_id("not-a-container").is_err());
    }

    fn dependency(state: &str) -> OpenWebUiDependency {
        match state {
            "stopped" => OpenWebUiDependency::ContainerStopped {
                id: CONTAINER_ID.to_string(),
            },
            "running" => OpenWebUiDependency::ContainerRunning {
                id: CONTAINER_ID.to_string(),
            },
            "unavailable" => OpenWebUiDependency::ContainerRunningEndpointUnavailable {
                id: CONTAINER_ID.to_string(),
            },
            "ready" => OpenWebUiDependency::ContainerReady {
                id: CONTAINER_ID.to_string(),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn open_webui_start_state_matrix_is_strict_and_idempotent() {
        for (state, expected_commands) in [
            ("stopped", 1),
            ("running", 0),
            ("unavailable", 0),
            ("ready", 0),
        ] {
            let plan = build(
                "open-webui",
                RuntimeOperationAction::Start,
                RuntimePlanningContext::OpenWebUi(webui_context(
                    RuntimeOperationAction::Start,
                    dependency(state),
                )),
            )
            .unwrap();
            let commands = commands_and_verification(&plan.adapter).0;
            assert_eq!(commands.len(), expected_commands, "state {state}");
            if let Some(command) = commands.first() {
                assert_eq!(
                    command.args[command.args.len() - 2..],
                    ["start", CONTAINER_ID]
                );
            }
        }
    }

    #[test]
    fn open_webui_stop_and_restart_state_matrices_are_strict() {
        for state in ["running", "unavailable", "ready"] {
            let stop = build(
                "open-webui",
                RuntimeOperationAction::Stop,
                RuntimePlanningContext::OpenWebUi(webui_context(
                    RuntimeOperationAction::Stop,
                    dependency(state),
                )),
            )
            .unwrap();
            assert_eq!(
                commands_and_verification(&stop.adapter).0[0].args
                    [commands_and_verification(&stop.adapter).0[0].args.len() - 2],
                "stop"
            );
            let restart = build(
                "open-webui",
                RuntimeOperationAction::Restart,
                RuntimePlanningContext::OpenWebUi(webui_context(
                    RuntimeOperationAction::Restart,
                    dependency(state),
                )),
            )
            .unwrap();
            assert_eq!(
                commands_and_verification(&restart.adapter).0[0].args
                    [commands_and_verification(&restart.adapter).0[0].args.len() - 2],
                "restart"
            );
        }
        let stopped = build(
            "open-webui",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::OpenWebUi(webui_context(
                RuntimeOperationAction::Stop,
                dependency("stopped"),
            )),
        )
        .unwrap();
        assert!(commands_and_verification(&stopped.adapter).0.is_empty());
        assert_eq!(
            build(
                "open-webui",
                RuntimeOperationAction::Restart,
                RuntimePlanningContext::OpenWebUi(webui_context(
                    RuntimeOperationAction::Restart,
                    dependency("stopped"),
                )),
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
    }

    #[test]
    fn docker_commands_bind_one_frozen_local_host_and_clear_remote_selectors() {
        let target = docker_target();
        for args in [
            vec!["info"],
            vec!["ps", "-a"],
            vec!["inspect", CONTAINER_ID],
            vec!["start", CONTAINER_ID],
            vec!["stop", CONTAINER_ID],
            vec!["restart", CONTAINER_ID],
        ] {
            let command = docker_command(&target, args);
            assert_eq!(command.program, DOCKER_CLI);
            assert_eq!(command.args[0], "--host");
            assert_eq!(command.args[1], target.host);
            assert_eq!(command.env_remove, DOCKER_ENV_REMOVALS);
            assert!(!command.args.iter().any(|arg| arg.starts_with("tcp://")));
        }
        let unverified = OpenWebUiContext::Manage {
            endpoint: endpoint("http://localhost:3000"),
            dependency: dependency("running"),
            target: None,
        };
        assert!(build(
            "open-webui",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::OpenWebUi(unverified)
        )
        .is_err());
    }

    #[test]
    fn docker_target_is_exact_and_open_needs_no_target() {
        let home = Path::new("/Users/current");
        let expected = LocalDockerTarget {
            host: "unix:///Users/current/.docker/run/docker.sock".to_string(),
        };
        let same_suffix_wrong_home = LocalDockerTarget {
            host: "unix:///Users/other/.docker/run/docker.sock".to_string(),
        };
        assert!(is_expected_local_docker_target(&expected, home));
        assert!(!is_expected_local_docker_target(
            &same_suffix_wrong_home,
            home
        ));

        let plan = build(
            "docker-desktop",
            RuntimeOperationAction::Open,
            RuntimePlanningContext::Docker(DockerContext { target: None }),
        )
        .unwrap();
        let (commands, verification) = commands_and_verification(&plan.adapter);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, OPEN);
        assert_eq!(commands[0].args, ["-a", "Docker"]);
        assert_eq!(*verification, Verification::None);
    }

    #[test]
    fn plan_debug_redacts_urls_paths_tokens_and_raw_arguments() {
        let endpoint = classify_endpoint("http://localhost:3000/dashboard").unwrap();
        let target = LocalDockerTarget {
            host: "unix:///Users/private-user/.docker/run/docker.sock".to_string(),
        };
        let command = NativeCommand::new(
            LAUNCHCTL,
            [
                "bootstrap",
                "gui/501",
                "/Users/private-user/Library/LaunchAgents/secret.plist",
            ],
        );
        let verification = Verification::ContainerStopped {
            id: CONTAINER_ID.to_string(),
            target,
        };
        let values = format!("{endpoint:?} {command:?} {verification:?}");
        for secret in [
            "http://localhost:3000/dashboard",
            "/Users/private-user",
            "docker.sock",
            "secret.plist",
            "bootstrap",
            CONTAINER_ID,
        ] {
            assert!(!values.contains(secret), "Debug leaked {secret}");
        }
    }

    #[test]
    fn container_states_are_explicit_and_transitional_states_are_never_stopped() {
        let states = [
            ("running", DockerContainerState::Running),
            ("exited", DockerContainerState::Exited),
            ("created", DockerContainerState::Created),
            ("paused", DockerContainerState::Paused),
            ("restarting", DockerContainerState::Restarting),
            ("removing", DockerContainerState::Removing),
            ("dead", DockerContainerState::Dead),
            ("unexpected", DockerContainerState::Unknown),
        ];
        for (raw, expected) in states {
            assert_eq!(parse_container_state(raw), expected);
        }
        for state in [
            DockerContainerState::Paused,
            DockerContainerState::Restarting,
            DockerContainerState::Removing,
            DockerContainerState::Dead,
            DockerContainerState::Unknown,
        ] {
            let classified = classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[(CONTAINER_ID.to_string(), state)],
                None,
            );
            assert!(matches!(
                classified,
                OpenWebUiDependency::ContainerUnsupported { .. }
            ));
            let context = OpenWebUiContext::Manage {
                endpoint: endpoint("http://localhost:3000"),
                dependency: classified,
                target: Some(docker_target()),
            };
            assert!(build(
                "open-webui",
                RuntimeOperationAction::Start,
                RuntimePlanningContext::OpenWebUi(context)
            )
            .is_err());
        }
    }

    #[test]
    fn no_op_plans_revalidate_the_frozen_fact() {
        let ready_start = build(
            "open-webui",
            RuntimeOperationAction::Start,
            RuntimePlanningContext::OpenWebUi(webui_context(
                RuntimeOperationAction::Start,
                dependency("ready"),
            )),
        )
        .unwrap();
        assert!(commands_and_verification(&ready_start.adapter).0.is_empty());
        assert!(matches!(
            commands_and_verification(&ready_start.adapter).1,
            Verification::HttpReady(_)
        ));
        let stopped_stop = build(
            "open-webui",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::OpenWebUi(webui_context(
                RuntimeOperationAction::Stop,
                dependency("stopped"),
            )),
        )
        .unwrap();
        assert!(commands_and_verification(&stopped_stop.adapter)
            .0
            .is_empty());
        assert!(matches!(
            commands_and_verification(&stopped_stop.adapter).1,
            Verification::ContainerStopped { .. }
        ));
        let openclaw_stop = build(
            "openclaw",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::OpenClaw(openclaw_context("ws://localhost:18789", false)),
        )
        .unwrap();
        assert!(commands_and_verification(&openclaw_stop.adapter)
            .0
            .is_empty());
        assert!(matches!(
            commands_and_verification(&openclaw_stop.adapter).1,
            Verification::LaunchServiceNotLoaded(_)
        ));
    }

    #[test]
    fn dependency_inspection_failure_never_maps_to_missing() {
        assert_eq!(
            classify_open_webui_inspection(true, true, true, false, &[], None),
            OpenWebUiDependency::DockerInspectionFailed
        );
        assert_eq!(
            classify_open_webui_inspection(true, true, true, true, &[], None),
            OpenWebUiDependency::ContainerMissing
        );
        let missing_binary = NativeCommand::new("/definitely/missing/docker", ["info"])
            .with_env_removals(DOCKER_ENV_REMOVALS);
        let error = probe_native_status(&missing_binary, Duration::from_millis(10)).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::OperationFailed);
    }

    #[test]
    fn open_webui_management_rejects_every_dependency_failure_for_every_action() {
        let failures = [
            OpenWebUiDependency::DockerNotInstalled,
            OpenWebUiDependency::DockerInstalledStopped,
            OpenWebUiDependency::DockerProcessPresentDaemonUnavailable,
            OpenWebUiDependency::DockerInspectionFailed,
            OpenWebUiDependency::ContainerMissing,
            OpenWebUiDependency::ContainerAmbiguous,
        ];
        for action in [
            RuntimeOperationAction::Start,
            RuntimeOperationAction::Stop,
            RuntimeOperationAction::Restart,
        ] {
            for failure in failures.iter().cloned() {
                assert!(build(
                    "open-webui",
                    action,
                    RuntimePlanningContext::OpenWebUi(webui_context(action, failure))
                )
                .is_err());
            }
        }
        let invalid = OpenWebUiContext::Manage {
            endpoint: endpoint("http://localhost:3000"),
            dependency: OpenWebUiDependency::ContainerRunning {
                id: "invalid".to_string(),
            },
            target: Some(docker_target()),
        };
        assert!(build(
            "open-webui",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::OpenWebUi(invalid)
        )
        .is_err());
    }

    #[test]
    fn homebrew_formula_and_service_ownership_are_distinct() {
        assert!(!homebrew_service_managed(
            "Name Status User File\nollama none"
        ));
        assert!(homebrew_service_managed(
            "Name Status User File\nollama started test ~/Library/LaunchAgents/homebrew.mxcl.ollama.plist"
        ));
        let formula = ollama_context(
            "http://localhost:11434",
            OllamaInstallation::HomebrewFormulaInstalled {
                brew_path: "/opt/homebrew/bin/brew",
            },
        );
        assert!(build(
            "ollama",
            RuntimeOperationAction::Start,
            RuntimePlanningContext::Ollama(formula.clone())
        )
        .is_ok());
        assert_eq!(
            build(
                "ollama",
                RuntimeOperationAction::Stop,
                RuntimePlanningContext::Ollama(formula)
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
        let manual = ollama_context(
            "http://localhost:11434",
            OllamaInstallation::OtherInstallation,
        );
        assert!(build(
            "ollama",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::Ollama(manual)
        )
        .is_err());
    }

    #[test]
    fn lifecycle_commands_use_fixed_paths_and_never_plan_broad_kills() {
        let service = ollama_context(
            "http://localhost:11434",
            OllamaInstallation::HomebrewServiceManaged {
                brew_path: "/opt/homebrew/bin/brew",
            },
        );
        let plan = build(
            "ollama",
            RuntimeOperationAction::Stop,
            RuntimePlanningContext::Ollama(service),
        )
        .unwrap();
        let debug = format!("{:?}", commands_and_verification(&plan.adapter).0);
        assert!(!debug.contains("pkill"));
        assert!(!debug.contains("nohup"));
        assert_eq!(CURL, "/usr/bin/curl");
    }

    #[test]
    fn command_timeout_kills_only_the_owned_child_and_returns_safe_error() {
        let command = NativeCommand::new("/bin/sleep", ["1"]);
        let error = run_native_command(&command, Duration::from_millis(10)).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::ReadinessTimeout);
        assert!(!error.message.contains("sleep"));
        assert!(!error.message.contains("stdout"));
        assert!(!error.message.contains("stderr"));
    }

    #[test]
    fn verification_deadline_includes_time_spent_inside_probe() {
        let started = Instant::now();
        let error = verify_with_deadline(Duration::from_millis(10), |_| {
            thread::sleep(Duration::from_millis(20));
            Ok(false)
        })
        .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::ReadinessTimeout);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn readiness_polling_uses_a_bounded_low_frequency() {
        let probes = Cell::new(0_u32);
        let started = Instant::now();
        let _ = verify_with_deadline(Duration::from_millis(720), |_| {
            probes.set(probes.get() + 1);
            Ok(false)
        });
        assert!(started.elapsed() >= Duration::from_millis(700));
        assert!(probes.get() <= 4, "probe count was {}", probes.get());
    }

    #[test]
    fn dead_code_allowances_are_narrow_entry_point_annotations() {
        let source = include_str!("lifecycle.rs");
        let broad = ["#![allow(", "dead_code)]"].concat();
        let narrow = ["#[allow(", "dead_code)]"].concat();
        assert!(!source.contains(&broad));
        assert_eq!(source.matches(&narrow).count(), 2);
        let blocking_wait = ["child.", "wait()"].concat();
        assert!(!source.contains(&blocking_wait));
    }
}
