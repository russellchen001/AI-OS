//! Grok through the official xAI Grok CLI (`grok`), like Claude Code: the CLI
//! owns sign-in and its credential; AI-OS only reads its status and sends
//! bounded single-turn requests with every tool and web access turned off.

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

// Same bound as Claude Code: long syntheses need minutes; cancellable.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const GROK_CLI_DEFAULT_MODEL: &str = "grok-cli-default";
static GROK_CLI_REQUESTS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn active_requests() -> &'static Mutex<HashMap<String, CancellationToken>> {
    GROK_CLI_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn has_active_requests() -> bool {
    active_requests()
        .lock()
        .is_ok_and(|requests| !requests.is_empty())
}

pub(crate) fn installed_version() -> Option<String> {
    discover_binary()
        .and_then(|binary| run_bounded(&binary, &["--version"], None, STATUS_TIMEOUT, None).ok())
        .and_then(|output| String::from_utf8(output).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokCliStatus {
    installed: bool,
    authenticated: bool,
    binary_path: Option<String>,
    version: Option<String>,
    models: Vec<String>,
    default_model: Option<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokCliRequest {
    pub(crate) operation_id: Option<String>,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokCliResponse {
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
        candidates.extend([home.join(".grok/bin/grok"), home.join(".local/bin/grok")]);
    }
    if let Some(path) = inherited_path {
        candidates.extend(std::env::split_paths(&path).map(|entry| entry.join("grok")));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/grok"),
        PathBuf::from("/usr/local/bin/grok"),
    ]);
    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.clone()));
    candidates
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
    discover_binary_from(candidate_paths_for(
        std::env::var_os("AI_OS_GROK_CLI_BINARY"),
        dirs::home_dir(),
        std::env::var_os("PATH"),
    ))
}

/// An empty working directory, so the CLI never reads project files or
/// instructions from wherever AI-OS happens to run.
fn isolated_workdir() -> Result<PathBuf, String> {
    let directory = std::env::temp_dir().join("ai-os-grok-cli");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Grok CLI working folder could not be prepared".to_owned())?;
    Ok(directory)
}

fn sanitized_process_error(stderr: &[u8]) -> String {
    let value = String::from_utf8_lossy(stderr);
    let line = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Grok CLI could not complete the request");
    let safe: String = line
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect();
    let lower = safe.to_lowercase();
    if lower.contains("bearer ") || lower.contains("token") || lower.contains("xai-") {
        "Grok CLI returned a credential-related error".to_owned()
    } else {
        safe
    }
}

fn run_bounded(
    binary: &Path,
    arguments: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
    cancellation: Option<&CancellationToken>,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .current_dir(isolated_workdir()?)
        // The account sign-in is the credential; never a stray key.
        .env_remove("XAI_API_KEY")
        .env_remove("GROK_DEPLOYMENT_KEY")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "Grok CLI could not be started".to_owned())?;
    // Drain both pipes while the child is running; waiting first can deadlock
    // once a complete long answer fills the OS pipe buffer.
    let read_pipe = |mut pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.by_ref()
                .take(2_000_001)
                .read_to_end(&mut bytes)
                .map_err(|_| "Grok CLI output could not be read".to_owned())?;
            std::io::copy(&mut pipe, &mut std::io::sink())
                .map_err(|_| "Grok CLI output could not be read".to_owned())?;
            if bytes.len() > 2_000_000 {
                return Err("Grok CLI output exceeded the response limit".to_owned());
            }
            Ok(bytes)
        })
    };
    let stdout = read_pipe(Box::new(child.stdout.take().expect("piped stdout")));
    let stderr = read_pipe(Box::new(child.stderr.take().expect("piped stderr")));
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input.as_bytes())
            .map_err(|_| "Grok CLI could not receive the request".to_owned())?;
    }
    let started = Instant::now();
    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Grok CLI request was cancelled".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = stdout
                    .join()
                    .map_err(|_| "Grok CLI output reader was interrupted".to_owned())??;
                let error = stderr
                    .join()
                    .map_err(|_| "Grok CLI error reader was interrupted".to_owned())??;
                return if status.success() {
                    Ok(output)
                } else {
                    Err(sanitized_process_error(&error))
                };
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Grok CLI request timed out".to_owned());
            }
            Err(_) => return Err("Grok CLI process state is unavailable".to_owned()),
        }
    }
}

