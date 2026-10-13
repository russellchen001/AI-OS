use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use url::{Host, Url};

use crate::openclaw;

use super::models::{
    NormalizedRuntimeError, RuntimeAdapterKind, RuntimeErrorCode, RuntimeLocation,
    RuntimeOperationAction, RuntimeOperationProgress,
};
use super::registry;

const OPENCLAW_SERVICE: &str = "ai.openclaw.gateway";
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
pub(crate) const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);
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

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeLifecycleRequest {
    pub(crate) runtime_id: String,
    pub(crate) action: RuntimeOperationAction,
    pub(crate) endpoint_url: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ValidatedRuntimeLifecycleRequest {
    runtime_id: String,
    adapter_kind: RuntimeAdapterKind,
    action: RuntimeOperationAction,
    endpoint: Option<ValidatedEndpoint>,
}

impl ValidatedRuntimeLifecycleRequest {
    #[cfg(test)]
    pub(crate) fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    #[cfg(test)]
    pub(crate) fn action(&self) -> RuntimeOperationAction {
        self.action
    }

    pub(crate) fn adapter_kind(&self) -> RuntimeAdapterKind {
        self.adapter_kind
    }
}

impl std::fmt::Debug for ValidatedRuntimeLifecycleRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedRuntimeLifecycleRequest")
            .field("runtime_id", &self.runtime_id)
            .field("adapter_kind", &self.adapter_kind)
            .field("action", &self.action)
            .field("endpoint_present", &self.endpoint.is_some())
            .finish()
    }
}

pub(crate) fn validate_runtime_lifecycle_request(
    request: RuntimeLifecycleRequest,
) -> Result<ValidatedRuntimeLifecycleRequest, NormalizedRuntimeError> {
    if request.action == RuntimeOperationAction::Execute {
        return Err(unsupported());
    }
    let adapter_kind = registry::adapter_kind(&request.runtime_id).ok_or_else(runtime_not_found)?;
    if request.action == RuntimeOperationAction::Restart {
        return Err(unsupported());
    }

    let endpoint = match adapter_kind {
        RuntimeAdapterKind::Openclaw | RuntimeAdapterKind::CherryStudio => None,
    };

    Ok(ValidatedRuntimeLifecycleRequest {
        runtime_id: request.runtime_id,
        adapter_kind,
        action: request.action,
        endpoint,
    })
}

