use serde_json::{json, Value};
use std::{
    env,
    error::Error,
    fmt, fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

const DEFAULT_MAX_STEPS: u16 = 25;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const STOP_GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const LOCAL_MINIMUM_MEMORY_BYTES: u64 = 32 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManoMode {
    Local,
    Cloud,
}

impl ManoMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ManoFallbackRequest {
    pub(crate) execution_id: String,
    pub(crate) task_id: String,
    pub(crate) plan_id: String,
    pub(crate) step_id: String,
    pub(crate) goal: String,
    pub(crate) capability: String,
    pub(crate) input: Value,
    pub(crate) user_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManoFallbackProgress {
    pub(crate) phase: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ManoFallbackResult {
    pub(crate) output: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManoProbeResult {
    pub(crate) mode: ManoMode,
    pub(crate) concurrency: u8,
    pub(crate) primary_display_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManoFallbackErrorKind {
    InvalidRequest,
    PermissionDenied,
    Unsupported,
    Unavailable,
    AlreadyRunning,
    Cancelled,
    TimedOut,
    ExecutionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManoFallbackError {
    pub(crate) kind: ManoFallbackErrorKind,
    pub(crate) message: String,
    pub(crate) retryable: bool,
}

impl ManoFallbackError {
    fn new(kind: ManoFallbackErrorKind, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable,
        }
    }
}

impl fmt::Display for ManoFallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ManoFallbackError {}

pub(crate) trait ManoFallbackAdapter: Send + Sync {
    fn probe(&self) -> Result<ManoProbeResult, ManoFallbackError>;

    fn execute(
        &self,
        request: &ManoFallbackRequest,
        report: &mut dyn FnMut(ManoFallbackProgress),
    ) -> Result<ManoFallbackResult, ManoFallbackError>;

    fn cancel(&self, execution_id: &str) -> Result<(), ManoFallbackError>;
}

#[derive(Debug, Clone)]
struct ManoPolicy {
    mode: ManoMode,
    cloud_authorized: bool,
    max_steps: u16,
    timeout: Duration,
    local_hardware_supported: bool,
}

impl ManoPolicy {
    fn from_environment() -> Result<Self, ManoFallbackError> {
        let mode = match env::var("AI_OS_MANO_MODE")
            .unwrap_or_else(|_| "local".to_owned())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "local" => ManoMode::Local,
            "cloud" => ManoMode::Cloud,
            _ => {
                return Err(ManoFallbackError::new(
                    ManoFallbackErrorKind::Unavailable,
                    "Mano mode configuration is invalid.",
                    false,
                ))
            }
        };
        let cloud_authorized = env_flag("AI_OS_MANO_CLOUD_AUTHORIZED");
        let max_steps =
            bounded_env_u64("AI_OS_MANO_MAX_STEPS", DEFAULT_MAX_STEPS as u64, 1, 100) as u16;
        let timeout_seconds = bounded_env_u64(
            "AI_OS_MANO_TIMEOUT_SECONDS",
            DEFAULT_TIMEOUT.as_secs(),
            30,
            30 * 60,
        );

        Ok(Self {
            mode,
            cloud_authorized,
            max_steps,
            timeout: Duration::from_secs(timeout_seconds),
            local_hardware_supported: local_hardware_supported(),
        })
    }
}

struct ActiveExecution {
    execution_id: String,
    child: Child,
    started_at: Instant,
    cancel_requested_at: Option<Instant>,
    timed_out: bool,
}

pub(crate) struct ManagedManoCliFallback {
    executable: PathBuf,
    local_stop_flag: PathBuf,
    policy: Result<ManoPolicy, ManoFallbackError>,
    active: Mutex<Option<ActiveExecution>>,
}

impl ManagedManoCliFallback {
    pub(crate) fn production() -> Self {
        Self {
            executable: env::var_os("AI_OS_MANO_CUA_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("mano-cua")),
            local_stop_flag: dirs::home_dir().unwrap_or_default().join(".mano/stop.flag"),
            policy: ManoPolicy::from_environment(),
            active: Mutex::new(None),
        }
    }

    fn policy(&self) -> Result<&ManoPolicy, ManoFallbackError> {
        self.policy.as_ref().map_err(Clone::clone)
    }

    fn run_control_command(&self, args: &[&str]) -> Result<(), ManoFallbackError> {
        let mut child = Command::new(&self.executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable())?;
        let deadline = Instant::now() + STOP_GRACE;

        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => return Err(unavailable()),
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ManoFallbackError::new(
                        ManoFallbackErrorKind::Unavailable,
                        "Mano-CUA readiness command timed out.",
                        true,
                    ));
                }
                Err(_) => return Err(unavailable()),
            }
        }
    }

    fn request_stop(&self) -> Result<(), ManoFallbackError> {
        match self.policy()?.mode {
            ManoMode::Local => {
                let parent = self.local_stop_flag.parent().ok_or_else(execution_failed)?;
                fs::create_dir_all(parent).map_err(|_| execution_failed())?;
                fs::write(&self.local_stop_flag, []).map_err(|_| execution_failed())
            }
            ManoMode::Cloud => self.run_control_command(&["stop"]),
        }
    }

    fn finish_active(
        &self,
        execution_id: &str,
    ) -> Result<(std::process::ExitStatus, bool, bool), ManoFallbackError> {
        loop {
            let mut request_stop = false;
            let terminal = {
                let mut active = self.active.lock().map_err(|_| execution_failed())?;
                let running = active.as_mut().ok_or_else(execution_failed)?;
                if running.execution_id != execution_id {
                    return Err(execution_failed());
                }

                if !running.timed_out && running.cancel_requested_at.is_none() {
                    let policy = self.policy()?;
                    if running.started_at.elapsed() >= policy.timeout {
                        running.timed_out = true;
                        running.cancel_requested_at = Some(Instant::now());
                        request_stop = true;
                    }
                }

                if let Some(requested_at) = running.cancel_requested_at {
                    if requested_at.elapsed() >= STOP_GRACE {
                        let _ = running.child.kill();
                    }
                }

                match running.child.try_wait() {
                    Ok(Some(status)) => Some((
                        status,
                        running.cancel_requested_at.is_some(),
                        running.timed_out,
                    )),
                    Ok(None) => None,
                    Err(_) => return Err(execution_failed()),
                }
            };

            if request_stop {
                let _ = self.request_stop();
            }
            if let Some(result) = terminal {
                let mut active = self.active.lock().map_err(|_| execution_failed())?;
                active.take();
                return Ok(result);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

impl ManoFallbackAdapter for ManagedManoCliFallback {
    fn probe(&self) -> Result<ManoProbeResult, ManoFallbackError> {
        let policy = self.policy()?;
        if policy.mode == ManoMode::Cloud && !policy.cloud_authorized {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::PermissionDenied,
                "Mano Cloud execution is not explicitly authorized.",
                false,
            ));
        }
        self.run_control_command(&["run", "--help"])?;
        self.run_control_command(&["stop", "--help"])?;
        if policy.mode == ManoMode::Local && !policy.local_hardware_supported {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::Unsupported,
                "Mano Local is unsupported on this hardware policy.",
                false,
            ));
        }

        if policy.mode == ManoMode::Local {
            self.run_control_command(&["check"])?;
        }

        Ok(ManoProbeResult {
            mode: policy.mode,
            concurrency: 1,
            primary_display_only: true,
        })
    }

    fn execute(
        &self,
        request: &ManoFallbackRequest,
        report: &mut dyn FnMut(ManoFallbackProgress),
    ) -> Result<ManoFallbackResult, ManoFallbackError> {
        if request.execution_id.trim().is_empty()
            || request.task_id.trim().is_empty()
            || request.plan_id.trim().is_empty()
            || request.step_id.trim().is_empty()
            || request.goal.trim().is_empty()
            || request.capability.trim().is_empty()
        {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::InvalidRequest,
                "Mano fallback request is invalid.",
                false,
            ));
        }
        if !request.user_confirmed {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::PermissionDenied,
                "Mano GUI fallback requires explicit user confirmation.",
                false,
            ));
        }
        if contains_sensitive_material(request) {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::PermissionDenied,
                "Mano GUI fallback cannot receive credentials or secret material.",
                false,
            ));
        }

        let probe = self.probe()?;
        let policy = self.policy()?;
        let mut args = vec![
            "run".to_owned(),
            request.goal.clone(),
            "--minimize".to_owned(),
            "--max-steps".to_owned(),
            policy.max_steps.to_string(),
        ];
        if probe.mode == ManoMode::Local {
            args.push("--local".to_owned());
        }

        report(ManoFallbackProgress {
            phase: "starting".to_owned(),
            message: "Starting bounded Mano-CUA fallback execution.".to_owned(),
        });

        let mut active = self.active.lock().map_err(|_| execution_failed())?;
        if active.is_some() {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::AlreadyRunning,
                "Mano-CUA already has an active execution.",
                true,
            ));
        }
        let child = Command::new(&self.executable)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable())?;
        *active = Some(ActiveExecution {
            execution_id: request.execution_id.clone(),
            child,
            started_at: Instant::now(),
            cancel_requested_at: None,
            timed_out: false,
        });
        drop(active);

        report(ManoFallbackProgress {
            phase: "executing".to_owned(),
            message: "Mano-CUA fallback execution is active.".to_owned(),
        });
        let (status, cancelled, timed_out) = self.finish_active(&request.execution_id)?;
        if timed_out {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::TimedOut,
                "Mano-CUA execution exceeded its Runtime bound.",
                true,
            ));
        }
        if cancelled {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::Cancelled,
                "Mano-CUA execution was cancelled.",
                false,
            ));
        }
        if !status.success() {
            return Err(execution_failed());
        }

        report(ManoFallbackProgress {
            phase: "completed".to_owned(),
            message: "Mano-CUA fallback execution completed.".to_owned(),
        });
        Ok(ManoFallbackResult {
            output: json!({
                "executor": "mano-cua",
                "mode": probe.mode.as_str(),
                "status": "completed"
            }),
        })
    }

    fn cancel(&self, execution_id: &str) -> Result<(), ManoFallbackError> {
        let execution_id = execution_id.trim();
        if execution_id.is_empty() {
            return Err(ManoFallbackError::new(
                ManoFallbackErrorKind::InvalidRequest,
                "Mano execution identity is invalid.",
                false,
            ));
        }
        {
            let mut active = self.active.lock().map_err(|_| execution_failed())?;
            let running = active.as_mut().ok_or_else(|| {
                ManoFallbackError::new(
                    ManoFallbackErrorKind::InvalidRequest,
                    "No matching Mano-CUA execution is active.",
                    false,
                )
            })?;
            if running.execution_id != execution_id {
                return Err(ManoFallbackError::new(
                    ManoFallbackErrorKind::InvalidRequest,
                    "No matching Mano-CUA execution is active.",
                    false,
                ));
            }
            if running.cancel_requested_at.is_none() {
                running.cancel_requested_at = Some(Instant::now());
            }
        }
        self.request_stop()
    }
}

