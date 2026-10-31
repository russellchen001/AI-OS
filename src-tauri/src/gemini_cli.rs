//! Gemini through the official Google Gemini CLI (`gemini`), like Claude Code
//! and Grok CLI: the user signs in once with their own Google account in the
//! CLI, so requests use that account's own quota and never an AI-OS Cloud
//! project. AI-OS only reads which sign-in method the CLI has, and sends
//! bounded single-turn headless requests with every tool denied.

use crate::cli_process::{self, CliProcess, OperationRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(20);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const GEMINI_CLI_DEFAULT_MODEL: &str = "gemini-cli-default";
static GEMINI_CLI_REQUESTS: OnceLock<OperationRegistry> = OnceLock::new();

const PROCESS: CliProcess = CliProcess {
    label: "Gemini CLI",
    workdir_name: "ai-os-gemini-cli",
    env_set: &[],
    env_remove: &[],
    secret_markers: &["aiza", "ya29.", "client_secret", "refresh_token"],
};

pub(crate) const HOME_LOCATIONS: &[&str] = &[
    ".local/bin/gemini",
    ".npm-global/bin/gemini",
    // An AI-OS managed install (cli_provider_manager).
    ".ai-os/providers/google/node_modules/.bin/gemini",
];

/// Every tool, built-in or MCP, denied at the Admin tier (above the user's
/// own policies). Denied global rules remove the tool from the model's view.
const DENY_ALL_TOOLS_POLICY: &str =
    "# Written by AI-OS for its headless Gemini CLI requests.\n[[rule]]\ntoolName = \"*\"\ndecision = \"deny\"\npriority = 999\n";
/// Hooks run user commands and skills add instructions; neither belongs in a
/// bounded AI-OS request. Passed as the system settings layer, so the user's
/// own ~/.gemini/settings.json is never modified.
const SYSTEM_SETTINGS: &str =
    "{\"hooksConfig\":{\"enabled\":false},\"skills\":{\"enabled\":false}}\n";

fn active_requests() -> &'static OperationRegistry {
    GEMINI_CLI_REQUESTS.get_or_init(OperationRegistry::new)
}

pub(crate) fn has_active_requests() -> bool {
    active_requests().is_busy()
}