/// Reads `grok models`: sign-in state, the default and the available models.
fn parse_models_output(output: &str) -> (bool, Option<String>, Vec<String>) {
    let authenticated = output.contains("You are logged in");
    let default_model = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Default model:"))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let models = output
        .lines()
        .skip_while(|line| !line.trim().starts_with("Available models"))
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            let name = line
                .strip_prefix("* ")
                .or_else(|| line.strip_prefix("- "))?;
            let name = name.split_whitespace().next()?;
            name.chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
                .then(|| name.to_owned())
        })
        .collect();
    (authenticated, default_model, models)
}

fn status_for_binary(binary: &Path) -> Result<GrokCliStatus, String> {
    let version = run_bounded(binary, &["--version"], None, STATUS_TIMEOUT, None)
        .ok()
        .and_then(|output| String::from_utf8(output).ok())
        .map(|value| value.trim().to_owned());
    let output = run_bounded(binary, &["models"], None, STATUS_TIMEOUT, None)?;
    let (authenticated, default_model, models) =
        parse_models_output(&String::from_utf8_lossy(&output));
    Ok(GrokCliStatus {
        installed: true,
        authenticated,
        binary_path: Some(binary.display().to_string()),
        version,
        models: if authenticated { models } else { Vec::new() },
        default_model,
        message: if authenticated {
            "Grok account is connected through the official Grok CLI.".to_owned()
        } else {
            "Run `grok login` in Terminal to sign in with your xAI account.".to_owned()
        },
    })
}

#[tauri::command]
pub(crate) fn get_grok_cli_status() -> GrokCliStatus {
    let Some(binary) = discover_binary() else {
        return GrokCliStatus {
            installed: false,
            authenticated: false,
            binary_path: None,
            version: None,
            models: Vec::new(),
            default_model: None,
            message: "Install the official Grok CLI (x.ai/cli) to connect a Grok account."
                .to_owned(),
        };
    };
    status_for_binary(&binary).unwrap_or_else(|message| GrokCliStatus {
        installed: true,
        authenticated: false,
        binary_path: Some(binary.display().to_string()),
        version: None,
        models: Vec::new(),
        default_model: None,
        message,
    })
}

fn validate_model_id(value: &str) -> Result<(), String> {
    // A leading "-" would be read by the CLI as another option.
    if !value.starts_with(|character: char| character.is_ascii_alphanumeric())
        || value.len() > 200
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err("Grok CLI model ID is invalid".to_owned());
    }
    Ok(())
}

fn request_arguments(model_id: &str) -> Vec<&str> {
    let mut arguments = vec![
        "-p",
        "-",
        "--output-format",
        "json",
        "--tools",
        "",
        "--disable-web-search",
        "--no-subagents",
        "--max-turns",
        "1",
    ];
    if model_id != GROK_CLI_DEFAULT_MODEL {
        arguments.extend(["--model", model_id]);
    }
    arguments
}

#[tauri::command]
pub(crate) async fn generate_grok_cli_response(
    input: GrokCliRequest,
) -> Result<GrokCliResponse, String> {
    let model_id = input.model_id.trim().to_owned();
    let prompt = input.prompt.trim().to_owned();
    let operation_id = input
        .operation_id
        .as_deref()
        .map(validate_operation_id)
        .transpose()?;
    validate_model_id(&model_id)?;
    if prompt.is_empty() || prompt.len() > 100_000 {
        return Err("message is empty or too large".to_owned());
    }
    let binary = discover_binary().ok_or_else(|| "Grok CLI is not installed".to_owned())?;
    let cancellation = CancellationToken::new();
    if let Some(operation_id) = operation_id.as_deref() {
        let mut requests = active_requests()
            .lock()
            .map_err(|_| "Grok CLI request state is unavailable".to_owned())?;
        if requests
            .insert(operation_id.to_owned(), cancellation.clone())
            .is_some()
        {
            return Err("Grok CLI operation is already active".to_owned());
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let output = run_bounded(
            &binary,
            &request_arguments(&model_id),
            Some(&prompt),
            COMMAND_TIMEOUT,
            Some(&cancellation),
        )?;
        let value: Value = serde_json::from_slice(&output)
            .map_err(|_| "Grok CLI returned an unreadable response".to_owned())?;
        let text = value
            .get("text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Grok CLI returned no text".to_owned())?;
        Ok(GrokCliResponse {
            model_id,
            text: text.to_owned(),
        })
    })
    .await
    .unwrap_or_else(|_| Err("Grok CLI request was interrupted".to_owned()));
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
        return Err("Grok CLI operation ID is invalid".to_owned());
    }
    Ok(value.to_owned())
}