impl Drop for ManagedManoCliFallback {
    fn drop(&mut self) {
        if self.active.get_mut().is_ok_and(|active| active.is_some()) {
            let _ = self.request_stop();
        }
        if let Ok(active) = self.active.get_mut() {
            if let Some(running) = active.as_mut() {
                let _ = running.child.kill();
                let _ = running.child.wait();
            }
        }
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn bounded_env_u64(name: &str, default: u64, minimum: u64, maximum: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

fn contains_sensitive_material(request: &ManoFallbackRequest) -> bool {
    let goal = request.goal.to_ascii_lowercase();
    [
        "password",
        "passcode",
        "one-time code",
        "verification code",
        "api key",
        "access token",
        "secret",
        "密码",
        "验证码",
        "口令",
        "密钥",
    ]
    .iter()
    .any(|marker| goal.contains(marker))
        || value_contains_sensitive_key(&request.input)
}

fn value_contains_sensitive_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            [
                "password",
                "passcode",
                "token",
                "secret",
                "credential",
                "apikey",
                "api_key",
            ]
            .iter()
            .any(|marker| key.contains(marker))
                || value_contains_sensitive_key(value)
        }),
        Value::Array(values) => values.iter().any(value_contains_sensitive_key),
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn local_hardware_supported() -> bool {
    let memory = sysctl_value("hw.memsize")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let chip = sysctl_value("machdep.cpu.brand_string").unwrap_or_default();
    let generation = chip
        .split_whitespace()
        .find_map(|part| part.strip_prefix('M')?.parse::<u8>().ok())
        .unwrap_or(0);
    memory >= LOCAL_MINIMUM_MEMORY_BYTES && generation >= 4
}

#[cfg(not(target_os = "macos"))]
fn local_hardware_supported() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn sysctl_value(name: &str) -> Option<String> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", name])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn unavailable() -> ManoFallbackError {
    ManoFallbackError::new(
        ManoFallbackErrorKind::Unavailable,
        "Mano-CUA is unavailable or not ready.",
        true,
    )
}

