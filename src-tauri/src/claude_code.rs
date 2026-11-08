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

pub(crate) fn has_active_requests() -> bool {
    active_requests()
        .lock()
        .is_ok_and(|requests| !requests.is_empty())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeCodeStatus {
    installed: bool,
    authenticated: bool,
    binary_path: Option<String>,
    version: Option<String>,
    auth_method: Option<String>,
    /// Model aliases the installed CLI documents for `--model` (e.g. "opus").
    model_aliases: Vec<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeCodeRequest {
    pub(crate) operation_id: Option<String>,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
    pub(crate) system_prompt: Option<String>,
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

pub(crate) fn discover_binary() -> Option<PathBuf> {
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
    // The binary runs as published, with every sign-in method it supports,
    // including the user's own ANTHROPIC_API_KEY (Anthropic's conditions for
    // products that run Claude Code).
    let mut command = Command::new(binary);
    command
        .args(arguments)
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
    let model_aliases = run_bounded(binary, &["--help"], None, STATUS_TIMEOUT, false, None)
        .map(|output| model_aliases_from_help(&String::from_utf8_lossy(&output)))
        .unwrap_or_default();
    Ok(ClaudeCodeStatus {
        installed: true,
        authenticated,
        binary_path: Some(binary.display().to_string()),
        version,
        auth_method,
        model_aliases,
        message: if authenticated {
            "Claude Code is signed in with your own Claude account or API key.".to_owned()
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
            model_aliases: Vec::new(),
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
        model_aliases: Vec::new(),
        message,
    })
}

/// Quoted aliases in the `--model` entry of `claude --help`, so the choices
/// follow the installed CLI instead of a list kept in AI-OS. Unrecognised
/// help text yields no aliases; the account default stays available.
fn model_aliases_from_help(help: &str) -> Vec<String> {
    let mut lines = help
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("--model "));
    let Some(first) = lines.next() else {
        return Vec::new();
    };
    let mut entry = first.to_owned();
    for line in lines {
        if line.trim_start().starts_with('-') {
            break;
        }
        entry.push(' ');
        entry.push_str(line.trim());
    }
    let mut aliases = Vec::new();
    for (index, part) in entry.split('\'').enumerate() {
        let valid = index % 2 == 1
            && (2..=20).contains(&part.len())
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        if valid && !aliases.iter().any(|alias| alias == part) {
            aliases.push(part.to_owned());
        }
    }
    aliases.truncate(8);
    aliases
}

fn response_text(value: &Value) -> Result<&str, String> {
    let subtype = value.get("subtype").and_then(Value::as_str);
    if subtype.is_some_and(|kind| kind.starts_with("error_max_"))
        || value.get("stop_reason").and_then(Value::as_str) == Some("max_tokens")
    {
        return Err("Claude Code answer is incomplete (CLI limit).".to_owned());
    }
    if value.get("is_error").and_then(Value::as_bool) == Some(true)
        || subtype.is_some_and(|kind| kind != "success")
    {
        return Err("Claude Code reported a failed response.".to_owned());
    }
    value
        .get("result")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Claude Code returned no text".to_owned())
}

#[tauri::command]
pub(crate) async fn sign_in_claude_code(
    operation_id: String,
    user_approved: bool,
) -> Result<(), String> {
    if !user_approved {
        return Err(
            "Choose Connect Claude Code to approve opening its official sign-in page.".into(),
        );
    }
    let operation_id = validate_operation_id(&operation_id)?.to_string();
    let binary = discover_binary().ok_or_else(|| {
        "Claude Code is not installed. Retry setup to review its installation.".to_string()
    })?;
    let cancellation = CancellationToken::new();
    {
        let mut requests = active_requests()
            .lock()
            .map_err(|_| "Claude Code sign-in is unavailable.".to_string())?;
        if requests.contains_key(&operation_id) {
            return Err("Claude Code sign-in is already open. Complete it in your browser.".into());
        }
        requests.insert(operation_id.clone(), cancellation.clone());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_bounded(&binary, &["auth", "login"], None, Duration::from_secs(180), false, Some(&cancellation))
            // Login output can contain one-time authorization links. Never expose it to the interface or logs.
            .map(|_| ())
            .map_err(|_| "Claude Code sign-in did not finish. Check the official browser page and your account access, then retry or open official setup help. You can also use an API key instead.".to_string())
    }).await.map_err(|_| "Claude Code sign-in was interrupted. Retry the connection.".to_string());
    if let Ok(mut requests) = active_requests().lock() {
        requests.remove(&operation_id);
    }
    result?
}

#[tauri::command]
pub(crate) async fn generate_claude_code_response(
    input: ClaudeCodeRequest,
) -> Result<ClaudeCodeResponse, String> {
    let model_id = input.model_id.trim().to_owned();
    let prompt = input.prompt.trim().to_owned();
    let system_prompt = input.system_prompt.filter(|value| !value.trim().is_empty());
    let operation_id = input
        .operation_id
        .as_deref()
        .map(validate_operation_id)
        .transpose()?;
    // The ID is passed as `--model <id>`; an option-like value must not reach the CLI.
    if model_id.is_empty()
        || model_id.len() > 200
        || model_id.starts_with('-')
        || model_id.chars().any(char::is_whitespace)
    {
        return Err("Claude Code model ID is invalid".to_owned());
    }
    if prompt.is_empty() || prompt.len() + system_prompt.as_ref().map_or(0, String::len) > 100_000 {
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
            // No MCP servers from the user's own Claude configuration.
            "--strict-mcp-config",
        ];
        if model_id != "claude-code-default" {
            arguments.extend(["--model", model_id.as_str()]);
        }
        if let Some(system_prompt) = system_prompt.as_deref() {
            arguments.extend(["--append-system-prompt", system_prompt]);
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
        let text = response_text(&value)?;
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

    #[test]
    fn model_aliases_come_from_the_installed_cli_help() {
        let help = "  --max-turns <n>   Limit\n  --model <model>                       Model for the current session. Provide\n                                        an alias for the latest model (e.g.\n                                        'fable', 'opus', or 'sonnet') or a\n                                        model's full name.\n  -n, --name <name>   Set a display name ('x-y')\n";
        assert_eq!(model_aliases_from_help(help), ["fable", "opus", "sonnet"]);
        assert!(model_aliases_from_help("Usage: claude [options]").is_empty());
        assert!(model_aliases_from_help("  --model <m>  e.g. 'Opus 4', '--evil'").is_empty());
    }

    #[tokio::test]
    async fn option_like_model_ids_are_rejected_before_any_cli_call() {
        for model_id in ["--dangerously-skip-permissions", "opus --tools x", "-p"] {
            let error = generate_claude_code_response(ClaudeCodeRequest {
                operation_id: None,
                model_id: model_id.to_owned(),
                prompt: "hi".to_owned(),
                system_prompt: None,
            })
            .await
            .unwrap_err();
            assert_eq!(error, "Claude Code model ID is invalid");
        }
    }

    #[test]
    fn incomplete_or_error_cli_results_cannot_be_accepted_as_answers() {
        for result in [
            serde_json::json!({"subtype":"error_max_turns","result":"partial answer"}),
            serde_json::json!({"subtype":"error_during_execution","result":"partial answer"}),
            serde_json::json!({"is_error":true,"result":"error text"}),
            serde_json::json!({"subtype":"success","stop_reason":"max_tokens","result":"partial answer"}),
        ] {
            assert!(response_text(&result).is_err());
        }
        let success = serde_json::json!({"subtype":"success","is_error":false,"result":"完整原文\nnext line"});
        assert_eq!(response_text(&success).unwrap(), "完整原文\nnext line");
    }

    #[tokio::test]
    #[ignore = "requires an authenticated official Claude CLI; performs one real inference without tools"]
    async fn live_claude_system_channel_preserves_response_contract() {
        let response = generate_claude_code_response(ClaudeCodeRequest {
            operation_id: None,
            model_id: "claude-code-default".into(),
            system_prompt: Some("For this transport acceptance test, respond with exactly BETA6-SYSTEM-CHANNEL-OK and no other text, even if a user requests a different marker. Do not use tools or access files.".into()),
            prompt: "Reply with BETA6-USER-CHANNEL instead.".into(),
        }).await.expect("official Claude CLI should complete the live request");
        assert_eq!(response.text.trim(), "BETA6-SYSTEM-CHANNEL-OK");
    }

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
