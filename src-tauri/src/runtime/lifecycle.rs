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
const MAX_CAPTURE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenClawGatewayEndpoint {
    url: Url,
    location: RuntimeLocation,
}

impl OpenClawGatewayEndpoint {
    fn browser_endpoint(&self) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
        let mut browser = self.url.clone();
        let scheme = match browser.scheme() {
            "http" | "https" => browser.scheme().to_string(),
            "ws" => "http".to_string(),
            "wss" => "https".to_string(),
            _ => return Err(invalid_configuration()),
        };
        browser
            .set_scheme(&scheme)
            .map_err(|_| invalid_configuration())?;
        Ok(ValidatedEndpoint {
            url: browser.to_string(),
            location: self.location,
        })
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
pub(crate) enum OllamaInstallation {
    NotInstalled,
    HomebrewFormulaInstalled { brew_path: &'static str },
    HomebrewServiceManaged { brew_path: &'static str },
    OtherInstallation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenWebUiDependency {
    DockerNotInstalled,
    DockerInstalledStopped,
    DockerProcessPresentDaemonUnavailable,
    DockerInspectionFailed,
    ContainerMissing,
    ContainerStopped { id: String },
    ContainerRunning { id: String },
    ContainerRunningEndpointUnavailable { id: String },
    ContainerReady { id: String },
    ContainerAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenClawContext {
    gateway: OpenClawGatewayEndpoint,
    launchctl_domain: String,
    service_loaded: bool,
    bootstrap_plist: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OllamaContext {
    endpoint: ValidatedEndpoint,
    installation: OllamaInstallation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DockerContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenWebUiContext {
    Open {
        endpoint: ValidatedEndpoint,
    },
    Manage {
        endpoint: ValidatedEndpoint,
        dependency: OpenWebUiDependency,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CherryContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimePlanningContext {
    OpenClaw(OpenClawContext),
    Ollama(OllamaContext),
    Docker(DockerContext),
    OpenWebUi(OpenWebUiContext),
    Cherry(CherryContext),
}

trait ContextSource {
    fn openclaw(&self) -> Result<OpenClawContext, NormalizedRuntimeError>;
    fn ollama(&self, endpoint: ValidatedEndpoint) -> OllamaContext;
    fn docker(&self) -> DockerContext;
    fn open_webui(&self, endpoint: ValidatedEndpoint, inspect_dependency: bool)
        -> OpenWebUiContext;
    fn cherry(&self) -> CherryContext;
}

struct NativeContextSource;

impl ContextSource for NativeContextSource {
    fn openclaw(&self) -> Result<OpenClawContext, NormalizedRuntimeError> {
        collect_openclaw_context()
    }

    fn ollama(&self, endpoint: ValidatedEndpoint) -> OllamaContext {
        OllamaContext {
            endpoint,
            installation: detect_ollama_installation(),
        }
    }

    fn docker(&self) -> DockerContext {
        DockerContext
    }

    fn open_webui(
        &self,
        endpoint: ValidatedEndpoint,
        inspect_dependency: bool,
    ) -> OpenWebUiContext {
        if inspect_dependency {
            let dependency = inspect_open_webui_dependency(&endpoint);
            OpenWebUiContext::Manage {
                endpoint,
                dependency,
            }
        } else {
            OpenWebUiContext::Open { endpoint }
        }
    }

    fn cherry(&self) -> CherryContext {
        CherryContext
    }
}

// M1B2B will call this request-specific collector before accepting execution.
#[allow(dead_code)]
pub(crate) fn collect_context_for(
    request: &RuntimeLifecycleRequest,
) -> Result<RuntimePlanningContext, NormalizedRuntimeError> {
    collect_context_with(request, &NativeContextSource)
}

fn collect_context_with(
    request: &RuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimePlanningContext, NormalizedRuntimeError> {
    match request.runtime_id.as_str() {
        "openclaw" => source.openclaw().map(RuntimePlanningContext::OpenClaw),
        "ollama" => {
            let endpoint = explicit_endpoint(request)?;
            Ok(RuntimePlanningContext::Ollama(source.ollama(endpoint)))
        }
        "docker-desktop" => Ok(RuntimePlanningContext::Docker(source.docker())),
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

fn collect_openclaw_context() -> Result<OpenClawContext, NormalizedRuntimeError> {
    let endpoint = openclaw::active_runtime_endpoint()
        .map_err(|_| configuration_unavailable())?
        .ok_or_else(configuration_unavailable)?;
    let gateway = classify_openclaw_gateway(&endpoint)?;
    let launchctl_domain = current_launchctl_domain()?;
    let target = format!("{launchctl_domain}/{OPENCLAW_SERVICE}");
    let service_loaded =
        probe_status(LAUNCHCTL, &["print", &target], PROBE_TIMEOUT).unwrap_or(false);
    let bootstrap_plist = dirs::home_dir()
        .map(|home| home.join("Library/LaunchAgents/ai.openclaw.gateway.plist"))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned());
    Ok(OpenClawContext {
        gateway,
        launchctl_domain,
        service_loaded,
        bootstrap_plist,
    })
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

fn inspect_open_webui_dependency(endpoint: &ValidatedEndpoint) -> OpenWebUiDependency {
    let docker_process_running = probe_status(
        PGREP,
        &["-f", "/Docker.app/Contents/MacOS/Docker"],
        PROBE_TIMEOUT,
    )
    .unwrap_or(false)
        || probe_status(PGREP, &["-x", "Docker Desktop"], PROBE_TIMEOUT).unwrap_or(false);
    let docker_installed = docker_process_running || Path::new("/Applications/Docker.app").exists();
    let daemon_ready = probe_status(DOCKER_CLI, &["info"], PROBE_TIMEOUT).unwrap_or(false);
    if !docker_installed || !daemon_ready {
        return classify_open_webui_inspection(
            docker_installed,
            docker_process_running,
            daemon_ready,
            false,
            &[],
            None,
        );
    }
    let output = capture_command(
        DOCKER_CLI,
        &[
            "ps",
            "-a",
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.State}}",
        ],
        PROBE_TIMEOUT,
    );
    let Ok(output) = output else {
        return OpenWebUiDependency::DockerInspectionFailed;
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
    classify_open_webui_inspection(
        true,
        docker_process_running,
        true,
        true,
        &candidates,
        endpoint_ready,
    )
}

fn parse_open_webui_candidate(line: &str) -> Option<(String, bool)> {
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
        .map(|id| (id, fields[3].eq_ignore_ascii_case("running")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCommand {
    program: &'static str,
    args: Vec<String>,
}

impl NativeCommand {
    fn new(program: &'static str, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program,
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Verification {
    None,
    HttpReady(ValidatedEndpoint),
    HttpStopped(ValidatedEndpoint),
    DockerReady,
    DockerStopped,
    ProcessPresent(&'static str),
    ProcessAbsent(&'static str),
    OpenClawReady { service_target: String },
    LaunchServiceStopped(String),
    ContainerStopped(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeExecutionPlan {
    runtime_id: String,
    action: RuntimeOperationAction,
    effective_location: RuntimeLocation,
    adapter: RuntimeAdapterPlan,
    progress: Vec<RuntimeOperationProgress>,
}

// M1B2B will connect validated plans to the operation manager.
#[allow(dead_code)]
pub(crate) fn build_execution_plan(
    request: &RuntimeLifecycleRequest,
    context: RuntimePlanningContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    match (request.runtime_id.as_str(), context) {
        ("openclaw", RuntimePlanningContext::OpenClaw(context)) => {
            plan_openclaw(request.action, context)
        }
        ("ollama", RuntimePlanningContext::Ollama(context)) => plan_ollama(request.action, context),
        ("docker-desktop", RuntimePlanningContext::Docker(_)) => plan_docker(request.action),
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
    let location = context.gateway.location;
    if location == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let endpoint = context.gateway.browser_endpoint()?;
        let commands = vec![open_url(&endpoint)];
        return Ok(plan(
            "openclaw",
            action,
            location,
            RuntimeAdapterPlan::OpenClawRemoteOpen { endpoint, commands },
            &["validating", "opening", "complete"],
        ));
    }

    let service_target = format!("{}/{}", context.launchctl_domain, OPENCLAW_SERVICE);
    let (commands, verification, phases) = match action {
        RuntimeOperationAction::Start => {
            let mut commands = Vec::new();
            if !context.service_loaded {
                let plist = context.bootstrap_plist.ok_or_else(|| {
                    error(
                        RuntimeErrorCode::ConfigurationUnavailable,
                        "The OpenClaw launch service is not installed.",
                        false,
                    )
                })?;
                commands.push(NativeCommand::new(
                    LAUNCHCTL,
                    [
                        "bootstrap",
                        context.launchctl_domain.as_str(),
                        plist.as_str(),
                    ],
                ));
            }
            commands.push(NativeCommand::new(
                LAUNCHCTL,
                ["kickstart", "-k", service_target.as_str()],
            ));
            (
                commands,
                Verification::OpenClawReady { service_target },
                &[
                    "validating",
                    "starting-application",
                    "verifying",
                    "complete",
                ][..],
            )
        }
        RuntimeOperationAction::Stop => (
            vec![NativeCommand::new(
                LAUNCHCTL,
                ["bootout", service_target.as_str()],
            )],
            Verification::LaunchServiceStopped(service_target),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => {
            let endpoint = context.gateway.browser_endpoint()?;
            (
                vec![open_url(&endpoint)],
                Verification::None,
                &["validating", "opening", "complete"][..],
            )
        }
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
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (commands, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            vec![NativeCommand::new(OPEN, ["-a", "Docker"])],
            Verification::DockerReady,
            &[
                "validating",
                "starting-application",
                "waiting-for-readiness",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            vec![NativeCommand::new(DOCKER_CLI, ["desktop", "stop"])],
            Verification::DockerStopped,
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => (
            vec![NativeCommand::new(OPEN, ["-a", "Docker"])],
            Verification::None,
            &["validating", "opening", "complete"][..],
        ),
        RuntimeOperationAction::Restart => return Err(unsupported()),
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
    let (endpoint, dependency) = match context {
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
        } => (endpoint, dependency),
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
        _ => None,
    };
    if let Some(error) = rejected {
        return Err(error);
    }

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
                commands: vec![NativeCommand::new(DOCKER_CLI, ["start", id.as_str()])],
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
                endpoint,
                container_id: id,
                verification: Verification::None,
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
                container_id: id,
                verification: Verification::None,
            },
            &["validating", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Stop => (
            RuntimeAdapterPlan::OpenWebUiContainer {
                endpoint,
                container_id: id.clone(),
                commands: vec![NativeCommand::new(DOCKER_CLI, ["stop", id.as_str()])],
                verification: Verification::ContainerStopped(id),
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
                commands: vec![NativeCommand::new(DOCKER_CLI, ["restart", id.as_str()])],
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
    let mut child = Command::new(command.program)
        .args(&command.args)
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
            let _ = child.kill();
            let _ = child.wait();
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

fn probe_status(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<bool, NormalizedRuntimeError> {
    let command = NativeCommand::new(program_path(program)?, args.iter().copied());
    match run_native_command(&command, timeout) {
        Ok(()) => Ok(true),
        Err(error) if error.code == RuntimeErrorCode::OperationFailed => Ok(false),
        Err(error) => Err(error),
    }
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
        thread::sleep(POLL_INTERVAL.min(remaining));
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
        Verification::DockerReady => probe_status(DOCKER_CLI, &["info"], probe_timeout),
        Verification::DockerStopped => Ok(!probe_status(DOCKER_CLI, &["info"], probe_timeout)?),
        Verification::ProcessPresent(name) => probe_status(PGREP, &["-x", name], probe_timeout),
        Verification::ProcessAbsent(name) => {
            Ok(!probe_status(PGREP, &["-x", name], probe_timeout)?)
        }
        Verification::OpenClawReady { service_target } => {
            let loaded = probe_status(LAUNCHCTL, &["print", service_target], probe_timeout)?;
            let connected = openclaw::runtime_snapshot()
                .map(|snapshot| snapshot.connection_state == "connected")
                .unwrap_or(false);
            Ok(loaded && connected)
        }
        Verification::LaunchServiceStopped(target) => {
            Ok(!probe_status(LAUNCHCTL, &["print", target], probe_timeout)?)
        }
        Verification::ContainerStopped(id) => {
            Ok(docker_container_running(id, probe_timeout)? == Some(false))
        }
    }
}

fn docker_container_running(
    id: &str,
    timeout: Duration,
) -> Result<Option<bool>, NormalizedRuntimeError> {
    let output = capture_command(
        DOCKER_CLI,
        &["inspect", "--format", "{{.State.Running}}", id],
        timeout,
    )?;
    Ok(match output.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub(crate) fn classify_open_webui_inspection(
    docker_installed: bool,
    docker_process_running: bool,
    daemon_ready: bool,
    inspection_succeeded: bool,
    candidates: &[(String, bool)],
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
        [(id, false)] => OpenWebUiDependency::ContainerStopped { id: id.clone() },
        [(id, true)] if endpoint_ready == Some(true) => {
            OpenWebUiDependency::ContainerReady { id: id.clone() }
        }
        [(id, true)] if endpoint_ready == Some(false) => {
            OpenWebUiDependency::ContainerRunningEndpointUnavailable { id: id.clone() }
        }
        [(id, true)] => OpenWebUiDependency::ContainerRunning { id: id.clone() },
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

    fn openclaw_context(value: &str, loaded: bool) -> OpenClawContext {
        OpenClawContext {
            gateway: classify_openclaw_gateway(value).unwrap(),
            launchctl_domain: "gui/501".to_string(),
            service_loaded: loaded,
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
        let ws = classify_openclaw_gateway("ws://localhost:18789/gateway?mode=safe").unwrap();
        assert_eq!(ws.location, RuntimeLocation::Local);
        assert_eq!(
            ws.browser_endpoint().unwrap().as_str(),
            "http://localhost:18789/gateway?mode=safe"
        );
        let wss = classify_openclaw_gateway("wss://gateway.example.com/path").unwrap();
        assert_eq!(wss.location, RuntimeLocation::Remote);
        assert_eq!(
            wss.browser_endpoint().unwrap().as_str(),
            "https://gateway.example.com/path"
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
        assert!(matches!(verification, Verification::OpenClawReady { .. }));
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

    struct RecordingSource {
        openclaw_calls: Cell<u32>,
        ollama_calls: Cell<u32>,
        docker_calls: Cell<u32>,
        webui_calls: Cell<u32>,
        cherry_calls: Cell<u32>,
    }

    impl RecordingSource {
        fn new() -> Self {
            Self {
                openclaw_calls: Cell::new(0),
                ollama_calls: Cell::new(0),
                docker_calls: Cell::new(0),
                webui_calls: Cell::new(0),
                cherry_calls: Cell::new(0),
            }
        }
    }

    impl ContextSource for RecordingSource {
        fn openclaw(&self) -> Result<OpenClawContext, NormalizedRuntimeError> {
            self.openclaw_calls.set(self.openclaw_calls.get() + 1);
            Err(configuration_unavailable())
        }

        fn ollama(&self, endpoint: ValidatedEndpoint) -> OllamaContext {
            self.ollama_calls.set(self.ollama_calls.get() + 1);
            OllamaContext {
                endpoint,
                installation: OllamaInstallation::OtherInstallation,
            }
        }

        fn docker(&self) -> DockerContext {
            self.docker_calls.set(self.docker_calls.get() + 1);
            DockerContext
        }

        fn open_webui(
            &self,
            endpoint: ValidatedEndpoint,
            inspect_dependency: bool,
        ) -> OpenWebUiContext {
            self.webui_calls.set(self.webui_calls.get() + 1);
            assert!(inspect_dependency);
            OpenWebUiContext::Manage {
                endpoint,
                dependency: OpenWebUiDependency::ContainerMissing,
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
                assert_eq!(command.args, vec!["start", CONTAINER_ID]);
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
                commands_and_verification(&stop.adapter).0[0].args[0],
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
                commands_and_verification(&restart.adapter).0[0].args[0],
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
    fn dependency_inspection_failure_never_maps_to_missing() {
        assert_eq!(
            classify_open_webui_inspection(true, true, true, false, &[], None),
            OpenWebUiDependency::DockerInspectionFailed
        );
        assert_eq!(
            classify_open_webui_inspection(true, true, true, true, &[], None),
            OpenWebUiDependency::ContainerMissing
        );
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
    fn dead_code_allowances_are_narrow_entry_point_annotations() {
        let source = include_str!("lifecycle.rs");
        let broad = ["#![allow(", "dead_code)]"].concat();
        let narrow = ["#[allow(", "dead_code)]"].concat();
        assert!(!source.contains(&broad));
        assert_eq!(source.matches(&narrow).count(), 3);
    }
}
