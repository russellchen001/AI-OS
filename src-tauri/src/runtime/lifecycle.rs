#![allow(dead_code)]

use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use url::{Host, Url};

use crate::{health, openclaw};

use super::models::{
    NormalizedRuntimeError, RuntimeErrorCode, RuntimeLocation, RuntimeOperationAction,
    RuntimeOperationProgress,
};

const OPENCLAW_SERVICE: &str = "ai.openclaw.gateway";
const DOCKER_CLI: &str = "/Applications/Docker.app/Contents/Resources/bin/docker";
const OPEN: &str = "/usr/bin/open";
const LAUNCHCTL: &str = "/bin/launchctl";
const OSASCRIPT: &str = "/usr/bin/osascript";
const CHERRY_APP: &str = "Cherry Studio";
const CHERRY_QUIT_SCRIPT: &str = "tell application \"Cherry Studio\" to quit";
const MAX_VERIFY_ATTEMPTS: usize = 30;

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

pub(crate) fn classify_endpoint(value: &str) -> Result<ValidatedEndpoint, NormalizedRuntimeError> {
    let parsed = Url::parse(value.trim()).map_err(|_| invalid_configuration())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(invalid_configuration());
    }

    let location = classify_url_host(&parsed).ok_or_else(invalid_configuration)?;

    Ok(ValidatedEndpoint {
        url: parsed.to_string(),
        location,
    })
}

pub(crate) fn classify_runtime_url(value: &str) -> Option<RuntimeLocation> {
    Url::parse(value).ok().as_ref().and_then(classify_url_host)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OllamaInstallation {
    NotInstalled,
    HomebrewManaged { brew_path: &'static str },
    Other,
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
pub(crate) struct LifecycleContext {
    pub openclaw_endpoint: Option<String>,
    pub launchctl_domain: String,
    pub openclaw_service_loaded: bool,
    pub openclaw_bootstrap_plist: Option<String>,
    pub ollama_installation: OllamaInstallation,
    pub open_webui_dependency: OpenWebUiDependency,
}

pub(crate) fn collect_lifecycle_context(
    open_webui_endpoint: Option<&ValidatedEndpoint>,
) -> Result<LifecycleContext, NormalizedRuntimeError> {
    let launchctl_domain = current_launchctl_domain()?;
    let service_target = format!("{launchctl_domain}/{OPENCLAW_SERVICE}");
    let openclaw_bootstrap_plist = dirs::home_dir()
        .map(|home| home.join("Library/LaunchAgents/ai.openclaw.gateway.plist"))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned());
    Ok(LifecycleContext {
        openclaw_endpoint: openclaw::active_runtime_endpoint().map_err(|_| {
            error(
                RuntimeErrorCode::ConfigurationUnavailable,
                "OpenClaw configuration could not be read.",
                true,
            )
        })?,
        launchctl_domain,
        openclaw_service_loaded: command_ok(LAUNCHCTL, &["print", &service_target]),
        openclaw_bootstrap_plist,
        ollama_installation: detect_ollama_installation(),
        open_webui_dependency: inspect_open_webui_dependency(open_webui_endpoint),
    })
}

fn current_launchctl_domain() -> Result<String, NormalizedRuntimeError> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| operation_failed())?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !uid.is_empty() && uid.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(format!("gui/{uid}"))
    } else {
        Err(operation_failed())
    }
}

fn detect_ollama_installation() -> OllamaInstallation {
    for brew in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if Path::new(brew).is_file() && command_ok(brew, &["list", "--formula", "ollama"]) {
            return OllamaInstallation::HomebrewManaged { brew_path: brew };
        }
    }
    if Path::new("/opt/homebrew/bin/ollama").is_file()
        || Path::new("/usr/local/bin/ollama").is_file()
        || Path::new("/Applications/Ollama.app").exists()
    {
        OllamaInstallation::Other
    } else {
        OllamaInstallation::NotInstalled
    }
}

