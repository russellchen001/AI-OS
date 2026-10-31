//! Grok through the official xAI Grok CLI (`grok`), like Claude Code: the CLI
//! owns sign-in and its credential; AI-OS only reads its status and sends
//! bounded single-turn requests with every tool and web access turned off.

use crate::cli_process::{self, CliProcess, OperationRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

// Same bound as Claude Code: long syntheses need minutes; cancellable.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const GROK_CLI_DEFAULT_MODEL: &str = "grok-cli-default";
static GROK_CLI_REQUESTS: OnceLock<OperationRegistry> = OnceLock::new();

const PROCESS: CliProcess = CliProcess {
    label: "Grok CLI",
    workdir_name: "ai-os-grok-cli",
    // Don't load the user's Claude/Cursor MCP servers (one exposed a delete
    // tool) or workflows into AI-OS requests.
    env_set: &[
        ("GROK_CLAUDE_MCPS_ENABLED", "0"),
        ("GROK_CURSOR_MCPS_ENABLED", "0"),
        ("GROK_WORKFLOWS", "0"),
    ],
    // The account sign-in is the credential; never a stray key.
    env_remove: &["XAI_API_KEY", "GROK_DEPLOYMENT_KEY"],
    secret_markers: &["xai-"],
};

fn active_requests() -> &'static OperationRegistry {
    GROK_CLI_REQUESTS.get_or_init(OperationRegistry::new)
}

pub(crate) fn has_active_requests() -> bool {
    active_requests().is_busy()
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

pub(crate) const HOME_LOCATIONS: &[&str] = &[
    ".grok/bin/grok",
    ".local/bin/grok",
    // An AI-OS managed install (cli_provider_manager).
    ".ai-os/providers/grok/node_modules/.bin/grok",
];

fn candidate_paths_for(
    explicit: Option<OsString>,
    home: Option<PathBuf>,
    inherited_path: Option<OsString>,
) -> Vec<PathBuf> {
    cli_process::candidate_paths("grok", explicit, home, HOME_LOCATIONS, inherited_path)
}

pub(crate) fn discover_binary() -> Option<PathBuf> {
    cli_process::discover_executable(candidate_paths_for(
        std::env::var_os("AI_OS_GROK_CLI_BINARY"),
        dirs::home_dir(),
        std::env::var_os("PATH"),
    ))
}

fn run_bounded(
    binary: &Path,
    arguments: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<Vec<u8>, String> {
    PROCESS.run(binary, arguments, stdin, timeout, cancellation)
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
    cli_process::validate_model_id(PROCESS.label, value)
}

/// Every built-in tool the official CLI can offer, removed by its documented
/// `--disallowed-tools`. An empty `--tools ""` "names no tool, so other tools
/// stay available" (Grok docs): a Council cross-review then tried a shell
/// command, the headless run could not ask for approval and stopped with only
/// its opening sentence ("Grok CLI returned no text").
const GROK_DISALLOWED_TOOLS: &str = "run_terminal_cmd,task,Agent,read_file,grep,list_dir,write,search_replace,Delete,web_search,web_fetch,image_gen,image_edit,image_to_video,reference_to_video,CallMcpTool,FetchMcpResource,ListMcpResources,todo_write,monitor,search_tool,use_tool,workflow,enter_plan_mode,exit_plan_mode,ask_user_question,send_feedback,update_goal,CreateGoal";

fn request_arguments(model_id: &str) -> Vec<&str> {
    let mut arguments = vec![
        "-p",
        "-",
        "--output-format",
        "json",
        "--disallowed-tools",
        GROK_DISALLOWED_TOOLS,
        // Any remaining tool call is denied instead of waiting for approval.
        "--permission-mode",
        "dontAsk",
        "--disable-web-search",
        "--no-subagents",
        "--max-turns",
        "1",
        // A 22 KB cross-review took 381 s at the default effort (18k reasoning
        // tokens) and 167 s at medium with a complete answer.
        "--reasoning-effort",
        "medium",
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
    let cancellation = match operation_id.as_deref() {
        Some(operation_id) => active_requests().register(PROCESS.label, operation_id)?,
        None => tokio_util::sync::CancellationToken::new(),
    };
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
        active_requests().finish(&operation_id);
    }
    result
}

fn validate_operation_id(value: &str) -> Result<String, String> {
    cli_process::validate_operation_id(PROCESS.label, value)
}

#[tauri::command]
pub(crate) fn cancel_grok_cli_request(operation_id: String) -> Result<bool, String> {
    let operation_id = validate_operation_id(&operation_id)?;
    active_requests().cancel(PROCESS.label, &operation_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
        // An empty --tools leaves every tool available; tools are removed instead.
        assert!(!arguments.contains(&"--tools"));
        let removed = arguments
            .iter()
            .position(|item| *item == "--disallowed-tools")
            .unwrap();
        for tool in ["run_terminal_cmd", "write", "search_replace", "Delete", "use_tool", "CallMcpTool", "task"] {
            assert!(arguments[removed + 1].split(',').any(|item| item == tool), "{tool} stays available");
        }
        let mode = arguments.iter().position(|item| *item == "--permission-mode").unwrap();
        assert_eq!(arguments[mode + 1], "dontAsk");
        assert!(!arguments.contains(&"--model"));
        assert!(request_arguments("grok-4.6").ends_with(&["--model", "grok-4.6"]));
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
        let token = active_requests().register("Grok CLI", operation_id).unwrap();
        assert!(cancel_grok_cli_request(operation_id.to_owned()).unwrap());
        assert!(token.is_cancelled());
        assert!(!cancel_grok_cli_request(operation_id.to_owned()).unwrap());
    }
}
