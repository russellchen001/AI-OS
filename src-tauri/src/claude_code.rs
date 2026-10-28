use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

// A five-seat Arena synthesis (about 18 KB of drafts) measured 129 s, so
// 90 s cut off complete answers. Requests stay cancellable throughout.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
static CLAUDE_CODE_REQUESTS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn active_requests() -> &'static Mutex<HashMap<String, CancellationToken>> {
    CLAUDE_CODE_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeCodeStatus {
    installed: bool,
    authenticated: bool,
    binary_path: Option<String>,
    version: Option<String>,
    auth_method: Option<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeCodeRequest {
    pub(crate) operation_id: Option<String>,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeCodeResponse {
    pub(crate) model_id: String,
    pub(crate) text: String,
}

fn candidate_paths_for(
    explicit: Option<OsString>,
    home: Option<PathBuf>,
    inherited_path: Option<OsString>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit {
        candidates.push(PathBuf::from(path));
    }
    if let Some(home) = home {
        candidates.extend([
            home.join(".local/bin/claude"),
            home.join(".claude/local/claude"),
            home.join(".volta/bin/claude"),
            home.join(".bun/bin/claude"),
            home.join(".npm-global/bin/claude"),
            home.join("Library/pnpm/claude"),
        ]);
    }
    if let Some(path) = inherited_path {
        candidates.extend(std::env::split_paths(&path).map(|entry| entry.join("claude")));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ]);
    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.clone()));
    candidates
}

fn candidate_paths() -> Vec<PathBuf> {
    candidate_paths_for(
        std::env::var_os("AI_OS_CLAUDE_CODE_BINARY"),
        dirs::home_dir(),
        std::env::var_os("PATH"),
    )
}

fn discover_binary_from(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| {
        path.is_file()
            && path.metadata().is_ok_and(|metadata| {
                #[cfg(unix)]
                {
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
    })
}

fn discover_binary() -> Option<PathBuf> {
    discover_binary_from(candidate_paths())
}

fn sanitized_process_error(stderr: &[u8]) -> String {
    let value = String::from_utf8_lossy(stderr);
    let line = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Claude Code could not complete the request");
    let safe: String = line
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect();
    if safe.contains("sk-ant-") || safe.contains("Bearer ") || safe.contains("oauth") {
        "Claude Code returned a credential-related error".to_owned()
    } else {
        safe
    }
}

fn run_bounded(
    binary: &Path,
    arguments: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
    accept_stdout_on_failure: bool,
    cancellation: Option<&CancellationToken>,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .env_remove("ANTHROPIC_API_KEY")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "Claude Code could not be started".to_owned())?;
    let read_pipe = |mut pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.by_ref()
                .take(2_000_001)
                .read_to_end(&mut bytes)
                .map_err(|_| "Claude Code output could not be read".to_owned())?;
            std::io::copy(&mut pipe, &mut std::io::sink())
                .map_err(|_| "Claude Code output could not be read".to_owned())?;
            if bytes.len() > 2_000_000 {
                return Err("Claude Code output exceeded the response limit".to_owned());
            }
            Ok(bytes)
        })
    };
    let stdout = read_pipe(Box::new(child.stdout.take().expect("piped stdout")));
    let stderr = read_pipe(Box::new(child.stderr.take().expect("piped stderr")));
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input.as_bytes())
            .map_err(|_| "Claude Code could not receive the request".to_owned())?;
    }
    let started = Instant::now();
    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Claude Code request was cancelled".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = stdout
                    .join()
                    .map_err(|_| "Claude Code output reader was interrupted".to_owned())??;
                let error = stderr
                    .join()
                    .map_err(|_| "Claude Code error reader was interrupted".to_owned())??;
                return if status.success() || (accept_stdout_on_failure && !output.is_empty()) {
                    Ok(output)
                } else {
                    Err(sanitized_process_error(&error))
                };
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Claude Code request timed out".to_owned());
            }
            Err(_) => return Err("Claude Code process state is unavailable".to_owned()),
        }
    }
}

fn status_for_binary(binary: &Path) -> Result<ClaudeCodeStatus, String> {
    let version = run_bounded(binary, &["--version"], None, STATUS_TIMEOUT, false, None)
        .ok()
        .and_then(|output| String::from_utf8(output).ok())
        .map(|value| value.trim().to_owned());
    let output = run_bounded(
        binary,
        &["auth", "status", "--json"],
        None,
        STATUS_TIMEOUT,
        true,
        None,
    )?;
    let status: Value = serde_json::from_slice(&output)
        .map_err(|_| "Claude Code returned an unreadable authentication status".to_owned())?;
    let authenticated = status
        .get("loggedIn")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let auth_method = status
        .get("authMethod")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value != "none")
        .map(str::to_owned);
    Ok(ClaudeCodeStatus {
        installed: true,
        authenticated,
        binary_path: Some(binary.display().to_string()),
        version,
        auth_method,
        message: if authenticated {
            "Claude Code subscription is connected.".to_owned()
        } else {
            "Sign in with the official Claude Code CLI to use your subscription.".to_owned()
        },
    })
}

#[tauri::command]
pub(crate) fn get_claude_code_status() -> ClaudeCodeStatus {
    let Some(binary) = discover_binary() else {
        return ClaudeCodeStatus {
            installed: false,
            authenticated: false,
            binary_path: None,
            version: None,
            auth_method: None,
            message: "Install the official Claude Code CLI to connect a Claude subscription."
                .to_owned(),
        };
    };
    status_for_binary(&binary).unwrap_or_else(|message| ClaudeCodeStatus {
        installed: true,
        authenticated: false,
        binary_path: Some(binary.display().to_string()),
        version: None,
        auth_method: None,
        message,
    })
}