pub(crate) fn discover_binary() -> Option<PathBuf> {
    cli_process::discover_executable(cli_process::candidate_paths(
        "gemini",
        std::env::var_os("AI_OS_GEMINI_CLI_BINARY"),
        dirs::home_dir(),
        HOME_LOCATIONS,
        std::env::var_os("PATH"),
    ))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiCliStatus {
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
pub(crate) struct GeminiCliRequest {
    pub(crate) operation_id: Option<String>,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiCliResponse {
    pub(crate) model_id: String,
    pub(crate) text: String,
}

/// The sign-in method the CLI saved (`security.auth.selectedType`, or the
/// older top-level `selectedAuthType`). Only this setting is read; the
/// credential itself stays with the CLI (file or Keychain).
fn selected_auth_type(settings: &Value) -> Option<String> {
    settings
        .pointer("/security/auth/selectedType")
        .or_else(|| settings.get("selectedAuthType"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn user_settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("GEMINI_CLI_HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)?;
    Some(home.join(".gemini/settings.json"))
}

fn configured_auth_type() -> Option<String> {
    let text = std::fs::read_to_string(user_settings_path()?).ok()?;
    // settings.json allows comments; strip whole-line ones before parsing.
    let json: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    selected_auth_type(&serde_json::from_str(&json).ok()?)
}

fn auth_description(auth_type: &str) -> &'static str {
    match auth_type {
        "oauth-personal" => "your Google account",
        "gemini-api-key" => "your own Gemini API key",
        "vertex-ai" => "your own Vertex AI project",
        _ => "your Gemini CLI sign-in",
    }
}

fn status_for(binary: Option<&Path>, auth_type: Option<String>) -> GeminiCliStatus {
    let Some(binary) = binary else {
        return GeminiCliStatus {
            installed: false,
            authenticated: false,
            binary_path: None,
            version: None,
            models: Vec::new(),
            default_model: None,
            message: "Install the official Gemini CLI (github.com/google-gemini/gemini-cli) to use your own Google account.".to_owned(),
        };
    };
    let version = PROCESS
        .run(binary, &["--version"], None, STATUS_TIMEOUT, None)
        .ok()
        .and_then(|output| cli_process::parse_version(&String::from_utf8_lossy(&output)));
    let authenticated = auth_type.is_some();
    GeminiCliStatus {
        installed: true,
        authenticated,
        binary_path: Some(binary.display().to_string()),
        version,
        models: if authenticated {
            vec![GEMINI_CLI_DEFAULT_MODEL.to_owned()]
        } else {
            Vec::new()
        },
        default_model: authenticated.then(|| GEMINI_CLI_DEFAULT_MODEL.to_owned()),
        message: match auth_type {
            Some(auth_type) => format!(
                "Gemini CLI uses {}; requests count against that account's own quota.",
                auth_description(&auth_type)
            ),
            None => "Run `gemini` in Terminal once and choose “Sign in with Google” to use your own account.".to_owned(),
        },
    }
}

#[tauri::command]
pub(crate) fn get_gemini_cli_status() -> GeminiCliStatus {
    status_for(discover_binary().as_deref(), configured_auth_type())
}

/// Gemini CLI ignores `--admin-policy` once any `.toml` sits in the standard
/// system policy folder, so AI-OS could no longer guarantee tools are off.
const SYSTEM_POLICY_DIRECTORY: &str = "/Library/Application Support/GeminiCli/policies";

fn has_system_admin_policies(directory: &Path) -> bool {
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|extension| extension == "toml"))
    })
}

/// AI-OS's own policy and settings files, rewritten before each request so a
/// changed file can never loosen them.
fn request_files() -> Result<(PathBuf, PathBuf), String> {
    if has_system_admin_policies(Path::new(SYSTEM_POLICY_DIRECTORY)) {
        return Err(
            "An administrator Gemini CLI policy is installed on this Mac, so the CLI would ignore AI-OS's tools-off policy. AI-OS will not send requests through it."
                .to_owned(),
        );
    }
    let directory = PROCESS.workdir()?.join(".ai-os");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Gemini CLI policy folder could not be prepared".to_owned())?;
    let policy = directory.join("deny-all-tools.toml");
    let settings = directory.join("system-settings.json");
    std::fs::write(&policy, DENY_ALL_TOOLS_POLICY)
        .and_then(|_| std::fs::write(&settings, SYSTEM_SETTINGS))
        .map_err(|_| "Gemini CLI policy could not be written".to_owned())?;
    Ok((policy, settings))
}

fn request_arguments<'a>(model_id: &'a str, policy: &'a str) -> Vec<&'a str> {
    // The prompt arrives on stdin; `-p ""` only selects headless mode.
    let mut arguments = vec![
        "-p",
        "",
        "--output-format",
        "json",
        "--admin-policy",
        policy,
        // No MCP server name matches, so none is started.
        "--allowed-mcp-server-names",
        "ai-os-none",
        "-e",
        "none",
        // The empty AI-OS folder; headless runs exit on an untrusted folder.
        "--skip-trust",
    ];
    if model_id != GEMINI_CLI_DEFAULT_MODEL {
        arguments.extend(["--model", model_id]);
    }
    arguments
}

/// `response` on success; the CLI reports failures (quota, sign-in) as
/// `error.message`, sometimes with exit code 0.
fn parse_response(output: &[u8]) -> Result<String, String> {
    let value: Value = serde_json::from_slice(output)
        .map_err(|_| "Gemini CLI returned an unreadable response".to_owned())?;
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(PROCESS.sanitized_error(message.as_bytes()));
    }
    value
        .get("response")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Gemini CLI returned no text".to_owned())
}