#[tauri::command]
pub(crate) fn cancel_grok_cli_request(operation_id: String) -> Result<bool, String> {
    let operation_id = validate_operation_id(&operation_id)?;
    let token = active_requests()
        .lock()
        .map_err(|_| "Grok CLI request state is unavailable".to_owned())?
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
            None,
        )
        .expect("long CLI output must not deadlock on a full pipe");
        assert_eq!(output.len(), 131072);
    }

    #[test]
    fn cli_candidates_cover_official_install_location_without_duplicates() {
        let home = PathBuf::from("/tmp/grok-home");
        let path = std::env::join_paths(["/tmp/grok-path", "/tmp/grok-home/.local/bin"]).unwrap();
        let candidates = candidate_paths_for(None, Some(home.clone()), Some(path));
        assert_eq!(candidates[0], home.join(".grok/bin/grok"));
        assert!(candidates.contains(&PathBuf::from("/tmp/grok-path/grok")));
        let unique: HashSet<_> = candidates.iter().collect();
        assert_eq!(unique.len(), candidates.len());
    }

    #[test]
    fn non_executable_cli_is_not_reported_as_installed() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("grok");
        std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
        assert_eq!(discover_binary_from(vec![binary]), None);
    }

    #[test]
    fn models_output_reports_sign_in_default_and_models() {
        let signed_in = "You are logged in with grok.com.\n\nDefault model: grok-4.7\n\nAvailable models:\n  * grok-4.7 (default)\n  - grok-4.7-build-fast\n  - grok-4.6\n";
        assert_eq!(
            parse_models_output(signed_in),
            (
                true,
                Some("grok-4.7".to_owned()),
                vec![
                    "grok-4.7".to_owned(),
                    "grok-4.7-build-fast".to_owned(),
                    "grok-4.6".to_owned()
                ]
            )
        );
        let signed_out =
            "You are not authenticated.\n\nDefault model: grok-4.6\n\nAvailable models:\n";
        assert!(!parse_models_output(signed_out).0);
    }

    #[test]
    fn requests_disable_tools_web_and_subagents_and_read_stdin() {
        let arguments = request_arguments(GROK_CLI_DEFAULT_MODEL);
        assert_eq!(&arguments[..2], &["-p", "-"]);
        for flag in ["--disable-web-search", "--no-subagents"] {
            assert!(arguments.contains(&flag));
        }
        let tools = arguments
            .iter()
            .position(|item| *item == "--tools")
            .unwrap();
        assert_eq!(arguments[tools + 1], "");
        assert!(!arguments.contains(&"--model"));
        assert!(request_arguments("grok-4.6").ends_with(&["--model", "grok-4.6"]));
    }

    #[test]
    fn credential_shaped_errors_are_redacted() {
        assert_eq!(
            sanitized_process_error(b"Authorization: Bearer private-value"),
            "Grok CLI returned a credential-related error"
        );
        assert_eq!(
            sanitized_process_error(b"rate limited\nmore"),
            "rate limited"
        );
    }

    #[test]
    fn model_and_operation_ids_are_bounded_and_path_free() {
        assert!(validate_model_id("grok-4.7").is_ok());
        assert!(validate_model_id("--tools").is_err());
        assert!(validate_model_id("a b").is_err());
        assert!(validate_operation_id("../../secret").is_err());
    }

    #[test]
    fn cancellation_is_scoped_and_consumes_the_operation() {
        let operation_id = "grok-cli-cancel-test";
        let token = CancellationToken::new();
        active_requests()
            .lock()
            .unwrap()
            .insert(operation_id.to_owned(), token.clone());
        assert!(cancel_grok_cli_request(operation_id.to_owned()).unwrap());
        assert!(token.is_cancelled());
        assert!(!cancel_grok_cli_request(operation_id.to_owned()).unwrap());
    }
}