#[tauri::command]
pub(crate) async fn generate_claude_code_response(
    input: ClaudeCodeRequest,
) -> Result<ClaudeCodeResponse, String> {
    let model_id = input.model_id.trim().to_owned();
    let prompt = input.prompt.trim().to_owned();
    let operation_id = input
        .operation_id
        .as_deref()
        .map(validate_operation_id)
        .transpose()?;
    if model_id.is_empty() || model_id.len() > 200 {
        return Err("Claude Code model ID is invalid".to_owned());
    }
    if prompt.is_empty() || prompt.len() > 100_000 {
        return Err("message is empty or too large".to_owned());
    }
    let binary = discover_binary().ok_or_else(|| "Claude Code is not installed".to_owned())?;
    let status = status_for_binary(&binary)?;
    if !status.authenticated {
        return Err("Claude Code needs account sign-in".to_owned());
    }
    let cancellation = CancellationToken::new();
    if let Some(operation_id) = operation_id.as_deref() {
        let mut requests = active_requests()
            .lock()
            .map_err(|_| "Claude Code request state is unavailable".to_owned())?;
        if requests
            .insert(operation_id.to_owned(), cancellation.clone())
            .is_some()
        {
            return Err("Claude Code operation is already active".to_owned());
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut arguments = vec![
            "--print",
            "--output-format",
            "json",
            "--permission-mode",
            "dontAsk",
            "--tools",
            "",
        ];
        if model_id != "claude-code-default" {
            arguments.extend(["--model", model_id.as_str()]);
        }
        let output = run_bounded(
            &binary,
            &arguments,
            Some(&prompt),
            COMMAND_TIMEOUT,
            false,
            Some(&cancellation),
        )?;
        let value: Value = serde_json::from_slice(&output)
            .map_err(|_| "Claude Code returned an unreadable response".to_owned())?;
        let text = value
            .get("result")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Claude Code returned no text".to_owned())?;
        Ok(ClaudeCodeResponse {
            model_id,
            text: text.to_owned(),
        })
    })
    .await
    .unwrap_or_else(|_| Err("Claude Code request was interrupted".to_owned()));
    if let Some(operation_id) = operation_id {
        if let Ok(mut requests) = active_requests().lock() {
            requests.remove(&operation_id);
        }
    }
    result
}

fn validate_operation_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err("Claude Code operation ID is invalid".to_owned());
    }
    Ok(value.to_owned())
}

#[tauri::command]
pub(crate) fn cancel_claude_code_request(operation_id: String) -> Result<bool, String> {
    let operation_id = validate_operation_id(&operation_id)?;
    let token = active_requests()
        .lock()
        .map_err(|_| "Claude Code request state is unavailable".to_owned())?
        .remove(&operation_id);
    if let Some(token) = token {
        token.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn long_stdout_and_stderr_are_drained_before_process_exit() {
        let output = run_bounded(
            Path::new("/bin/sh"),
            &[
                "-c",
                "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2",
            ],
            None,
            Duration::from_secs(5),
            false,
            None,
        )
        .expect("long CLI output must not deadlock on a full pipe");
        assert_eq!(output.len(), 131072);
    }

    #[test]
    fn cli_candidates_cover_custom_home_and_launch_path_without_duplicates() {
        let home = PathBuf::from("/tmp/claude-home");
        let path =
            std::env::join_paths(["/tmp/claude-path", "/tmp/claude-home/.local/bin"]).unwrap();
        let candidates = candidate_paths_for(None, Some(home.clone()), Some(path));
        assert!(candidates.contains(&home.join(".claude/local/claude")));
        assert!(candidates.contains(&PathBuf::from("/tmp/claude-path/claude")));
        let unique: HashSet<_> = candidates.iter().collect();
        assert_eq!(unique.len(), candidates.len());
    }

    #[test]
    fn non_executable_cli_is_not_reported_as_installed() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("claude");
        std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
        assert_eq!(discover_binary_from(Vec::new()), None);
        assert_eq!(discover_binary_from(vec![binary]), None);
    }

    #[test]
    fn credential_shaped_errors_are_redacted() {
        assert_eq!(
            sanitized_process_error(b"Authorization: Bearer private-value"),
            "Claude Code returned a credential-related error"
        );
    }

    #[test]
    fn ordinary_errors_are_bounded_and_preserved() {
        assert_eq!(
            sanitized_process_error(b"not logged in\nmore"),
            "not logged in"
        );
    }

    #[test]
    fn operation_ids_are_bounded_and_path_free() {
        assert_eq!(
            validate_operation_id("claude-code-123").unwrap(),
            "claude-code-123"
        );
        assert!(validate_operation_id("../../secret").is_err());
        assert!(validate_operation_id("has space").is_err());
    }

    #[test]
    fn cancellation_is_scoped_and_consumes_the_operation() {
        let operation_id = "claude-code-cancel-test";
        let token = CancellationToken::new();
        active_requests()
            .lock()
            .unwrap()
            .insert(operation_id.to_owned(), token.clone());

        assert!(cancel_claude_code_request(operation_id.to_owned()).unwrap());
        assert!(token.is_cancelled());
        assert!(!cancel_claude_code_request(operation_id.to_owned()).unwrap());
    }
}