fn inspect_open_webui_dependency(endpoint: Option<&ValidatedEndpoint>) -> OpenWebUiDependency {
    let docker_process_running = health::probe_docker_process();
    let docker_installed = docker_process_running || Path::new("/Applications/Docker.app").exists();
    let daemon_ready = command_ok(DOCKER_CLI, &["info"]);
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
    let output = Command::new(DOCKER_CLI)
        .args([
            "ps",
            "-a",
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.State}}",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return OpenWebUiDependency::DockerInspectionFailed;
    };
    if !output.status.success() {
        return OpenWebUiDependency::DockerInspectionFailed;
    }
    let candidates = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_open_webui_candidate)
        .collect::<Vec<_>>();
    let endpoint_ready =
        endpoint.map(|value| command_ok("curl", &["-fsS", "--max-time", "3", value.as_str()]));
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
    OpenClawReady {
        endpoint: ValidatedEndpoint,
        service_target: String,
    },
    LaunchServiceLoaded(String),
    LaunchServiceStopped(String),
    ContainerRunning(String),
    ContainerStopped(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeAdapterPlan {
    OpenClawLocal {
        command: NativeCommand,
        verification: Verification,
    },
    OpenClawRemoteOpen {
        endpoint: ValidatedEndpoint,
        command: NativeCommand,
    },
    OllamaHomebrewLocal {
        endpoint: ValidatedEndpoint,
        command: NativeCommand,
        verification: Verification,
    },
    OllamaRemoteOpen {
        endpoint: ValidatedEndpoint,
        command: NativeCommand,
    },
    DockerDesktop {
        command: NativeCommand,
        verification: Verification,
    },
    OpenWebUiLocalContainer {
        endpoint: ValidatedEndpoint,
        container_id: String,
        command: NativeCommand,
        verification: Verification,
    },
    OpenWebUiRemoteOpen {
        endpoint: ValidatedEndpoint,
        command: NativeCommand,
    },
    CherryStudio {
        command: NativeCommand,
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

pub(crate) fn build_execution_plan(
    request: &RuntimeLifecycleRequest,
    context: &LifecycleContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    match request.runtime_id.as_str() {
        "openclaw" => plan_openclaw(request.action, context),
        "ollama" => plan_ollama(request.action, request.endpoint_url.as_deref(), context),
        "docker-desktop" => plan_docker(request.action),
        "open-webui" => plan_open_webui(request.action, request.endpoint_url.as_deref(), context),
        "cherry-studio" => plan_cherry(request.action),
        _ => Err(error(
            RuntimeErrorCode::RuntimeNotFound,
            "The requested runtime is not available.",
            false,
        )),
    }
}

fn plan_openclaw(
    action: RuntimeOperationAction,
    context: &LifecycleContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let endpoint = classify_endpoint(
        context
            .openclaw_endpoint
            .as_deref()
            .ok_or_else(invalid_configuration)?,
    )?;
    if endpoint.location() == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let command = open_url(&endpoint);
        return Ok(plan(
            "openclaw",
            action,
            RuntimeLocation::Remote,
            RuntimeAdapterPlan::OpenClawRemoteOpen { endpoint, command },
            &["validating", "opening", "complete"],
        ));
    }
    let service_target = format!("{}/{}", context.launchctl_domain, OPENCLAW_SERVICE);
    let (command, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            if context.openclaw_service_loaded {
                NativeCommand::new(LAUNCHCTL, ["kickstart", "-k", service_target.as_str()])
            } else {
                let plist = context.openclaw_bootstrap_plist.as_deref().ok_or_else(|| {
                    error(
                        RuntimeErrorCode::ConfigurationUnavailable,
                        "The OpenClaw launch service is not installed.",
                        false,
                    )
                })?;
                NativeCommand::new(
                    LAUNCHCTL,
                    ["bootstrap", context.launchctl_domain.as_str(), plist],
                )
            },
            Verification::OpenClawReady {
                endpoint: endpoint.clone(),
                service_target,
            },
            &[
                "validating",
                "starting-application",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            NativeCommand::new(LAUNCHCTL, ["bootout", service_target.as_str()]),
            Verification::LaunchServiceStopped(service_target),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => (
            open_url(&endpoint),
            Verification::None,
            &["validating", "opening", "complete"][..],
        ),
        RuntimeOperationAction::Restart => return Err(unsupported()),
    };
    Ok(plan(
        "openclaw",
        action,
        RuntimeLocation::Local,
        RuntimeAdapterPlan::OpenClawLocal {
            command,
            verification,
        },
        phases,
    ))
}

fn plan_ollama(
    action: RuntimeOperationAction,
    endpoint: Option<&str>,
    context: &LifecycleContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let endpoint = classify_endpoint(endpoint.ok_or_else(invalid_configuration)?)?;
    if endpoint.location() == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let command = open_url(&endpoint);
        return Ok(plan(
            "ollama",
            action,
            RuntimeLocation::Remote,
            RuntimeAdapterPlan::OllamaRemoteOpen { endpoint, command },
            &["validating", "opening", "complete"],
        ));
    }
    if action == RuntimeOperationAction::Restart {
        return Err(unsupported());
    }
    if action == RuntimeOperationAction::Open {
        let command = open_url(&endpoint);
        return Ok(plan(
            "ollama",
            action,
            RuntimeLocation::Local,
            RuntimeAdapterPlan::OllamaHomebrewLocal {
                endpoint,
                command,
                verification: Verification::None,
            },
            &["validating", "opening", "complete"],
        ));
    }
    match context.ollama_installation {
        OllamaInstallation::NotInstalled => {
            return Err(error(
                RuntimeErrorCode::DependencyNotInstalled,
                "Ollama is not installed.",
                false,
            ))
        }
        OllamaInstallation::Other => return Err(unsupported()),
        OllamaInstallation::HomebrewManaged { .. } => {}
    }
    let OllamaInstallation::HomebrewManaged { brew_path: brew } = context.ollama_installation
    else {
        unreachable!()
    };
    let (verb, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            "start",
            Verification::HttpReady(endpoint.clone()),
            &[
                "validating",
                "starting-application",
                "waiting-for-readiness",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            "stop",
            Verification::HttpStopped(endpoint.clone()),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        _ => unreachable!(),
    };
    let command = NativeCommand::new(brew, ["services", verb, "ollama"]);
    Ok(plan(
        "ollama",
        action,
        RuntimeLocation::Local,
        RuntimeAdapterPlan::OllamaHomebrewLocal {
            endpoint,
            command,
            verification,
        },
        phases,
    ))
}

fn plan_docker(
    action: RuntimeOperationAction,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (command, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            NativeCommand::new(OPEN, ["-a", "Docker"]),
            Verification::DockerReady,
            &[
                "validating",
                "starting-application",
                "waiting-for-readiness",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            NativeCommand::new(DOCKER_CLI, ["desktop", "stop"]),
            Verification::DockerStopped,
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => (
            NativeCommand::new(OPEN, ["-a", "Docker"]),
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
            command,
            verification,
        },
        phases,
    ))
}

fn plan_open_webui(
    action: RuntimeOperationAction,
    endpoint: Option<&str>,
    context: &LifecycleContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let endpoint = classify_endpoint(endpoint.ok_or_else(invalid_configuration)?)?;
    if endpoint.location() == RuntimeLocation::Remote {
        if action != RuntimeOperationAction::Open {
            return Err(invalid_location());
        }
        let command = open_url(&endpoint);
        return Ok(plan(
            "open-webui",
            action,
            RuntimeLocation::Remote,
            RuntimeAdapterPlan::OpenWebUiRemoteOpen { endpoint, command },
            &["validating", "opening", "complete"],
        ));
    }
    if action == RuntimeOperationAction::Open {
        let command = open_url(&endpoint);
        return Ok(plan(
            "open-webui",
            action,
            RuntimeLocation::Local,
            RuntimeAdapterPlan::OpenWebUiLocalContainer {
                endpoint,
                container_id: String::new(),
                command,
                verification: Verification::None,
            },
            &["validating", "opening", "complete"],
        ));
    }
    let id = match &context.open_webui_dependency {
        OpenWebUiDependency::DockerNotInstalled => {
            return Err(error(
                RuntimeErrorCode::DependencyNotInstalled,
                "Docker Desktop is not installed.",
                false,
            ))
        }
        OpenWebUiDependency::DockerInstalledStopped
        | OpenWebUiDependency::DockerProcessPresentDaemonUnavailable => {
            return Err(error(
                RuntimeErrorCode::DependencyUnavailable,
                "Docker must be running before managing Open WebUI.",
                true,
            ))
        }
        OpenWebUiDependency::DockerInspectionFailed => {
            return Err(error(
                RuntimeErrorCode::ProbeFailed,
                "Docker container inspection failed.",
                true,
            ))
        }
        OpenWebUiDependency::ContainerMissing => {
            return Err(error(
                RuntimeErrorCode::ContainerNotFound,
                "The Open WebUI container was not found.",
                false,
            ))
        }
        OpenWebUiDependency::ContainerAmbiguous => {
            return Err(error(
                RuntimeErrorCode::ContainerAmbiguous,
                "Multiple Open WebUI containers were found.",
                false,
            ))
        }
        OpenWebUiDependency::ContainerStopped { id }
        | OpenWebUiDependency::ContainerRunning { id }
        | OpenWebUiDependency::ContainerRunningEndpointUnavailable { id }
        | OpenWebUiDependency::ContainerReady { id } => validate_container_id(id)?,
    };
    let (verb, verification, phase) = match action {
        RuntimeOperationAction::Start => (
            "start",
            Verification::HttpReady(endpoint.clone()),
            "starting-container",
        ),
        RuntimeOperationAction::Stop => (
            "stop",
            Verification::ContainerStopped(id.clone()),
            "stopping-container",
        ),
        RuntimeOperationAction::Restart => (
            "restart",
            Verification::HttpReady(endpoint.clone()),
            "restarting-container",
        ),
        RuntimeOperationAction::Open => unreachable!(),
    };
    let command = NativeCommand::new(DOCKER_CLI, [verb, id.as_str()]);
    Ok(plan(
        "open-webui",
        action,
        RuntimeLocation::Local,
        RuntimeAdapterPlan::OpenWebUiLocalContainer {
            endpoint,
            container_id: id,
            command,
            verification,
        },
        &[
            "validating",
            "checking-dependency",
            "locating-container",
            phase,
            "verifying",
            "complete",
        ],
    ))
}

fn plan_cherry(
    action: RuntimeOperationAction,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let (command, verification, phases) = match action {
        RuntimeOperationAction::Start => (
            NativeCommand::new(OPEN, ["-a", CHERRY_APP]),
            Verification::ProcessPresent(CHERRY_APP),
            &[
                "validating",
                "starting-application",
                "verifying",
                "complete",
            ][..],
        ),
        RuntimeOperationAction::Stop => (
            NativeCommand::new(OSASCRIPT, ["-e", CHERRY_QUIT_SCRIPT]),
            Verification::ProcessAbsent(CHERRY_APP),
            &["validating", "stopping-service", "verifying", "complete"][..],
        ),
        RuntimeOperationAction::Open => (
            NativeCommand::new(OPEN, ["-a", CHERRY_APP]),
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
            command,
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
        "locating-container" => "Locating the runtime container.",
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
        .unwrap_or(1);
    for update in plan.progress.iter().take(command_phase + 1) {
        report(update.clone());
    }
    let (command, verification) = command_and_verification(&plan.adapter);
    let status = Command::new(command.program)
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| operation_failed())?;
    if !status.success() {
        return Err(operation_failed());
    }
    for update in plan
        .progress
        .iter()
        .skip(command_phase + 1)
        .take_while(|update| update.phase != "complete")
    {
        report(update.clone());
    }
    verify(verification)?;
    if let Some(complete) = plan.progress.last() {
        report(complete.clone());
    }
    Ok(())
}

fn command_and_verification(adapter: &RuntimeAdapterPlan) -> (&NativeCommand, &Verification) {
    match adapter {
        RuntimeAdapterPlan::OpenClawLocal {
            command,
            verification,
        }
        | RuntimeAdapterPlan::OllamaHomebrewLocal {
            command,
            verification,
            ..
        }
        | RuntimeAdapterPlan::DockerDesktop {
            command,
            verification,
        }
        | RuntimeAdapterPlan::OpenWebUiLocalContainer {
            command,
            verification,
            ..
        }
        | RuntimeAdapterPlan::CherryStudio {
            command,
            verification,
        } => (command, verification),
        RuntimeAdapterPlan::OpenClawRemoteOpen { command, .. }
        | RuntimeAdapterPlan::OllamaRemoteOpen { command, .. }
        | RuntimeAdapterPlan::OpenWebUiRemoteOpen { command, .. } => {
            static NONE: Verification = Verification::None;
            (command, &NONE)
        }
    }
}

fn verify(verification: &Verification) -> Result<(), NormalizedRuntimeError> {
    if *verification == Verification::None {
        return Ok(());
    }
    for _ in 0..MAX_VERIFY_ATTEMPTS {
        if verification_satisfied(verification) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err(readiness_timeout())
}

fn readiness_timeout() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::ReadinessTimeout,
        "The runtime did not reach the expected state in time.",
        true,
    )
}

fn verification_satisfied(verification: &Verification) -> bool {
    match verification {
        Verification::None => true,
        Verification::HttpReady(endpoint) => {
            command_ok("curl", &["-fsS", "--max-time", "3", endpoint.as_str()])
        }
        Verification::HttpStopped(endpoint) => {
            !command_ok("curl", &["-fsS", "--max-time", "3", endpoint.as_str()])
        }
        Verification::DockerReady => command_ok(DOCKER_CLI, &["info"]),
        Verification::DockerStopped => !command_ok(DOCKER_CLI, &["info"]),
        Verification::ProcessPresent(name) => command_ok("/usr/bin/pgrep", &["-x", name]),
        Verification::ProcessAbsent(name) => !command_ok("/usr/bin/pgrep", &["-x", name]),
        Verification::OpenClawReady {
            endpoint,
            service_target,
        } => {
            command_ok(LAUNCHCTL, &["print", service_target])
                && command_ok("curl", &["-fsS", "--max-time", "3", endpoint.as_str()])
        }
        Verification::LaunchServiceLoaded(target) => command_ok(LAUNCHCTL, &["print", target]),
        Verification::LaunchServiceStopped(target) => !command_ok(LAUNCHCTL, &["print", target]),
        Verification::ContainerRunning(id) => docker_container_running(id) == Some(true),
        Verification::ContainerStopped(id) => docker_container_running(id) == Some(false),
    }
}

fn docker_container_running(id: &str) -> Option<bool> {
    let output = Command::new(DOCKER_CLI)
        .args(["inspect", "--format", "{{.State.Running}}", id])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&output.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn command_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
        "A valid HTTP or HTTPS endpoint is required.",
        false,
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

fn operation_failed() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::OperationFailed,
        "The native runtime operation failed.",
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
    use super::*;

    fn context() -> LifecycleContext {
        LifecycleContext {
            openclaw_endpoint: Some("http://localhost:18789".to_string()),
            launchctl_domain: "gui/501".to_string(),
            openclaw_service_loaded: true,
            openclaw_bootstrap_plist: Some(
                "/Users/test/Library/LaunchAgents/ai.openclaw.gateway.plist".to_string(),
            ),
            ollama_installation: OllamaInstallation::HomebrewManaged {
                brew_path: "/opt/homebrew/bin/brew",
            },
            open_webui_dependency: OpenWebUiDependency::ContainerReady {
                id: "0123456789abcdef".to_string(),
            },
        }
    }

    fn request(
        runtime: &str,
        action: RuntimeOperationAction,
        endpoint: Option<&str>,
    ) -> RuntimeLifecycleRequest {
        RuntimeLifecycleRequest {
            runtime_id: runtime.to_string(),
            action,
            endpoint_url: endpoint.map(str::to_string),
        }
    }

    #[test]
    fn endpoint_safety_classifies_loopback_and_remote_hosts() {
        for value in [
            "http://localhost:1",
            "http://127.0.0.1:1",
            "http://127.4.5.6:1",
            "http://[::1]:1",
        ] {
            assert_eq!(
                classify_endpoint(value).unwrap().location(),
                RuntimeLocation::Local
            );
        }
        for value in ["https://example.com", "http://192.168.1.10"] {
            assert_eq!(
                classify_endpoint(value).unwrap().location(),
                RuntimeLocation::Remote
            );
        }
    }

    #[test]
    fn endpoint_safety_rejects_invalid_hostless_credentials_and_schemes() {
        for value in [
            "not a url",
            "file:///tmp/socket",
            "javascript:alert(1)",
            "data:text/plain,x",
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
    fn plan_freezes_values_from_mutable_request_and_context() {
        let mut request = request(
            "ollama",
            RuntimeOperationAction::Open,
            Some("http://localhost:11434"),
        );
        let mut context = context();
        let plan = build_execution_plan(&request, &context).unwrap();
        request.endpoint_url = Some("https://remote.example.com".to_string());
        context.ollama_installation = OllamaInstallation::Other;
        match plan.adapter {
            RuntimeAdapterPlan::OllamaHomebrewLocal { endpoint, .. } => {
                assert_eq!(endpoint.as_str(), "http://localhost:11434/")
            }
            _ => panic!("expected frozen local plan"),
        }
    }

    #[test]
    fn openclaw_plans_local_actions_denies_remote_lifecycle_and_restart() {
        let local = build_execution_plan(
            &request("openclaw", RuntimeOperationAction::Start, None),
            &context(),
        )
        .unwrap();
        let command = command_and_verification(&local.adapter).0;
        assert_eq!(command.program, LAUNCHCTL);
        assert!(!command.args.iter().any(|value| value.contains("pkill")));
        assert_eq!(
            build_execution_plan(
                &request("openclaw", RuntimeOperationAction::Restart, None),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
        let mut remote = context();
        remote.openclaw_endpoint = Some("https://gateway.example.com".to_string());
        assert_eq!(
            build_execution_plan(
                &request("openclaw", RuntimeOperationAction::Stop, None),
                &remote
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::InvalidRuntimeLocation
        );
    }

    #[test]
    fn openclaw_start_deliberately_bootstraps_an_unloaded_service() {
        let mut context = context();
        context.openclaw_service_loaded = false;
        let plan = build_execution_plan(
            &request("openclaw", RuntimeOperationAction::Start, None),
            &context,
        )
        .unwrap();
        let command = command_and_verification(&plan.adapter).0;
        assert_eq!(command.program, LAUNCHCTL);
        assert_eq!(command.args[0], "bootstrap");
        assert_eq!(command.args[1], "gui/501");
        assert!(command.args[2].ends_with("ai.openclaw.gateway.plist"));
    }

    #[test]
    fn ollama_requires_endpoint_and_homebrew_ownership_for_lifecycle() {
        assert_eq!(
            build_execution_plan(
                &request("ollama", RuntimeOperationAction::Start, None),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::InvalidConfiguration
        );
        let mut other = context();
        other.ollama_installation = OllamaInstallation::Other;
        assert_eq!(
            build_execution_plan(
                &request(
                    "ollama",
                    RuntimeOperationAction::Stop,
                    Some("http://localhost:11434")
                ),
                &other
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
        assert_eq!(
            build_execution_plan(
                &request(
                    "ollama",
                    RuntimeOperationAction::Start,
                    Some("https://ollama.example.com")
                ),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::InvalidRuntimeLocation
        );
        let plan = build_execution_plan(
            &request(
                "ollama",
                RuntimeOperationAction::Start,
                Some("http://localhost:11434"),
            ),
            &context(),
        )
        .unwrap();
        let command = command_and_verification(&plan.adapter).0;
        assert_eq!(command.program, "/opt/homebrew/bin/brew");
        assert_eq!(command.args, vec!["services", "start", "ollama"]);
        assert!(!format!("{command:?}").contains("nohup"));
        assert!(!format!("{command:?}").contains("pkill"));
    }

    #[test]
    fn docker_uses_static_commands_and_rejects_restart() {
        let plan = build_execution_plan(
            &request("docker-desktop", RuntimeOperationAction::Start, None),
            &context(),
        )
        .unwrap();
        let command = command_and_verification(&plan.adapter).0;
        assert_eq!(command, &NativeCommand::new(OPEN, ["-a", "Docker"]));
        assert_eq!(
            build_execution_plan(
                &request("docker-desktop", RuntimeOperationAction::Restart, None),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
    }

    #[test]
    fn open_webui_distinguishes_dependency_and_container_states() {
        assert_eq!(
            classify_open_webui_inspection(false, false, false, false, &[], None),
            OpenWebUiDependency::DockerNotInstalled
        );
        assert_eq!(
            classify_open_webui_inspection(true, true, false, false, &[], None),
            OpenWebUiDependency::DockerProcessPresentDaemonUnavailable
        );
        assert_eq!(
            classify_open_webui_inspection(true, true, true, false, &[], Some(false)),
            OpenWebUiDependency::DockerInspectionFailed
        );
        assert_eq!(
            classify_open_webui_inspection(true, false, false, false, &[], Some(false)),
            OpenWebUiDependency::DockerInstalledStopped
        );
        assert_eq!(
            classify_open_webui_inspection(true, true, true, true, &[], Some(false)),
            OpenWebUiDependency::ContainerMissing
        );
        assert_eq!(
            classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[("a".into(), true), ("b".into(), false)],
                Some(false)
            ),
            OpenWebUiDependency::ContainerAmbiguous
        );
        assert_eq!(
            classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[("0123456789ab".into(), false)],
                Some(false),
            ),
            OpenWebUiDependency::ContainerStopped {
                id: "0123456789ab".into()
            }
        );
        assert_eq!(
            classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[("0123456789ab".into(), true)],
                None,
            ),
            OpenWebUiDependency::ContainerRunning {
                id: "0123456789ab".into()
            }
        );
        assert_eq!(
            classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[("0123456789ab".into(), true)],
                Some(false),
            ),
            OpenWebUiDependency::ContainerRunningEndpointUnavailable {
                id: "0123456789ab".into()
            }
        );
        assert_eq!(
            classify_open_webui_inspection(
                true,
                true,
                true,
                true,
                &[("0123456789ab".into(), true)],
                Some(true),
            ),
            OpenWebUiDependency::ContainerReady {
                id: "0123456789ab".into()
            }
        );
    }

    #[test]
    fn open_webui_uses_confirmed_id_and_never_starts_docker() {
        for action in [
            RuntimeOperationAction::Start,
            RuntimeOperationAction::Stop,
            RuntimeOperationAction::Restart,
        ] {
            let plan = build_execution_plan(
                &request("open-webui", action, Some("http://localhost:3000")),
                &context(),
            )
            .unwrap();
            let command = command_and_verification(&plan.adapter).0;
            assert_eq!(command.program, DOCKER_CLI);
            assert_eq!(command.args.last().unwrap(), "0123456789abcdef");
            assert!(!command.args.iter().any(|arg| arg == "desktop"));
        }
        assert_eq!(
            build_execution_plan(
                &request(
                    "open-webui",
                    RuntimeOperationAction::Start,
                    Some("https://webui.example.com")
                ),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::InvalidRuntimeLocation
        );
    }

    #[test]
    fn cherry_uses_fixed_application_and_graceful_quit_without_pkill() {
        let plan = build_execution_plan(
            &request("cherry-studio", RuntimeOperationAction::Stop, None),
            &context(),
        )
        .unwrap();
        let command = command_and_verification(&plan.adapter).0;
        assert_eq!(command.program, OSASCRIPT);
        assert_eq!(command.args, vec!["-e", CHERRY_QUIT_SCRIPT]);
        assert!(!format!("{:?}", command).contains("pkill"));
        assert_eq!(
            build_execution_plan(
                &request("cherry-studio", RuntimeOperationAction::Restart, None),
                &context()
            )
            .unwrap_err()
            .code,
            RuntimeErrorCode::UnsupportedOperation
        );
    }

    #[test]
    fn normalized_errors_do_not_include_raw_process_or_configuration_data() {
        for error in [
            invalid_configuration(),
            invalid_location(),
            unsupported(),
            operation_failed(),
        ] {
            let message = error.message.to_lowercase();
            assert!(!message.contains("stderr"));
            assert!(!message.contains("stdout"));
            assert!(!message.contains("token"));
            assert!(!message.contains("/users/"));
        }
    }

    #[test]
    fn readiness_timeout_has_a_stable_safe_mapping() {
        let error = readiness_timeout();
        assert_eq!(error.code, RuntimeErrorCode::ReadinessTimeout);
        assert!(error.retryable);
        assert!(!error.message.contains("stderr"));
    }

    #[test]
    fn rejected_plan_cannot_reach_the_execution_primitive() {
        let mut executions = 0;
        let result = build_execution_plan(
            &request(
                "ollama",
                RuntimeOperationAction::Start,
                Some("file:///tmp/ollama"),
            ),
            &context(),
        );
        if let Ok(plan) = result {
            executions += 1;
            let _ = plan;
        }
        assert_eq!(executions, 0);
    }

    #[test]
    fn user_url_is_an_explicit_open_argument_not_a_shell_command() {
        let plan = build_execution_plan(
            &request(
                "ollama",
                RuntimeOperationAction::Open,
                Some("https://ollama.example.com/path?q=value"),
            ),
            &context(),
        )
        .unwrap();
        let command = command_and_verification(&plan.adapter).0;
        assert_eq!(command.program, OPEN);
        assert_eq!(command.args.len(), 1);
        assert!(command.args[0].starts_with("https://ollama.example.com/"));
    }
}