fn run_request(
    model_id: &str,
    prompt: &str,
    timeout: Duration,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<String, String> {
    let binary = discover_binary().ok_or_else(|| "Gemini CLI is not installed".to_owned())?;
    let (policy, settings) = request_files()?;
    let policy = policy.to_string_lossy().into_owned();
    let settings = settings.to_string_lossy().into_owned();
    let process = CliProcess {
        env_set: &[("GEMINI_CLI_SYSTEM_SETTINGS_PATH", settings.as_str())],
        ..PROCESS
    };
    let output = process
        .run(
            &binary,
            &request_arguments(model_id, &policy),
            Some(prompt),
            timeout,
            cancellation,
        )
        .map_err(|error| explain_tier_error(&error))?;
    parse_response(&output).map_err(|error| explain_tier_error(&error))
}

/// Google stopped serving personal accounts (free, AI Pro, Ultra) through
/// Gemini CLI on 2026-06-18; say what still works instead of only quoting it.
fn explain_tier_error(error: &str) -> String {
    if error.contains("IneligibleTierError") || error.contains("no longer supported for Gemini Code Assist") {
        "Google no longer serves personal Google accounts through Gemini CLI. Connect Google with your own Gemini API key from Google AI Studio instead.".to_owned()
    } else {
        error.to_owned()
    }
}

/// One tiny real request, so "verified" means the account can answer.
#[tauri::command]
pub(crate) async fn verify_gemini_cli_account() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(|| {
        run_request(
            GEMINI_CLI_DEFAULT_MODEL,
            "Reply with the single word OK.",
            VERIFY_TIMEOUT,
            None,
        )
        .map(|_| true)
    })
    .await
    .unwrap_or_else(|_| Err("Gemini CLI verification was interrupted".to_owned()))
}

#[tauri::command]
pub(crate) async fn generate_gemini_cli_response(
    input: GeminiCliRequest,
) -> Result<GeminiCliResponse, String> {
    let model_id = input.model_id.trim().to_owned();
    let prompt = input.prompt.trim().to_owned();
    let operation_id = input
        .operation_id
        .as_deref()
        .map(|value| cli_process::validate_operation_id(PROCESS.label, value))
        .transpose()?;
    cli_process::validate_model_id(PROCESS.label, &model_id)?;
    if prompt.is_empty() || prompt.len() > 100_000 {
        return Err("message is empty or too large".to_owned());
    }
    let cancellation = match operation_id.as_deref() {
        Some(operation_id) => active_requests().register(PROCESS.label, operation_id)?,
        None => tokio_util::sync::CancellationToken::new(),
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_request(&model_id, &prompt, COMMAND_TIMEOUT, Some(&cancellation))
            .map(|text| GeminiCliResponse { model_id, text })
    })
    .await
    .unwrap_or_else(|_| Err("Gemini CLI request was interrupted".to_owned()));
    if let Some(operation_id) = operation_id {
        active_requests().finish(&operation_id);
    }
    result
}