fn execution_failed() -> ManoFallbackError {
    ManoFallbackError::new(
        ManoFallbackErrorKind::ExecutionFailed,
        "Mano-CUA execution failed.",
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

    fn policy(mode: ManoMode, cloud_authorized: bool, local_ready: bool) -> ManoPolicy {
        ManoPolicy {
            mode,
            cloud_authorized,
            max_steps: 3,
            timeout: Duration::from_secs(30),
            local_hardware_supported: local_ready,
        }
    }

    fn adapter(executable: &str, policy: ManoPolicy) -> ManagedManoCliFallback {
        ManagedManoCliFallback {
            executable: PathBuf::from(executable),
            local_stop_flag: PathBuf::from("/tmp/ai-os-unused-mano-stop.flag"),
            policy: Ok(policy),
            active: Mutex::new(None),
        }
    }

    fn request(confirmed: bool) -> ManoFallbackRequest {
        ManoFallbackRequest {
            execution_id: "execution-1".to_owned(),
            task_id: "task-1".to_owned(),
            plan_id: "plan-1".to_owned(),
            step_id: "step-1".to_owned(),
            goal: "Open the fixture and complete the visual task".to_owned(),
            capability: "agent.execute".to_owned(),
            input: json!({}),
            user_confirmed: confirmed,
        }
    }

    fn blocking_cli_fixture() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("mano-cua");
        let pid_file = directory.path().join("active.pid");
        let script = format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
               run)\n\
                 [ \"$2\" = \"--help\" ] && exit 0\n\
                 echo $$ > \"{}\"\n\
                 trap 'exit 0' TERM INT\n\
                 while :; do sleep 0.1; done\n\
                 ;;\n\
               stop)\n\
                 [ \"$2\" = \"--help\" ] && exit 0\n\
                 [ -f \"{}\" ] && kill -TERM \"$(cat \"{}\")\"\n\
                 exit 0\n\
                 ;;\n\
               check) exit 0 ;;\n\
             esac\n\
             exit 1\n",
            pid_file.display(),
            pid_file.display(),
            pid_file.display(),
        );
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        (directory, executable)
    }

    #[test]
    fn mp1_cloud_is_rejected_before_cli_probe_without_explicit_authorization() {
        let adapter = adapter(
            "/definitely/missing/mano-cua",
            policy(ManoMode::Cloud, false, false),
        );

        assert_eq!(
            adapter.probe().unwrap_err().kind,
            ManoFallbackErrorKind::PermissionDenied
        );
    }

    #[test]
    fn mp1_unavailable_local_never_switches_to_authorized_cloud() {
        let adapter = adapter("/usr/bin/true", policy(ManoMode::Local, true, false));

        assert_eq!(
            adapter.probe().unwrap_err().kind,
            ManoFallbackErrorKind::Unsupported
        );
    }

    #[test]
    fn mp1_agent_input_cannot_enable_cloud() {
        let adapter = adapter("/usr/bin/true", policy(ManoMode::Local, true, false));
        let mut attempted_override = request(true);
        attempted_override.input = json!({
            "manoMode": "cloud",
            "manoCloudAuthorized": true
        });

        assert_eq!(
            adapter
                .execute(&attempted_override, &mut |_| {})
                .unwrap_err()
                .kind,
            ManoFallbackErrorKind::Unsupported
        );
    }

    #[test]
    fn mp1_task_confirmation_is_required_before_any_cli_probe() {
        let adapter = adapter(
            "/definitely/missing/mano-cua",
            policy(ManoMode::Cloud, true, false),
        );

        assert_eq!(
            adapter
                .execute(&request(false), &mut |_| {})
                .unwrap_err()
                .kind,
            ManoFallbackErrorKind::PermissionDenied
        );
    }

    #[test]
    fn mp1_sensitive_material_is_rejected_before_any_cli_probe() {
        let adapter = adapter(
            "/definitely/missing/mano-cua",
            policy(ManoMode::Cloud, true, false),
        );
        let mut sensitive = request(true);
        sensitive.input = json!({"credentials": {"password": "not-forwarded"}});

        assert_eq!(
            adapter.execute(&sensitive, &mut |_| {}).unwrap_err().kind,
            ManoFallbackErrorKind::PermissionDenied
        );
    }

    #[test]
    fn mp1_local_cancellation_signal_is_written_without_invoking_cli_stop() {
        let directory = tempfile::tempdir().unwrap();
        let stop_flag = directory.path().join("nested/stop.flag");
        let adapter = ManagedManoCliFallback {
            executable: PathBuf::from("/definitely/missing/mano-cua"),
            local_stop_flag: stop_flag.clone(),
            policy: Ok(policy(ManoMode::Local, false, true)),
            active: Mutex::new(None),
        };

        adapter.request_stop().unwrap();

        assert!(stop_flag.is_file());
    }

    #[test]
    fn mp1_explicitly_authorized_cloud_runs_as_one_bounded_black_box() {
        let adapter = adapter("/usr/bin/true", policy(ManoMode::Cloud, true, false));
        let mut phases = Vec::new();

        let result = adapter
            .execute(&request(true), &mut |progress| phases.push(progress.phase))
            .unwrap();

        assert_eq!(result.output["mode"], "cloud");
        assert_eq!(result.output["status"], "completed");
        assert_eq!(phases, vec!["starting", "executing", "completed"]);
        assert!(adapter.active.lock().unwrap().is_none());
    }

    #[test]
    fn mp1_task_text_and_input_do_not_leak_into_progress_or_result() {
        let adapter = adapter("/usr/bin/true", policy(ManoMode::Cloud, true, false));
        let marker = "AI_OS_MANO_PRIVATE_MARKER_7A4C";
        let mut marked = request(true);
        marked.goal = format!("Open TextEdit and type {marker}");
        marked.input = json!({"fixture": marker});
        let mut progress = Vec::new();

        let result = adapter
            .execute(&marked, &mut |update| progress.push(update))
            .unwrap();
        let exposed = format!("{progress:?} {:?}", result.output);

        assert!(!exposed.contains(marker));
    }

    #[test]
    fn mp1_concurrency_is_one_and_second_execution_is_rejected() {
        let (_directory, executable) = blocking_cli_fixture();
        let adapter = Arc::new(adapter(
            executable.to_str().unwrap(),
            policy(ManoMode::Cloud, true, false),
        ));
        let running_adapter = Arc::clone(&adapter);
        let handle = thread::spawn(move || running_adapter.execute(&request(true), &mut |_| {}));

        let deadline = Instant::now() + Duration::from_secs(3);
        while adapter.active.lock().unwrap().is_none() && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        assert!(adapter.active.lock().unwrap().is_some());

        let mut second = request(true);
        second.execution_id = "execution-2".to_owned();
        assert_eq!(
            adapter.execute(&second, &mut |_| {}).unwrap_err().kind,
            ManoFallbackErrorKind::AlreadyRunning
        );

        adapter.cancel("execution-1").unwrap();
        assert_eq!(
            handle.join().unwrap().unwrap_err().kind,
            ManoFallbackErrorKind::Cancelled
        );
        assert!(adapter.active.lock().unwrap().is_none());
    }

    #[test]
    fn mp1_runtime_cancel_uses_official_stop_and_reaches_cancelled_terminal_state() {
        let (_directory, executable) = blocking_cli_fixture();
        let adapter = Arc::new(adapter(
            executable.to_str().unwrap(),
            policy(ManoMode::Cloud, true, false),
        ));
        let execution_adapter = Arc::clone(&adapter);
        let handle = thread::spawn(move || execution_adapter.execute(&request(true), &mut |_| {}));

        let deadline = Instant::now() + Duration::from_secs(3);
        while adapter.active.lock().unwrap().is_none() && Instant::now() < deadline {
            thread::sleep(POLL_INTERVAL);
        }
        assert!(adapter.active.lock().unwrap().is_some());
        adapter.cancel("execution-1").unwrap();

        let error = handle.join().unwrap().unwrap_err();
        assert_eq!(error.kind, ManoFallbackErrorKind::Cancelled);
        assert!(adapter.active.lock().unwrap().is_none());
    }

    #[test]
    fn mp1_timeout_stops_the_process_and_reaches_timed_out_terminal_state() {
        let (_directory, executable) = blocking_cli_fixture();
        let mut bounded_policy = policy(ManoMode::Cloud, true, false);
        bounded_policy.timeout = Duration::from_millis(200);
        let adapter = adapter(executable.to_str().unwrap(), bounded_policy);

        let error = adapter.execute(&request(true), &mut |_| {}).unwrap_err();

        assert_eq!(error.kind, ManoFallbackErrorKind::TimedOut);
        assert!(adapter.active.lock().unwrap().is_none());
    }

    #[test]
    #[ignore = "requires the official mano-cua CLI on an M4/16GB acceptance machine"]
    fn mp5_real_cli_is_discovered_and_local_not_ready_is_normalized() {
        assert!(!local_hardware_supported());
        let directory = tempfile::tempdir().unwrap();
        let adapter = ManagedManoCliFallback {
            executable: PathBuf::from("mano-cua"),
            local_stop_flag: directory.path().join("stop.flag"),
            policy: Ok(policy(ManoMode::Local, false, false)),
            active: Mutex::new(None),
        };

        assert_eq!(
            adapter.probe().unwrap_err().kind,
            ManoFallbackErrorKind::Unsupported
        );
        assert_eq!(
            adapter.run_control_command(&["check"]).unwrap_err().kind,
            ManoFallbackErrorKind::Unavailable
        );
        assert!(adapter.active.lock().unwrap().is_none());
    }
}