impl std::fmt::Debug for RuntimeLifecycleRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeLifecycleRequest")
            .field("runtime_id", &self.runtime_id)
            .field("action", &self.action)
            .field("endpoint_present", &self.endpoint_url.is_some())
            .finish()
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchctlPrintOutcome {
    SuccessfulExit,
    ServiceNotFound,
    OtherNonzeroExit,
    SpawnFailure,
    Timeout,
    InternalWaitFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CherryContext;

#[derive(Clone, PartialEq, Eq)]
enum RuntimePlanningContext {
    OpenClaw(OpenClawContext),
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

impl std::fmt::Debug for RuntimePlanningContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::OpenClaw(_) => "OpenClaw",
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
    fn cherry(&self) -> CherryContext;
}

struct NativeContextSource {
    deadline: Instant,
}

impl ContextSource for NativeContextSource {
    fn openclaw(
        &self,
        action: RuntimeOperationAction,
    ) -> Result<OpenClawContext, NormalizedRuntimeError> {
        collect_openclaw_context(action, self.deadline)
    }

    fn cherry(&self) -> CherryContext {
        CherryContext
    }
}

pub(crate) fn prepare_execution_plan(
    request: &ValidatedRuntimeLifecycleRequest,
    deadline: Instant,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    if Instant::now() >= deadline {
        return Err(readiness_timeout());
    }
    let plan = prepare_validated_with_source(request, &NativeContextSource { deadline })?;
    ensure_preparation_time(deadline)?;
    Ok(plan)
}

#[cfg(test)]
fn prepare_with_source(
    request: &RuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let validated = validate_runtime_lifecycle_request(request.clone())?;
    prepare_validated_with_source(&validated, source)
}

fn prepare_validated_with_source(
    request: &ValidatedRuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    let context = collect_validated_context_with(request, source)?;
    build_execution_plan(request.adapter_kind(), request.action, context)
}

#[cfg(test)]
fn collect_context_with(
    request: &RuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimePlanningContext, NormalizedRuntimeError> {
    let validated = validate_runtime_lifecycle_request(request.clone())?;
    collect_validated_context_with(&validated, source)
}

fn collect_validated_context_with(
    request: &ValidatedRuntimeLifecycleRequest,
    source: &impl ContextSource,
) -> Result<RuntimePlanningContext, NormalizedRuntimeError> {
    match request.adapter_kind() {
        RuntimeAdapterKind::Openclaw => source
            .openclaw(request.action)
            .map(RuntimePlanningContext::OpenClaw),
        RuntimeAdapterKind::CherryStudio => Ok(RuntimePlanningContext::Cherry(source.cherry())),
    }
}

fn collect_openclaw_context(
    action: RuntimeOperationAction,
    deadline: Instant,
) -> Result<OpenClawContext, NormalizedRuntimeError> {
    ensure_preparation_time(deadline)?;
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
    if gateway.location == RuntimeLocation::Remote {
        return Ok(OpenClawContext::Open { gateway });
    }
    let launchctl_domain = current_launchctl_domain(deadline)?;
    let target = format!("{launchctl_domain}/{OPENCLAW_SERVICE}");
    let service_state = inspect_launch_service(&target, remaining_probe_time(deadline)?);
    ensure_preparation_time(deadline)?;
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
    classify_launch_service_outcome(run_launchctl_print(target, timeout))
}

fn classify_launch_service_outcome(outcome: LaunchctlPrintOutcome) -> LaunchServiceState {
    match outcome {
        LaunchctlPrintOutcome::SuccessfulExit => LaunchServiceState::Loaded,
        LaunchctlPrintOutcome::ServiceNotFound => LaunchServiceState::NotLoaded,
        LaunchctlPrintOutcome::OtherNonzeroExit
        | LaunchctlPrintOutcome::SpawnFailure
        | LaunchctlPrintOutcome::Timeout
        | LaunchctlPrintOutcome::InternalWaitFailure => LaunchServiceState::InspectionFailed,
    }
}

fn run_launchctl_print(target: &str, timeout: Duration) -> LaunchctlPrintOutcome {
    let mut child = match Command::new(LAUNCHCTL)
        .args(["print", target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return LaunchctlPrintOutcome::SpawnFailure,
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return LaunchctlPrintOutcome::SuccessfulExit;
            }
            Ok(Some(status)) if status.code() == Some(113) => {
                return LaunchctlPrintOutcome::ServiceNotFound;
            }
            Ok(Some(_)) => return LaunchctlPrintOutcome::OtherNonzeroExit,
            Err(_) => return LaunchctlPrintOutcome::InternalWaitFailure,
            Ok(None) if Instant::now() >= deadline => {
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
                return LaunchctlPrintOutcome::Timeout;
            }
            Ok(None) => {
                thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())))
            }
        }
    }
}

fn current_launchctl_domain(deadline: Instant) -> Result<String, NormalizedRuntimeError> {
    let output = capture_command("/usr/bin/id", &["-u"], remaining_probe_time(deadline)?)?;
    let uid = output.trim();
    if !uid.is_empty() && uid.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(format!("gui/{uid}"))
    } else {
        Err(operation_failed())
    }
}

fn ensure_preparation_time(deadline: Instant) -> Result<(), NormalizedRuntimeError> {
    if Instant::now() >= deadline {
        Err(readiness_timeout())
    } else {
        Ok(())
    }
}