#[tauri::command]
pub(crate) fn cancel_gemini_cli_request(operation_id: String) -> Result<bool, String> {
    let operation_id = cli_process::validate_operation_id(PROCESS.label, &operation_id)?;
    active_requests().cancel(PROCESS.label, &operation_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_method_is_read_from_current_and_legacy_settings() {
        let current = serde_json::json!({"security": {"auth": {"selectedType": "oauth-personal"}}});
        assert_eq!(
            selected_auth_type(&current).as_deref(),
            Some("oauth-personal")
        );
        let legacy = serde_json::json!({"selectedAuthType": "gemini-api-key"});
        assert_eq!(
            selected_auth_type(&legacy).as_deref(),
            Some("gemini-api-key")
        );
        assert_eq!(
            selected_auth_type(&serde_json::json!({"theme": "Dark"})),
            None
        );
    }

    #[test]
    fn signed_out_cli_offers_no_models_and_explains_google_sign_in() {
        let status = status_for(Some(Path::new("/nonexistent/gemini")), None);
        assert!(status.installed && !status.authenticated);
        assert!(status.models.is_empty());
        assert!(status.message.contains("Sign in with Google"));
        let signed_in = status_for(
            Some(Path::new("/nonexistent/gemini")),
            Some("oauth-personal".into()),
        );
        assert_eq!(signed_in.models, vec![GEMINI_CLI_DEFAULT_MODEL.to_owned()]);
        assert!(signed_in.message.contains("own quota"));
        assert!(!status_for(None, None).installed);
    }

    #[test]
    fn requests_deny_every_tool_and_skip_mcp_and_extensions() {
        let arguments = request_arguments(GEMINI_CLI_DEFAULT_MODEL, "/tmp/policy.toml");
        assert_eq!(&arguments[..2], &["-p", ""]);
        let policy = arguments
            .iter()
            .position(|item| *item == "--admin-policy")
            .unwrap();
        assert_eq!(arguments[policy + 1], "/tmp/policy.toml");
        let extensions = arguments.iter().position(|item| *item == "-e").unwrap();
        assert_eq!(arguments[extensions + 1], "none");
        assert!(arguments.contains(&"--allowed-mcp-server-names"));
        for unsafe_flag in ["-y", "--yolo", "--approval-mode"] {
            assert!(!arguments.contains(&unsafe_flag));
        }
        assert!(!arguments.contains(&"--model"));
        assert!(request_arguments("gemini-2.5-pro", "p").ends_with(&["--model", "gemini-2.5-pro"]));
        assert!(
            DENY_ALL_TOOLS_POLICY.contains("toolName = \"*\"")
                && DENY_ALL_TOOLS_POLICY.contains("decision = \"deny\"")
        );
    }

    #[test]
    fn system_admin_policies_block_requests_because_ours_would_be_ignored() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!has_system_admin_policies(directory.path()));
        std::fs::write(directory.path().join("README.txt"), "notes").unwrap();
        assert!(!has_system_admin_policies(directory.path()));
        std::fs::write(directory.path().join("org.toml"), "[[rule]]").unwrap();
        assert!(has_system_admin_policies(directory.path()));
        assert!(!has_system_admin_policies(Path::new("/nonexistent/gemini/policies")));
    }

    #[test]
    fn retired_personal_tier_points_to_the_users_own_api_key() {
        let explained = explain_tier_error("Error authenticating: IneligibleTierError: This client is no longer supported for Gemini Code Assist for individuals.");
        assert!(explained.contains("own Gemini API key"));
        assert_eq!(explain_tier_error("Quota exceeded"), "Quota exceeded");
    }

    #[test]
    fn json_errors_are_reported_even_with_a_successful_exit() {
        assert_eq!(
            parse_response(br#"{"response":"OK","stats":{}}"#).unwrap(),
            "OK"
        );
        let quota = parse_response(
            br#"{"error":{"type":"Error","message":"Quota exceeded for your account","code":429}}"#,
        );
        assert_eq!(quota.unwrap_err(), "Quota exceeded for your account");
        assert!(parse_response(br#"{"response":"  "}"#).is_err());
        assert_eq!(
            parse_response(br#"{"error":{"message":"invalid refresh_token value"}}"#).unwrap_err(),
            "Gemini CLI returned a credential-related error"
        );
    }
}

#[cfg(test)]
mod live_tests {
    /// Runs the real CLI through the AI-OS request path (signed-out Macs
    /// get the CLI's own sign-in error; signed-in ones a one-word answer).
    #[test]
    #[ignore = "runs the installed Gemini CLI"]
    fn live_request_reports_cli_outcome() {
        println!("status: {:?}", super::get_gemini_cli_status());
        println!(
            "request: {:?}",
            super::run_request(super::GEMINI_CLI_DEFAULT_MODEL, "Reply with the single word OK.", std::time::Duration::from_secs(90), None)
        );
    }
}