fn remaining_probe_time(deadline: Instant) -> Result<Duration, NormalizedRuntimeError> {
    ensure_preparation_time(deadline)?;
    Ok(deadline
        .saturating_duration_since(Instant::now())
        .min(PROBE_TIMEOUT))
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

#[derive(Clone, PartialEq, Eq)]
enum Verification {
    None,
    ProcessPresent(&'static str),
    ProcessAbsent(&'static str),
    LaunchServiceLoaded(String),
    LaunchServiceNotLoaded(String),


}

impl Verification {
    fn kind(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::ProcessPresent(_) => "ProcessPresent",
            Self::ProcessAbsent(_) => "ProcessAbsent",
            Self::LaunchServiceLoaded(_) => "LaunchServiceLoaded",
            Self::LaunchServiceNotLoaded(_) => "LaunchServiceNotLoaded",
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
    adapter_kind: RuntimeAdapterKind,
    action: RuntimeOperationAction,
    context: RuntimePlanningContext,
) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
    match adapter_kind {
        RuntimeAdapterKind::Openclaw => match context {
            RuntimePlanningContext::OpenClaw(context) => plan_openclaw(action, context),
            _ => Err(context_mismatch()),
        },
        RuntimeAdapterKind::CherryStudio => match context {
            RuntimePlanningContext::Cherry(_) => plan_cherry(action),
            _ => Err(context_mismatch()),
        },
    }
}

fn context_mismatch() -> NormalizedRuntimeError {
    error(
        RuntimeErrorCode::InvalidConfiguration,
        "The runtime context does not match the request.",
        false,
    )
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
        RuntimeOperationAction::Execute => return Err(unsupported()),
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
        RuntimeOperationAction::Execute => return Err(unsupported()),
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
        "opening" => "Opening the runtime.",
        "verifying" => "Verifying the runtime state.",
        "complete" => "Runtime operation complete.",
        _ => "Updating runtime operation.",
    }
}

fn open_url(endpoint: &ValidatedEndpoint) -> NativeCommand {
    NativeCommand::new(OPEN, [endpoint.as_str()])
}

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
        RuntimeAdapterPlan::OpenClawLocal { commands, verification }
        | RuntimeAdapterPlan::CherryStudio { commands, verification } => (commands, verification),
        RuntimeAdapterPlan::OpenClawRemoteOpen { commands, .. } => {
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
        "/usr/bin/id" => Ok("/usr/bin/id"),
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

    fn request(runtime_id: &str, action: RuntimeOperationAction, endpoint: Option<&str>) -> RuntimeLifecycleRequest {
        RuntimeLifecycleRequest { runtime_id: runtime_id.to_string(), action, endpoint_url: endpoint.map(str::to_string) }
    }
    fn endpoint(value: &str) -> ValidatedEndpoint { classify_endpoint(value).unwrap() }
    fn openclaw_context(value: &str, loaded: bool) -> OpenClawContext {
        OpenClawContext::Manage {
            gateway: classify_openclaw_gateway(value).unwrap(),
            launchctl_domain: "gui/501".to_string(),
            service_state: if loaded { LaunchServiceState::Loaded } else { LaunchServiceState::NotLoaded },
            bootstrap_plist: Some("/Users/test/Library/LaunchAgents/ai.openclaw.gateway.plist".to_string()),
        }
    }
    fn build(runtime: &str, action: RuntimeOperationAction, context: RuntimePlanningContext) -> Result<RuntimeExecutionPlan, NormalizedRuntimeError> {
        build_execution_plan(registry::adapter_kind(runtime).unwrap(), action, context)
    }
    #[test]
    fn supported_runtime_restart_is_rejected() {
        for id in ["openclaw", "cherry-studio"] {
            assert_eq!(validate_runtime_lifecycle_request(request(id, RuntimeOperationAction::Restart, None)).unwrap_err().code, RuntimeErrorCode::UnsupportedOperation);
        }
    }
    #[test]
    fn expired_deadline_never_prepares_execution() {
        let validated = validate_runtime_lifecycle_request(request("cherry-studio", RuntimeOperationAction::Open, None)).unwrap();
        assert_eq!(prepare_execution_plan(&validated, Instant::now()).unwrap_err().code, RuntimeErrorCode::ReadinessTimeout);
    }
    #[test]
    fn native_source_remains_runtime_isolated() {
        let cherry = validate_runtime_lifecycle_request(request("cherry-studio", RuntimeOperationAction::Open, None)).unwrap();
        let plan = prepare_execution_plan(&cherry, Instant::now() + Duration::from_secs(10)).unwrap();
        assert_eq!(plan.runtime_id, "cherry-studio");
        assert!(matches!(plan.adapter, RuntimeAdapterPlan::CherryStudio { .. }));
    }
    #[test]
    fn cherry_plans_use_fixed_commands_and_no_broad_kills() {
        for action in [RuntimeOperationAction::Start, RuntimeOperationAction::Stop, RuntimeOperationAction::Open] {
            let plan = build("cherry-studio", action, RuntimePlanningContext::Cherry(CherryContext)).unwrap();
            let commands = commands_and_verification(&plan.adapter).0;
            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0].program, OPEN | OSASCRIPT));
            assert!(!format!("{commands:?}").contains("pkill"));
        }
    }
    #[test]
    fn stopped_openclaw_revalidates_no_op() {
        let plan = build("openclaw", RuntimeOperationAction::Stop, RuntimePlanningContext::OpenClaw(openclaw_context("ws://localhost:18789", false))).unwrap();
        let (commands, verification) = commands_and_verification(&plan.adapter);
        assert!(commands.is_empty());
        assert!(matches!(verification, Verification::LaunchServiceNotLoaded(_)));
    }
    #[test]
    fn debug_redacts_sensitive_execution_data() {
        let cmd = NativeCommand::new(LAUNCHCTL, ["bootstrap", "gui/501", "/Users/private/secret.plist"]);
        let plan = build("openclaw", RuntimeOperationAction::Start, RuntimePlanningContext::OpenClaw(openclaw_context("ws://localhost:18789?token=secret-token", true))).unwrap();
        let output = format!("{cmd:?} {plan:?}");
        assert!(!output.contains("secret-token"));
        assert!(!output.contains("/Users/private"));
        assert!(!output.contains("bootstrap"));
    }
    #[test]
    fn every_registered_runtime_freezes_explicit_adapter_dispatch() {
        for definition in registry::definitions() {
            let endpoint = None;
            let validated = validate_runtime_lifecycle_request(request(
                &definition.id,
                RuntimeOperationAction::Open,
                endpoint,
            ))
            .unwrap();
            assert_eq!(validated.adapter_kind(), definition.adapter_kind);
        }
        let source = include_str!("lifecycle.rs");
        let dispatch = source
            .split("fn collect_validated_context_with")
            .nth(1)
            .unwrap()
            .split("fn collect_openclaw_context")
            .next()
            .unwrap();
        assert!(!dispatch.contains(&["unreachable", "!()"].concat()));
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
            classify_launch_service_outcome(LaunchctlPrintOutcome::SuccessfulExit),
            LaunchServiceState::Loaded
        );
        assert_eq!(
            classify_launch_service_outcome(LaunchctlPrintOutcome::ServiceNotFound),
            LaunchServiceState::NotLoaded
        );
        for outcome in [
            LaunchctlPrintOutcome::OtherNonzeroExit,
            LaunchctlPrintOutcome::SpawnFailure,
            LaunchctlPrintOutcome::Timeout,
            LaunchctlPrintOutcome::InternalWaitFailure,
        ] {
            assert_eq!(
                classify_launch_service_outcome(outcome),
                LaunchServiceState::InspectionFailed
            );
        }
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

    #[test]
    fn lifecycle_request_rejects_execute_action() {
        let error = validate_runtime_lifecycle_request(request(
            "openclaw",
            RuntimeOperationAction::Execute,
            None,
        ))
        .unwrap_err();

        assert_eq!(error.code, RuntimeErrorCode::UnsupportedOperation);
    }

    #[test]
    fn lifecycle_request_debug_contains_only_safe_presence_metadata() {
        let request = request(
            "openclaw",
            RuntimeOperationAction::Start,
            Some("https://user:password@example.com/private?token=secret#fragment"),
        );
        let debug = format!("{request:?}");
        assert!(debug.contains("openclaw"));
        assert!(debug.contains("Start"));
        assert!(debug.contains("endpoint_present"));
        assert!(debug.contains("true"));
        for secret in [
            "https://",
            "example.com",
            "private",
            "token",
            "secret",
            "fragment",
            "password",
        ] {
            assert!(!debug.contains(secret));
        }
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
        assert_eq!(source.matches(&narrow).count(), 0);
        let blocking_wait = ["child.", "wait()"].concat();
        assert!(!source.contains(&blocking_wait));
    }
}
