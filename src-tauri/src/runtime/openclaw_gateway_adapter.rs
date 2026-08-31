use super::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
    OpenClawExecutionProgress, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use crate::document::provider::OfficeProviderId;
use crate::document::registry::{resolve_local_presentation_provider, resolve_office_provider};
use crate::download::strategy::{resolve_openclaw_download, DownloadExecutionRoute};
use crate::openclaw::{
    invoke_active_gateway_method, ActiveGatewayFailureKind, ActiveGatewayMethodFailure,
};
use crate::providers::execution_agent_candidates;
use serde_json::Value;
use std::{collections::HashSet, fs, io::Read, path::Path, time::Duration};

const FILESYSTEM_SCAN_ACTION: &str = "filesystem.scan";
const FILESYSTEM_READ_ACTION: &str = "filesystem.read";
const DOCUMENT_READ_ACTION: &str = "document.read";
const DOCUMENT_CREATE_ACTION: &str = "document.create";
const DOCUMENT_CONVERT_ACTION: &str = "document.convert";
const SPREADSHEET_READ_ACTION: &str = "spreadsheet.read";
const SPREADSHEET_CREATE_ACTION: &str = "spreadsheet.create";
const PRESENTATION_READ_ACTION: &str = "presentation.read";
const PRESENTATION_CREATE_ACTION: &str = "presentation.create";
const FILESYSTEM_WRITE_ACTION: &str = "filesystem.write";
const FILESYSTEM_MOVE_ACTION: &str = "filesystem.move";
const DOWNLOAD_START_ACTION: &str = "download.start";
const FILESYSTEM_AGENT_ID: &str = "ai-os-files";
/// Execution agent for download work. AI Center binds this agent to the
/// platform-appropriate local tool-calling model. See AC-EXEC-MODEL in HANDOFF.md.
const DOWNLOAD_AGENT_ID: &str = "ai-os-exec-standard";
const MAX_DOWNLOAD_AGENT_ATTEMPTS: usize = 2;
const AGENT_WAIT_ATTEMPTS: usize = 35;
const LONG_DOWNLOAD_WAIT_ATTEMPTS: usize = 1_605;
const MAX_FILE_READ_BYTES: u64 = 1_000_000;
const MAX_FILE_OUTPUT_BYTES: u64 = 65_536;
const MAX_FILE_WRITE_BYTES: usize = 4_096;

pub(crate) struct OpenClawGatewayExecutionAdapter;

impl OpenClawExecutionAdapter for OpenClawGatewayExecutionAdapter {
    fn execute(
        &self,
        request: &OpenClawExecutionRequest,
        report: &mut dyn FnMut(OpenClawExecutionProgress),
    ) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
        execute_with_invoker(&ProductionGatewayMethodInvoker, request, report)
    }
}

trait GatewayMethodInvoker: Send + Sync {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure>;
}

struct ProductionGatewayMethodInvoker;

impl GatewayMethodInvoker for ProductionGatewayMethodInvoker {
    fn invoke(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ActiveGatewayMethodFailure> {
        invoke_active_gateway_method(method, params).map(|result| result.payload)
    }
}

fn execute_with_invoker(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
    _report: &mut dyn FnMut(OpenClawExecutionProgress),
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    if request.action.as_str() == FILESYSTEM_SCAN_ACTION {
        return execute_filesystem_scan(invoker, request);
    }
    if request.action.as_str() == FILESYSTEM_READ_ACTION {
        return execute_filesystem_read(invoker, request);
    }
    if request.action.as_str() == DOCUMENT_READ_ACTION {
        return execute_document_read(invoker, request);
    }
    if request.action.as_str() == DOCUMENT_CREATE_ACTION {
        return execute_document_create(invoker, request);
    }
    if request.action.as_str() == DOCUMENT_CONVERT_ACTION {
        return execute_document_convert(invoker, request);
    }
    if request.action.as_str() == SPREADSHEET_READ_ACTION {
        return execute_spreadsheet_read(invoker, request);
    }
    if request.action.as_str() == SPREADSHEET_CREATE_ACTION {
        return execute_spreadsheet_create(invoker, request);
    }

    if request.action.as_str() == PRESENTATION_READ_ACTION {
        return execute_presentation_read(invoker, request);
    }
    if request.action.as_str() == PRESENTATION_CREATE_ACTION {
        return execute_presentation_create(invoker, request);
    }
    if request.action.as_str() == FILESYSTEM_WRITE_ACTION {
        return execute_filesystem_write(invoker, request);
    }
    if request.action.as_str() == FILESYSTEM_MOVE_ACTION {
        return execute_filesystem_move(invoker, request);
    }
    if request.action.as_str() == DOWNLOAD_START_ACTION {
        return execute_download_start(invoker, request);
    }

    invoker
        .invoke(request.action.as_str(), Some(request.input.clone()))
        .map(|payload| OpenClawExecutionResult {
            output: payload,
            summary: None,
        })
        .map_err(map_gateway_failure)
}

fn download_execution_agents(_route: &DownloadExecutionRoute) -> Vec<String> {
    match execution_agent_candidates(DOWNLOAD_START_ACTION) {
        Ok(candidates) if !candidates.is_empty() => candidates,
        _ => vec![DOWNLOAD_AGENT_ID.to_owned()],
    }
}

fn should_retry_download_with_next_agent(error: &OpenClawExecutionError) -> bool {
    error.kind == OpenClawExecutionErrorKind::ProtocolFailure && error.retryable
}

fn execute_download_start(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let source = required_download_input(request, "source")?;
    let destination = required_download_input(request, "destination")?;
    let requested_extraction_code = optional_download_input(request, "extractionCode");
    let selection_hint = optional_download_input(request, "selectionHint").unwrap_or(source);
    let destination_path = Path::new(destination);
    if !destination_path.is_absolute() || !destination_path.is_dir() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "Download destination must be an existing absolute directory.",
            false,
        ));
    }

    let (route, resolved_source) = resolve_openclaw_download(source);
    let tool_source = resolved_source.as_str();
    let before = download_directory_files(destination_path)?;
    let long_running_download = matches!(&route, DownloadExecutionRoute::Web);
    let agent_ids = download_execution_agents(&route);
    let (tool, message) = match &route {
        DownloadExecutionRoute::Direct => (
            "direct-http",
            format!(
                "Call the existing exec tool exactly once with command {}. Set workdir to {}, background to false, and yieldMs to 10000. Use the command exactly as supplied and do not claim success unless it succeeds.",
                shell_quote_command(&[
                    "/usr/bin/curl", "--fail", "--location", "--remote-header-name",
                    "--remote-name", "--output-dir", destination, tool_source,
                ]),
                serde_json::to_string(destination).unwrap(),
            ),
        ),
        DownloadExecutionRoute::Web => (
            "web",
            format!(
                concat!(
                    "Complete this user-confirmed Web or cloud-share download automatically. ",
                    "Source: {}. User's complete item request: {}. Optional extraction code supplied by the user: {}. ",
                    "Step 1: before using curl or browser automation, check the Skills already listed in your context. ",
                    "Do not search the filesystem for them. Do not run find, ls, or which to locate a Skill. ",
                    "A Skill named <name> in your context has its document at exactly $HOME/.agents/skills/<name>/SKILL.md; ",
                    "read that path directly with your file read capability. ",
                    "If reading that exact path fails, treat the Skill as unavailable and move to Step 2. ",
                    "Never guess a command's flags. Use only commands and flags written in the SKILL.md you read. ",
                    "Preparing values such as a session id is not a step and never ends your turn. ",
                    "Compute any required value inline inside the same command that does the work, ",
                    "or run the real command immediately after computing it in the same turn. ",
                    "Never stop after echoing, exporting, or announcing a value. ",
                    "A Skill is an instruction document, not a tool. Never call a Skill name as if it were a tool. ",
                    "If an installed download or cloud-drive Skill declares support for this source, read its SKILL.md, ",
                    "then your very next tool call MUST execute its documented direct share-link download command with the exact source and destination. ",
                    "When that Skill provides a direct share-link download command, use it and do not use curl or browser tools. ",
                    "Do not read a browser Skill after finding a matching download Skill. ",
                    "If the download Skill supports an isolated transfer or target-folder option, use the unique folder name {} so older transferred files cannot be included. ",
                    "If the share contains multiple items, inspect them with the Skill's documented read-only listing command and download only the item matching the user's complete item request. ",
                    "Never download the entire share when the user identified one item by filename, type, or approximate size. ",
                    "If the request does not identify one item unambiguously, return the available names and sizes instead of downloading unrelated items. ",
                    "Pass exec a normal shell command string; never quote the command name and subcommand together. ",
                    "Reading SKILL.md is not completion. Do not stop to report that you read it and do not ask the user anything. ",
                    "Do not repeat the same failed command or browser wait more than once. If the documented Skill command fails, ",
                    "return its real error instead of trying unrelated curl flags or browser commands. ",
                    "Step 2: only when no installed Skill supports this source, call exec with /usr/bin/curl to fetch the page. ",
                    "If the HTML contains an ordinary download anchor, resolve its href against the source URL ",
                    "and immediately call exec again with ",
                    "/usr/bin/curl --fail --location --remote-name --output-dir {} followed by the resolved file URL. ",
                    "This is the normal path and handles most downloads. ",
                    "Step 3: when passing the source to any command, use the URL exactly as supplied above. ",
                    "Never rewrite it as Markdown link syntax, never wrap it in brackets or parentheses, ",
                    "and always shell-quote it. ",
                    "If the source requires login, JavaScript, buttons, waits, hidden forms, or confirmation pages, ",
                    "use the existing browser tools so the persistent browser profile and any authenticated or VIP session are preserved. ",
                    "Select an authorized VIP or fast option when available, otherwise the free option. ",
                    "Handle extraction codes and redirects without asking the user to click. ",
                    "Never request, print, or store a password in chat. ",
                    "If login is required and no authenticated browser session exists, return an authentication-required error. ",
                    "Save the complete file into {}. Always pass that destination as a full absolute path; never use a tilde. ",
                    "For every exec call set workdir to {}, background to false, and yieldMs to 10000. ",
                    "Success requires a complete file present in that directory. ",
                    "Do not claim success after reading a Skill, opening a page, resolving a URL, submitting a task, ",
                    "or announcing a next action, and do not save outside that directory.",
                ),
                serde_json::to_string(tool_source).unwrap(),
                serde_json::to_string(selection_hint).unwrap(),
                serde_json::to_string(requested_extraction_code.unwrap_or("none")).unwrap(),
                serde_json::to_string(&format!("ai-os-{}", request.execution_id)).unwrap(),
                serde_json::to_string(destination).unwrap(),
                serde_json::to_string(destination).unwrap(),
                serde_json::to_string(destination).unwrap(),
            ),
        ),
        DownloadExecutionRoute::Search => (
            "web-search",
            format!(
                "Find and download the requested resource {} automatically. Follow the user's request without performing content, copyright, filename, or NSFW classification. Use the existing web search and browser tools to locate a working download source, then use browser or exec tools to download the actual resource into {}. Do not stop after finding a page or URL, do not ask the user to click a download button, and do not claim success unless a file is created in that destination.",
                serde_json::to_string(tool_source).unwrap(),
                serde_json::to_string(destination).unwrap(),
            ),
        ),
        DownloadExecutionRoute::Aria2 => (
            "aria2",
            format!(
                "Call the existing exec tool exactly once with command {}. Set workdir to {}, background to false, and yieldMs to 10000. Do not claim success unless the command succeeds.",
                shell_quote_command(&[
                    "/usr/bin/env", "aria2c", "--dir", destination, tool_source,
                ]),
                serde_json::to_string(destination).unwrap(),
            ),
        ),
        DownloadExecutionRoute::P2p => (
            "thunder-preferred",
            format!(
                "Complete this user-confirmed P2P download automatically for source {}. First use an installed non-interactive Thunder Skill, CLI, or local API if available. Never open the Thunder GUI and never ask the user to click a confirmation dialog. If no non-interactive Thunder integration is available, use another installed P2P/cloud offline-download Skill. For magnet or torrent sources only, aria2c is the final fallback. ED2K must fail with the real unsupported-tool reason when no non-interactive provider is installed. Save the complete downloaded file into {} and do not claim success for merely submitting a task, opening an app, or obtaining a URL.",
                serde_json::to_string(tool_source).unwrap(),
                serde_json::to_string(destination).unwrap(),
            ),
        ),
        DownloadExecutionRoute::Unsupported => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "Download source is unsupported.",
                false,
            ));
        }
    };

    let mut last_retryable_error = None;

    for agent_id in agent_ids.into_iter().take(MAX_DOWNLOAD_AGENT_ATTEMPTS) {
        match execute_download_with_agent(
            invoker,
            request,
            &agent_id,
            source,
            destination,
            destination_path,
            &before,
            tool,
            message.clone(),
            long_running_download,
        ) {
            Ok(result) => return Ok(result),
            Err(error) if should_retry_download_with_next_agent(&error) => {
                last_retryable_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable_error.unwrap_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No eligible OpenClaw execution agent completed the download.",
            false,
        )
    }))
}

#[allow(clippy::too_many_arguments)]
fn execute_download_with_agent(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
    agent_id: &str,
    source: &str,
    destination: &str,
    destination_path: &Path,
    before: &HashSet<String>,
    tool: &str,
    message: String,
    long_running_download: bool,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let session_key = format!("agent:{agent_id}:ai-os-download-{}", request.execution_id);
    let message = if long_running_download {
        bind_visible_download_skill(invoker, agent_id, message)?
    } else {
        message
    };
    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": agent_id,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": if long_running_download { 14_400 } else { 180 },
                "idempotencyKey": format!("{}:{agent_id}", request.execution_id),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    let wait_attempts = if long_running_download {
        LONG_DOWNLOAD_WAIT_ATTEMPTS
    } else {
        AGENT_WAIT_ATTEMPTS
    };
    for _ in 0..wait_attempts {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("timeout") => continue,
            Some("error") => {
                let terminal_error = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw download execution failed.");
                let files = if terminal_error == "completed" {
                    wait_for_download_files(destination_path, before, 40)?
                } else {
                    download_directory_files(destination_path)?
                        .difference(before)
                        .cloned()
                        .collect::<Vec<_>>()
                };
                if !files.is_empty() {
                    return Ok(OpenClawExecutionResult {
                        output: serde_json::json!({
                            "kind": "download",
                            "source": source,
                            "destination": destination,
                            "tool": tool,
                            "status": "completed",
                            "files": files,
                        }),
                        summary: Some(format!(
                            "OpenClaw used the {tool} download route and AI-OS verified the file."
                        )),
                    });
                }
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    terminal_error,
                    false,
                ));
            }
            Some("ok") => {
                let files = download_directory_files(destination_path)?
                    .difference(before)
                    .cloned()
                    .collect::<Vec<_>>();
                if files.is_empty() {
                    return Err(OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw completed without creating a file in the selected destination.",
                        true,
                    ));
                }
                return Ok(OpenClawExecutionResult {
                    output: serde_json::json!({
                        "kind": "download",
                        "source": source,
                        "destination": destination,
                        "tool": tool,
                        "status": "completed",
                        "files": files,
                    }),
                    summary: Some(format!("OpenClaw used the {tool} download route.")),
                });
            }
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid download status.",
                    false,
                ))
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw download execution timed out.",
        true,
    ))
}

fn bind_visible_download_skill(
    invoker: &dyn GatewayMethodInvoker,
    agent_id: &str,
    message: String,
) -> Result<String, OpenClawExecutionError> {
    let inventory = invoker
        .invoke(
            "commands.list",
            Some(serde_json::json!({"agentId": agent_id, "scope": "text", "includeArgs": false})),
        )
        .map_err(map_gateway_failure)?;
    let command = inventory["commands"]
        .as_array()
        .and_then(|commands| commands.iter().find(|command| command["source"] == "skill"))
        .and_then(|command| command["textAliases"].as_array()?.first()?.as_str());
    Ok(match command {
        Some(command) => format!("{command} {message}"),
        None => message,
    })
}

fn optional_download_input<'a>(
    request: &'a OpenClawExecutionRequest,
    key: &str,
) -> Option<&'a str> {
    request
        .input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_download_input<'a>(
    request: &'a OpenClawExecutionRequest,
    key: &str,
) -> Result<&'a str, OpenClawExecutionError> {
    request
        .input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                format!("download.start requires {key}"),
                false,
            )
        })
}

fn download_directory_files(path: &Path) -> Result<HashSet<String>, OpenClawExecutionError> {
    fn is_complete_download(path: &Path) -> bool {
        if path.metadata().map(|metadata| metadata.len()).unwrap_or(0) == 0 {
            return false;
        }
        let Ok(mut file) = fs::File::open(path) else {
            return false;
        };
        let mut prefix = [0_u8; 512];
        let count = file.read(&mut prefix).unwrap_or(0);
        let text = String::from_utf8_lossy(&prefix[..count]).to_ascii_lowercase();
        let trimmed = text.trim_start();
        !trimmed.starts_with("<!doctype html") && !trimmed.starts_with("<html")
    }

    fn collect_files(
        root: &Path,
        current: &Path,
        files: &mut HashSet<String>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                collect_files(root, &entry_path, files)?;
            } else if entry_path.is_file() && is_complete_download(&entry_path) {
                if let Ok(relative) = entry_path.strip_prefix(root) {
                    files.insert(relative.to_string_lossy().into_owned());
                }
            }
        }
        Ok(())
    }

    let mut files = HashSet::new();
    collect_files(path, path, &mut files).map_err(|_| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::PermissionDenied,
            "Download destination could not be read.",
            false,
        )
    })?;
    Ok(files)
}

fn wait_for_download_files(
    path: &Path,
    before: &HashSet<String>,
    attempts: usize,
) -> Result<Vec<String>, OpenClawExecutionError> {
    for _ in 0..attempts {
        let files = download_directory_files(path)?
            .difference(before)
            .cloned()
            .collect::<Vec<_>>();
        if !files.is_empty() {
            return Ok(files);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(Vec::new())
}

fn shell_quote_command(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn execute_filesystem_scan(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.scan requires path",
                false,
            )
        })?;
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-core-skill-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label filesystem.scan is NOT an OpenClaw tool name: do not call any tool named filesystem.scan. Call the existing exec tool exactly once. Use command exactly as written: /usr/bin/find . -mindepth 1 -maxdepth 1 -print. Set workdir to {}, background to false, and yieldMs to 10000. Do not add pipes, xargs, printf, shell wrappers, or other flags. Do not create, modify, move, or delete anything. Return compact JSON with ok, path, and entries containing name and type, and do not claim success without the tool output.",
        serde_json::to_string(path).unwrap_or_else(|_| "\"\"".to_owned())
    );
    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|run_id| !run_id.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let output = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = filesystem_scan_output(&output, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw folder scan completed without a successful exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the folder scan.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw agent execution timed out.",
        true,
    ))
}

fn latest_successful_exec_result(history: &Value) -> Option<&Value> {
    let messages = history.get("messages")?.as_array()?;

    messages.iter().rev().find_map(|entry| {
        let message = entry.get("message").unwrap_or(entry);

        (message.get("role").and_then(Value::as_str) == Some("toolResult")
            && message.get("toolName").and_then(Value::as_str) == Some("exec")
            && message.get("isError").and_then(Value::as_bool) != Some(true))
        .then_some(message)
    })
}

fn filesystem_scan_output(history: &Value, path: &str) -> Option<Value> {
    let tool_result = latest_successful_exec_result(history)?;
    let text = tool_result
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.iter().any(|line| !line.starts_with("./")) {
        return None;
    }
    let entries = lines
        .into_iter()
        .filter_map(|line| line.strip_prefix("./"))
        .map(str::to_owned)
        .collect::<Vec<_>>();

    Some(serde_json::json!({"path": path, "entries": entries}))
}

fn execute_filesystem_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.read requires path",
                false,
            )
        })?;
    let workdir = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.read requires an absolute file path",
                false,
            )
        })?;
    let quoted_path = shell_quote(path);
    let command = format!(
        "file_path={quoted_path}; mime=$(/usr/bin/file -b --mime-type -- \"$file_path\") || exit 1; size=$(/usr/bin/stat -f %z -- \"$file_path\") || exit 1; /usr/bin/printf 'AIOS_MIME=%s\\nAIOS_SIZE=%s\\nAIOS_CONTENT_BEGIN\\n' \"$mime\" \"$size\"; if [ \"$size\" -gt {MAX_FILE_READ_BYTES} ]; then /usr/bin/printf 'AIOS_TOO_LARGE\\n'; else case \"$mime\" in text/*|application/json|application/xml|application/x-empty|inode/x-empty) /usr/bin/head -c {MAX_FILE_OUTPUT_BYTES} -- \"$file_path\" ;; *) /usr/bin/printf 'AIOS_UNSUPPORTED\\n' ;; esac; fi"
    );
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-core-skill-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label filesystem.read is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. This is read-only. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );
    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|run_id| !run_id.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = filesystem_read_output(&history, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw file read completed without a valid exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the file read.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw agent execution timed out.",
        true,
    ))
}

fn execute_document_create(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let (path, session_key, run_id) = start_document_create(invoker, request)?;
    finish_document_create(invoker, &path, &session_key, &run_id)
}

fn finish_document_create(
    invoker: &dyn GatewayMethodInvoker,
    path: &str,
    session_key: &str,
    run_id: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;

        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = document_create_output(&history, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw document create completed without a valid exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the document creation.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw document creation timed out.",
        true,
    ))
}

fn start_document_create(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String), OpenClawExecutionError> {
    let (path, content, workdir) = document_create_input(request)?;
    let provider = resolve_office_provider(DOCUMENT_CREATE_ACTION).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No available Office Provider supports document.create.",
            false,
        )
    })?;
    if provider.id != OfficeProviderId::MacosNative {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not yet have a document.create adapter.",
                provider.name
            ),
            false,
        ));
    }

    let command = document_create_command(path, content, workdir);
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-document-create-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label document.create is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. Create only; never overwrite an existing file. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );

    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    Ok((path.to_owned(), session_key, run_id.to_owned()))
}

fn document_create_command(path: &str, content: &str, workdir: &str) -> String {
    let format = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx");
    format!(
        "target={}; content={}; workdir={}; if [ -e \"$target\" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; else tmpdir=$(/usr/bin/mktemp -d \"$workdir/.ai-os-document.XXXXXX\") || exit 1; trap '/bin/rm -rf \"$tmpdir\"' EXIT; /usr/bin/printf '%s' \"$content\" > \"$tmpdir/source.txt\" || exit 1; if /usr/bin/textutil -convert {} -output \"$tmpdir/output.{}\" \"$tmpdir/source.txt\" >/dev/null 2>&1 && /bin/ln \"$tmpdir/output.{}\" \"$target\"; then size=$(/usr/bin/stat -f %z -- \"$target\") || exit 1; /usr/bin/printf 'AIOS_DOCUMENT_CREATED=%s\n' \"$size\"; elif [ -e \"$target\" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; else /usr/bin/printf 'AIOS_FAILED\n'; fi; fi",
        shell_quote(path),
        shell_quote(content),
        shell_quote(workdir),
        format,
        format,
        format,
    )
}

fn document_create_output(history: &Value, path: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some((_, size)) = text.rsplit_once("AIOS_DOCUMENT_CREATED=") {
        return Some(serde_json::json!({
            "path": path,
            "status": "created",
            "bytesWritten": size.trim().parse::<u64>().ok()?,
        }));
    }
    if text.lines().any(|line| line.trim() == "AIOS_EXISTS") {
        return Some(serde_json::json!({"path": path, "status": "exists"}));
    }
    if text.lines().any(|line| line.trim() == "AIOS_FAILED") {
        return Some(serde_json::json!({"path": path, "status": "failed"}));
    }
    None
}

fn execute_document_convert(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let (source, destination, session_key, run_id) = start_document_convert(invoker, request)?;
    finish_document_convert(invoker, &source, &destination, &session_key, &run_id)
}

fn finish_document_convert(
    invoker: &dyn GatewayMethodInvoker,
    source: &str,
    destination: &str,
    session_key: &str,
    run_id: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;

        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output =
                    document_convert_output(&history, source, destination).ok_or_else(|| {
                        OpenClawExecutionError::new(
                            OpenClawExecutionErrorKind::ProtocolFailure,
                            "OpenClaw document conversion completed without a valid exec tool result.",
                            false,
                        )
                    })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the document conversion.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw document conversion timed out.",
        true,
    ))
}

fn start_document_convert(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String, String), OpenClawExecutionError> {
    let (source, destination, workdir, format) = document_convert_input(request)?;
    let provider = resolve_office_provider(DOCUMENT_CONVERT_ACTION).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No available Office Provider supports document.convert.",
            false,
        )
    })?;
    if provider.id != OfficeProviderId::MacosNative {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not yet have a document.convert adapter.",
                provider.name
            ),
            false,
        ));
    }

    let command = document_convert_command(source, destination, workdir, &format);
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-document-convert-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label document.convert is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. Create only; never overwrite an existing file. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );

    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    Ok((
        source.to_owned(),
        destination.to_owned(),
        session_key,
        run_id.to_owned(),
    ))
}

fn document_convert_command(
    source: &str,
    destination: &str,
    workdir: &str,
    format: &str,
) -> String {
    format!(
        "source={}; target={}; workdir={}; if [ ! -f \"$source\" ]; then /usr/bin/printf 'AIOS_SOURCE_MISSING\n'; elif [ -e \"$target\" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; else tmpdir=$(/usr/bin/mktemp -d \"$workdir/.ai-os-document-convert.XXXXXX\") || exit 1; trap '/bin/rm -rf \"$tmpdir\"' EXIT; if /usr/bin/textutil -convert {} -output \"$tmpdir/output.{}\" \"$source\" >/dev/null 2>&1 && /bin/ln \"$tmpdir/output.{}\" \"$target\"; then size=$(/usr/bin/stat -f %z -- \"$target\") || exit 1; /usr/bin/printf 'AIOS_DOCUMENT_CONVERTED=%s\n' \"$size\"; elif [ -e \"$target\" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; else /usr/bin/printf 'AIOS_FAILED\n'; fi; fi",
        shell_quote(source),
        shell_quote(destination),
        shell_quote(workdir),
        format,
        format,
        format,
    )
}

fn document_convert_output(history: &Value, source: &str, destination: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some((_, size)) = text.rsplit_once("AIOS_DOCUMENT_CONVERTED=") {
        return Some(serde_json::json!({
            "source": source,
            "destination": destination,
            "status": "converted",
            "bytesWritten": size.trim().parse::<u64>().ok()?,
        }));
    }
    for (marker, status) in [
        ("AIOS_SOURCE_MISSING", "source_missing"),
        ("AIOS_EXISTS", "exists"),
        ("AIOS_FAILED", "failed"),
    ] {
        if text.lines().any(|line| line.trim() == marker) {
            return Some(serde_json::json!({
                "source": source,
                "destination": destination,
                "status": status,
            }));
        }
    }
    None
}

fn spreadsheet_create_execution(
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String), OpenClawExecutionError> {
    let (path, content, workdir) = spreadsheet_create_input(request)?;
    let provider = resolve_office_provider(SPREADSHEET_CREATE_ACTION).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No available Office Provider supports spreadsheet.create.",
            false,
        )
    })?;
    if provider.id != OfficeProviderId::MicrosoftOffice {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not yet have a spreadsheet.create adapter.",
                provider.name
            ),
            false,
        ));
    }

    Ok((
        path.to_owned(),
        workdir.to_owned(),
        spreadsheet_create_command(path, content),
    ))
}

fn start_spreadsheet_create(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String), OpenClawExecutionError> {
    let (path, workdir, command) = spreadsheet_create_execution(request)?;
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-spreadsheet-create-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label spreadsheet.create is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. Create only; never overwrite an existing file. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(&workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );

    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    Ok((path, session_key, run_id.to_owned()))
}

fn complete_spreadsheet_create(
    invoker: &dyn GatewayMethodInvoker,
    path: &str,
    session_key: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let history = invoker
        .invoke(
            "chat.history",
            Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
        )
        .map_err(map_gateway_failure)?;
    let output = spreadsheet_create_output(&history, path).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ProtocolFailure,
            "OpenClaw spreadsheet create completed without a valid exec tool result.",
            false,
        )
    })?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("OpenClaw completed the spreadsheet create.".to_owned()),
    })
}

fn finish_spreadsheet_create(
    invoker: &dyn GatewayMethodInvoker,
    path: &str,
    session_key: &str,
    run_id: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;

        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => return complete_spreadsheet_create(invoker, path, session_key),
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw spreadsheet create timed out.",
        true,
    ))
}

fn map_keynote_error(error: crate::document::keynote::KeynoteError) -> OpenClawExecutionError {
    OpenClawExecutionError::new(
        if error.invalid_request {
            OpenClawExecutionErrorKind::InvalidRequest
        } else {
            OpenClawExecutionErrorKind::ExecutionFailed
        },
        error.message,
        false,
    )
}

fn execute_presentation_read(
    _invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let provider =
        resolve_local_presentation_provider(PRESENTATION_READ_ACTION).ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                "No executable local Office Provider supports presentation.read.",
                false,
            )
        })?;

    if provider.id != OfficeProviderId::AppleIwork {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not have a native presentation.read adapter.",
                provider.name
            ),
            false,
        ));
    }

    let output = crate::document::keynote::read_keynote_presentation(&request.input)
        .map_err(map_keynote_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS completed the Keynote presentation read.".to_owned()),
    })
}

fn execute_presentation_create(
    _invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let provider =
        resolve_local_presentation_provider(PRESENTATION_CREATE_ACTION).ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                "No executable local Office Provider supports presentation.create.",
                false,
            )
        })?;

    if provider.id != OfficeProviderId::AppleIwork {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not have a native presentation.create adapter.",
                provider.name
            ),
            false,
        ));
    }

    let output = crate::document::keynote::create_keynote_presentation(&request.input)
        .map_err(map_keynote_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS created and validated the Keynote presentation.".to_owned()),
    })
}

fn execute_spreadsheet_create(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let (path, session_key, run_id) = start_spreadsheet_create(invoker, request)?;
    finish_spreadsheet_create(invoker, &path, &session_key, &run_id)
}

fn execute_spreadsheet_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let (path, session_key, run_id) = start_spreadsheet_read(invoker, request)?;
    finish_spreadsheet_read(invoker, &path, &session_key, &run_id)
}

fn finish_spreadsheet_read(
    invoker: &dyn GatewayMethodInvoker,
    path: &str,
    session_key: &str,
    run_id: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;

        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = spreadsheet_read_output(&history, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw spreadsheet read completed without a valid exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the spreadsheet read.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw spreadsheet read timed out.",
        true,
    ))
}

fn start_spreadsheet_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String), OpenClawExecutionError> {
    let (path, workdir) = spreadsheet_read_input(request)?;
    let provider = resolve_office_provider(SPREADSHEET_READ_ACTION).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No available Office Provider supports spreadsheet.read.",
            false,
        )
    })?;
    if provider.id != OfficeProviderId::MicrosoftOffice {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not yet have a spreadsheet.read adapter.",
                provider.name
            ),
            false,
        ));
    }

    let command = spreadsheet_read_command(path);
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-spreadsheet-read-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label spreadsheet.read is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. This is read-only; close the workbook without saving. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );

    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    Ok((path.to_owned(), session_key, run_id.to_owned()))
}

fn spreadsheet_read_output(history: &Value, path: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some((_, error)) = text.rsplit_once("AIOS_FAILED=") {
        return Some(serde_json::json!({
            "path": path,
            "status": "failed",
            "error": error.trim(),
        }));
    }

    let (_, payload) = text.split_once("AIOS_SHEET=")?;
    let (sheet, payload) = payload.split_once("\nAIOS_ROWS=")?;
    let (rows, payload) = payload.split_once("\nAIOS_COLUMNS=")?;
    let (columns, content) = payload.split_once("\nAIOS_CONTENT_BEGIN\n")?;
    let rows = rows.trim().parse::<u64>().ok()?;
    let columns = columns.trim().parse::<u64>().ok()?;

    Some(serde_json::json!({
        "path": path,
        "sheet": sheet.trim(),
        "status": "table",
        "rows": rows,
        "columns": columns,
        "truncated": text.len() >= MAX_FILE_OUTPUT_BYTES as usize,
        "content": content,
    }))
}

fn spreadsheet_create_command(path: &str, content: &str) -> String {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("xlsx")
        .to_ascii_lowercase();
    format!(
        r#"target={}; content={}; cache="$HOME/Library/Containers/com.microsoft.Excel/Data/Library/Caches/com.microsoft.Excel"; if [ -e "$target" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; elif [ ! -d "$cache" ]; then /usr/bin/printf 'AIOS_FAILED\n'; else tmpdir=$(/usr/bin/mktemp -d "$cache/ai-os-spreadsheet.XXXXXX") || exit 1; trap '/bin/rm -rf "$tmpdir"' EXIT; /usr/bin/printf '%s' "$content" > "$tmpdir/source.tsv" || exit 1; result=$(/usr/bin/osascript - "$tmpdir/source.tsv" "$tmpdir/output.{}" {} "$content" <<'AIOS_APPLESCRIPT'
on joinRow(rowValues)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to tab
    set rowText to rowValues as text
    set AppleScript's text item delimiters to oldDelimiters
    return rowText
end joinRow

on excelColumnName(columnNumber)
    set letters to "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    set resultText to ""
    set remaining to columnNumber
    repeat while remaining > 0
        set letterIndex to ((remaining - 1) mod 26) + 1
        set resultText to character letterIndex of letters & resultText
        set remaining to (remaining - letterIndex) div 26
    end repeat
    return resultText
end excelColumnName

on valuesMatch(actualValue, expectedValue)
    try
        set expectedNumber to expectedValue as number
        return actualValue = expectedNumber
    on error
        return (actualValue as text) = expectedValue
    end try
end valuesMatch

on run argv
    set outputPath to item 2 of argv
    set outputFormat to item 3 of argv
    set expectedContent to item 4 of argv
    set openedWorkbook to missing value
    tell application "Microsoft Excel"
        try
            set openedWorkbook to make new workbook
            set oldDelimiters to AppleScript's text item delimiters
            set AppleScript's text item delimiters to linefeed
            set sourceRows to text items of expectedContent
            repeat with rowIndex from 1 to count of sourceRows
                set sourceRow to item rowIndex of sourceRows
                set AppleScript's text item delimiters to tab
                set rowValues to text items of sourceRow
                repeat with columnIndex from 1 to count of rowValues
                    set cellAddress to my excelColumnName(columnIndex) & rowIndex
                    set value of range cellAddress of worksheet 1 of openedWorkbook to contents of item columnIndex of rowValues
                end repeat
            end repeat
            set AppleScript's text item delimiters to oldDelimiters
            if outputFormat is "xlsx" then
                save workbook as openedWorkbook filename outputPath file format Excel XML file format
            else
                save workbook as openedWorkbook filename outputPath file format Excel98to2004 file format
            end if
            try
                close openedWorkbook saving no
            end try
            set openedWorkbook to missing value
            open workbook workbook file name outputPath
            repeat 20 times
                repeat with workbookIndex from 1 to count of workbooks
                    set candidateWorkbook to workbook workbookIndex
                    set candidatePath to full name of candidateWorkbook
                    if candidatePath is outputPath then
                        set openedWorkbook to candidateWorkbook
                        exit repeat
                    end if
                end repeat
                if openedWorkbook is not missing value then exit repeat
                delay 0.25
            end repeat
            if openedWorkbook is missing value then error "Excel did not reopen the generated workbook"
            tell worksheet 1 of openedWorkbook
                set usedValues to value of used range
            end tell
            if class of usedValues is not list then
                set usedValues to {{usedValues}}
            else if (count of usedValues) > 0 then
                if class of item 1 of usedValues is not list then
                    set usedValues to {{usedValues}}
                end if
            end if
            set contentMatches to (count of usedValues) = (count of sourceRows)
            repeat with rowIndex from 1 to count of sourceRows
                if rowIndex > count of usedValues then
                    set contentMatches to false
                    exit repeat
                end if
                set AppleScript's text item delimiters to tab
                set expectedRow to text items of item rowIndex of sourceRows
                set actualRow to item rowIndex of usedValues
                if class of actualRow is not list then set actualRow to {{actualRow}}
                if (count of actualRow) is not (count of expectedRow) then set contentMatches to false
                repeat with columnIndex from 1 to count of expectedRow
                    if columnIndex > count of actualRow or not my valuesMatch(contents of item columnIndex of actualRow, contents of item columnIndex of expectedRow) then
                        set contentMatches to false
                        exit repeat
                    end if
                end repeat
            end repeat
            set AppleScript's text item delimiters to oldDelimiters
            close openedWorkbook saving no
            set openedWorkbook to missing value
            if contentMatches then
                return "AIOS_EXCEL_SAVED_VALIDATED"
            end if
            return "AIOS_CONTENT_MISMATCH"
        on error
            if openedWorkbook is not missing value then
                try
                    close openedWorkbook saving no
                end try
            end if
            return "AIOS_FAILED"
        end try
    end tell
end run
AIOS_APPLESCRIPT
); output="$tmpdir/output.{}"; if [ "$result" = "AIOS_EXCEL_SAVED_VALIDATED" ] && [ -f "$output" ]; then /bin/mv -n "$output" "$target"; if [ ! -e "$output" ] && [ -f "$target" ]; then size=$(/usr/bin/stat -f %z -- "$target") || exit 1; /usr/bin/printf 'AIOS_SPREADSHEET_CREATED=%s\n' "$size"; elif [ -e "$target" ]; then /usr/bin/printf 'AIOS_EXISTS\n'; else /usr/bin/printf 'AIOS_FAILED\n'; fi; else /usr/bin/printf 'AIOS_FAILED\n'; fi; fi"#,
        shell_quote(path),
        shell_quote(content),
        extension,
        shell_quote(&extension),
        extension,
    )
}

fn spreadsheet_create_output(history: &Value, path: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some((_, size)) = text.rsplit_once("AIOS_SPREADSHEET_CREATED=") {
        return Some(serde_json::json!({
            "path": path,
            "status": "created",
            "bytesWritten": size.trim().parse::<u64>().ok()?,
        }));
    }
    if text.lines().any(|line| line.trim() == "AIOS_EXISTS") {
        return Some(serde_json::json!({"path": path, "status": "exists"}));
    }
    if text.lines().any(|line| line.trim() == "AIOS_FAILED") {
        return Some(serde_json::json!({"path": path, "status": "failed"}));
    }
    None
}

fn spreadsheet_read_command(path: &str) -> String {
    format!(
        r#"/usr/bin/osascript - {} <<'AIOS_APPLESCRIPT' | /usr/bin/head -c {}
on joinRow(rowValues)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to tab
    set rowText to rowValues as text
    set AppleScript's text item delimiters to oldDelimiters
    return rowText
end joinRow

on run argv
    set workbookPath to item 1 of argv
    set openedWorkbook to missing value
    tell application "Microsoft Excel"
        try
            open workbook workbook file name workbookPath
            repeat 20 times
                repeat with workbookIndex from 1 to count of workbooks
                    set candidateWorkbook to workbook workbookIndex
                    set candidatePath to full name of candidateWorkbook
                    if candidatePath is workbookPath then
                        set openedWorkbook to candidateWorkbook
                        exit repeat
                    end if
                end repeat
                if openedWorkbook is not missing value then exit repeat
                delay 0.25
            end repeat
            if openedWorkbook is missing value then error "Excel did not open the workbook"
            tell worksheet 1 of openedWorkbook
                set sheetName to name
                set usedValues to value of used range
            end tell
            if class of usedValues is not list then
                set usedValues to {{usedValues}}
            else if (count of usedValues) > 0 then
                if class of item 1 of usedValues is not list then
                    set usedValues to {{usedValues}}
                end if
            end if
            set rowCount to count of usedValues
            set columnCount to count of item 1 of usedValues
            set outputText to ""
            repeat with rowValues in usedValues
                set outputText to outputText & my joinRow(contents of rowValues) & linefeed
            end repeat
            close openedWorkbook saving no
            return "AIOS_SHEET=" & sheetName & linefeed & ¬
                "AIOS_ROWS=" & rowCount & linefeed & ¬
                "AIOS_COLUMNS=" & columnCount & linefeed & ¬
                "AIOS_CONTENT_BEGIN" & linefeed & outputText
        on error errorMessage number errorNumber
            if openedWorkbook is not missing value then
                try
                    close openedWorkbook saving no
                end try
            end if
            return "AIOS_FAILED=" & errorNumber & ":" & errorMessage
        end try
    end tell
end run
AIOS_APPLESCRIPT"#,
        shell_quote(path),
        MAX_FILE_OUTPUT_BYTES,
    )
}

fn spreadsheet_read_input(
    request: &OpenClawExecutionRequest,
) -> Result<(&str, &str), OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.read requires path",
                false,
            )
        })?;

    let workbook_path = Path::new(path);
    if !workbook_path.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.read requires an absolute file path",
            false,
        ));
    }

    let extension = workbook_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(extension.as_str(), "xls" | "xlsx") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.read currently supports XLS and XLSX files",
            false,
        ));
    }

    let workdir = workbook_path
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.read requires an absolute file path",
                false,
            )
        })?;

    Ok((path, workdir))
}

fn spreadsheet_create_target(
    request: &OpenClawExecutionRequest,
) -> Result<(&str, &str), OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.create requires path",
                false,
            )
        })?;

    let target = Path::new(path);
    if !target.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.create requires an absolute file path",
            false,
        ));
    }

    let extension = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(extension.as_str(), "xls" | "xlsx") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.create currently supports XLS and XLSX files",
            false,
        ));
    }

    let workdir = target.parent().and_then(Path::to_str).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.create requires an absolute file path",
            false,
        )
    })?;

    Ok((path, workdir))
}

fn spreadsheet_create_input<'a>(
    request: &'a OpenClawExecutionRequest,
) -> Result<(&'a str, &'a str, &'a str), OpenClawExecutionError> {
    let (path, workdir) = spreadsheet_create_target(request)?;
    let content = request
        .input
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.create requires TSV content",
                false,
            )
        })?;

    if content.len() > MAX_FILE_WRITE_BYTES {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.create content exceeds the 4096 byte limit",
            false,
        ));
    }

    if request
        .input
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.create overwrite is not permitted",
            false,
        ));
    }

    Ok((path, content, workdir))
}

fn document_convert_input<'a>(
    request: &'a OpenClawExecutionRequest,
) -> Result<(&'a str, &'a str, &'a str, String), OpenClawExecutionError> {
    let required_path = |field: &str| {
        request
            .input
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::InvalidRequest,
                    format!("document.convert requires {field}"),
                    false,
                )
            })
    };
    let source = required_path("source")?;
    let destination = required_path("destination")?;

    if request
        .input
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.convert overwrite is not permitted",
            false,
        ));
    }

    let source_path = Path::new(source);
    let destination_path = Path::new(destination);
    if !source_path.is_absolute() || !destination_path.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.convert requires absolute source and destination paths",
            false,
        ));
    }
    if source_path == destination_path {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.convert source and destination must differ",
            false,
        ));
    }

    let extension = |value: &Path| {
        value
            .extension()
            .and_then(|item| item.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default()
    };
    let source_format = extension(source_path);
    let destination_format = extension(destination_path);
    if !matches!(source_format.as_str(), "doc" | "docx")
        || !matches!(destination_format.as_str(), "doc" | "docx")
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.convert currently supports DOC and DOCX files",
            false,
        ));
    }

    let workdir = destination_path
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "document.convert requires an absolute destination path",
                false,
            )
        })?;

    Ok((source, destination, workdir, destination_format))
}

fn document_create_input<'a>(
    request: &'a OpenClawExecutionRequest,
) -> Result<(&'a str, &'a str, &'a str), OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "document.create requires path",
                false,
            )
        })?;
    let content = request
        .input
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "document.create requires text content",
                false,
            )
        })?;
    if content.len() > MAX_FILE_WRITE_BYTES {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create content exceeds the 4096 byte limit",
            false,
        ));
    }
    if request
        .input
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create overwrite is not permitted",
            false,
        ));
    }

    let target = Path::new(path);
    if !target.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create requires an absolute file path",
            false,
        ));
    }
    let format = target
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(format.as_str(), "doc" | "docx") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create currently supports DOC and DOCX files",
            false,
        ));
    }
    let workdir = target.parent().and_then(Path::to_str).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create requires an absolute file path",
            false,
        )
    })?;

    Ok((path, content, workdir))
}

fn execute_document_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "document.read requires path",
                false,
            )
        })?;

    let document_path = Path::new(path);
    if !document_path.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.read requires an absolute file path",
            false,
        ));
    }

    let provider = resolve_office_provider(DOCUMENT_READ_ACTION).ok_or_else(|| {
        OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            "No available Office Provider supports document.read.",
            false,
        )
    })?;

    if provider.id != OfficeProviderId::MacosNative {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office Provider {} does not yet have a document.read adapter.",
                provider.name
            ),
            false,
        ));
    }

    let extension = document_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(extension.as_str(), "doc" | "docx") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.read currently supports DOC and DOCX files",
            false,
        ));
    }

    let workdir = document_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "document.read requires an absolute file path",
                false,
            )
        })?;

    let quoted_path = shell_quote(path);
    let command = format!(
        "/usr/bin/textutil -convert txt -stdout -- {quoted_path} | /usr/bin/head -c {MAX_FILE_OUTPUT_BYTES}"
    );
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-document-read-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label document.read is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. This is read-only. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );

    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;

    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|run_id| !run_id.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;

        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = document_read_output(&history, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw document read completed without a valid exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the document read.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw document read timed out.",
        true,
    ))
}

fn document_read_output(history: &Value, path: &str) -> Option<Value> {
    let tool_result = latest_successful_exec_result(history)?;
    let content = tool_result
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    Some(serde_json::json!({
        "path": path,
        "status": "text",
        "content": content,
    }))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn filesystem_read_output(history: &Value, path: &str) -> Option<Value> {
    let tool_result = latest_successful_exec_result(history)?;
    let text = tool_result
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let (_, payload) = text.split_once("AIOS_MIME=")?;
    let (mime_type, payload) = payload.split_once("\nAIOS_SIZE=")?;
    let (size, content) = payload.split_once("\nAIOS_CONTENT_BEGIN\n")?;
    let size = size.parse::<u64>().ok()?;
    if content.trim_end() == "AIOS_TOO_LARGE" {
        return Some(serde_json::json!({
            "path": path,
            "mimeType": mime_type,
            "size": size,
            "status": "too_large",
            "limitBytes": MAX_FILE_READ_BYTES,
        }));
    }
    if content.trim_end() == "AIOS_UNSUPPORTED" {
        return Some(serde_json::json!({
            "path": path,
            "mimeType": mime_type,
            "size": size,
            "status": "unsupported",
        }));
    }
    Some(serde_json::json!({
        "path": path,
        "mimeType": mime_type,
        "size": size,
        "status": "text",
        "truncated": size > MAX_FILE_OUTPUT_BYTES,
        "content": content,
    }))
}

fn execute_filesystem_write(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let path = request
        .input
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.write requires path",
                false,
            )
        })?;
    let content = request
        .input
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.write requires text content",
                false,
            )
        })?;
    if content.len() > MAX_FILE_WRITE_BYTES {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.write content exceeds the 4096 byte limit",
            false,
        ));
    }
    if request
        .input
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.write overwrite is not permitted",
            false,
        ));
    }
    let target_path = Path::new(path);
    if !target_path.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.write requires an absolute file path",
            false,
        ));
    }
    let workdir = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.write requires an absolute file path",
                false,
            )
        })?;
    let command = format!(
        "target={}; content={}; if [ -e \"$target\" ]; then /usr/bin/printf 'AIOS_EXISTS\\n'; else set -C; umask 077; if /usr/bin/printf '%s' \"$content\" > \"$target\"; then size=$(/usr/bin/stat -f %z -- \"$target\") || exit 1; /usr/bin/printf 'AIOS_WRITTEN=%s\\n' \"$size\"; else /usr/bin/printf 'AIOS_FAILED\\n'; fi; fi",
        shell_quote(path),
        shell_quote(content),
    );
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-core-skill-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label filesystem.write is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. Create only; never overwrite an existing file. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );
    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|run_id| !run_id.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output = filesystem_write_output(&history, path).ok_or_else(|| {
                    OpenClawExecutionError::new(
                        OpenClawExecutionErrorKind::ProtocolFailure,
                        "OpenClaw file write completed without a valid exec tool result.",
                        false,
                    )
                })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the file write.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw agent execution timed out.",
        true,
    ))
}

fn filesystem_write_output(history: &Value, path: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some((_, size)) = text.rsplit_once("AIOS_WRITTEN=") {
        return Some(serde_json::json!({
            "path": path,
            "status": "written",
            "bytesWritten": size.trim().parse::<u64>().ok()?,
        }));
    }
    if text.lines().any(|line| line.trim() == "AIOS_EXISTS") {
        return Some(serde_json::json!({"path": path, "status": "exists"}));
    }
    if text.lines().any(|line| line.trim() == "AIOS_FAILED") {
        return Some(serde_json::json!({"path": path, "status": "failed"}));
    }
    None
}

fn execute_filesystem_move(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    let source = request
        .input
        .get("source")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.move requires source",
                false,
            )
        })?;
    let destination = request
        .input
        .get("destination")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.move requires destination",
                false,
            )
        })?;
    if request
        .input
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.move overwrite is not permitted",
            false,
        ));
    }
    let source_path = Path::new(source);
    let destination_path = Path::new(destination);
    if !source_path.is_absolute() || !destination_path.is_absolute() {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.move requires absolute source and destination paths",
            false,
        ));
    }
    if source_path == destination_path {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "filesystem.move requires different source and destination paths",
            false,
        ));
    }
    let workdir = destination_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .and_then(Path::to_str)
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "filesystem.move requires an absolute destination file path",
                false,
            )
        })?;
    let command = format!(
        "source={}; destination={}; if [ ! -e \"$source\" ]; then /usr/bin/printf 'AIOS_SOURCE_MISSING\\n'; elif [ -e \"$destination\" ]; then /usr/bin/printf 'AIOS_DESTINATION_EXISTS\\n'; elif /bin/mv -n -- \"$source\" \"$destination\"; then if [ ! -e \"$source\" ] && [ -e \"$destination\" ]; then /usr/bin/printf 'AIOS_MOVED\\n'; else /usr/bin/printf 'AIOS_FAILED\\n'; fi; else /usr/bin/printf 'AIOS_FAILED\\n'; fi",
        shell_quote(source),
        shell_quote(destination),
    );
    let session_key = format!(
        "agent:{FILESYSTEM_AGENT_ID}:ai-os-core-skill-{}",
        request.execution_id
    );
    let message = format!(
        "This is an AI-OS internal capability request. The label filesystem.move is NOT an OpenClaw tool name. Call the existing exec tool exactly once with command {} and workdir {}. Keep background false and yieldMs 10000. Use the command exactly as supplied without adding or changing flags, pipes, or wrappers. Move only; never overwrite an existing destination. Do not claim success without the tool output.",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".to_owned()),
        serde_json::to_string(workdir).unwrap_or_else(|_| "\"\"".to_owned())
    );
    let accepted = invoker
        .invoke(
            "agent",
            Some(serde_json::json!({
                "message": message,
                "agentId": FILESYSTEM_AGENT_ID,
                "sessionKey": session_key,
                "thinking": "off",
                "deliver": false,
                "timeout": 120,
                "idempotencyKey": request.execution_id.as_str(),
            })),
        )
        .map_err(map_gateway_failure)?;
    let run_id = accepted
        .get("runId")
        .and_then(Value::as_str)
        .filter(|run_id| !run_id.trim().is_empty())
        .ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ProtocolFailure,
                "OpenClaw did not return a run identifier.",
                false,
            )
        })?;

    for _ in 0..AGENT_WAIT_ATTEMPTS {
        let terminal = invoker
            .invoke(
                "agent.wait",
                Some(serde_json::json!({"runId": run_id, "timeoutMs": 9_000})),
            )
            .map_err(map_gateway_failure)?;
        match terminal.get("status").and_then(Value::as_str) {
            Some("ok") => {
                let history = invoker
                    .invoke(
                        "chat.history",
                        Some(serde_json::json!({"sessionKey": session_key, "limit": 10})),
                    )
                    .map_err(map_gateway_failure)?;
                let output =
                    filesystem_move_output(&history, source, destination).ok_or_else(|| {
                        OpenClawExecutionError::new(
                            OpenClawExecutionErrorKind::ProtocolFailure,
                            "OpenClaw file move completed without a valid exec tool result.",
                            false,
                        )
                    })?;
                return Ok(OpenClawExecutionResult {
                    output,
                    summary: Some("OpenClaw completed the file move.".to_owned()),
                });
            }
            Some("error") => {
                let message = terminal
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenClaw agent execution failed.");
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ExecutionFailed,
                    message,
                    false,
                ));
            }
            Some("timeout") => continue,
            _ => {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::ProtocolFailure,
                    "OpenClaw returned an invalid agent terminal status.",
                    false,
                ));
            }
        }
    }

    Err(OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        "OpenClaw agent execution timed out.",
        true,
    ))
}

fn filesystem_move_output(history: &Value, source: &str, destination: &str) -> Option<Value> {
    let text = latest_successful_exec_result(history)?
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let status = if text.lines().any(|line| line.trim() == "AIOS_MOVED") {
        "moved"
    } else if text
        .lines()
        .any(|line| line.trim() == "AIOS_SOURCE_MISSING")
    {
        "source_missing"
    } else if text
        .lines()
        .any(|line| line.trim() == "AIOS_DESTINATION_EXISTS")
    {
        "destination_exists"
    } else if text.lines().any(|line| line.trim() == "AIOS_FAILED") {
        "failed"
    } else {
        return None;
    };
    Some(serde_json::json!({
        "source": source,
        "destination": destination,
        "status": status,
    }))
}

fn map_gateway_failure(failure: ActiveGatewayMethodFailure) -> OpenClawExecutionError {
    let (kind, retryable) = match failure.kind {
        ActiveGatewayFailureKind::Unauthorized => {
            (OpenClawExecutionErrorKind::AuthenticationRequired, false)
        }
        ActiveGatewayFailureKind::PairingRequired => {
            (OpenClawExecutionErrorKind::PairingRequired, false)
        }
        ActiveGatewayFailureKind::Unreachable => {
            (OpenClawExecutionErrorKind::ConnectionUnavailable, true)
        }
        ActiveGatewayFailureKind::Protocol => (OpenClawExecutionErrorKind::ProtocolFailure, false),
        ActiveGatewayFailureKind::NoActiveServer => {
            (OpenClawExecutionErrorKind::ConnectionUnavailable, false)
        }
        ActiveGatewayFailureKind::Unknown => (OpenClawExecutionErrorKind::ExecutionFailed, false),
    };

    OpenClawExecutionError::new(kind, failure.message, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::openclaw_execution::OpenClawExecutionRequest;
    use serde_json::json;
    use std::{collections::VecDeque, sync::Mutex};

    struct RecordingInvoker {
        calls: Mutex<Vec<(String, Option<Value>)>>,
        outcome: Result<Value, ActiveGatewayMethodFailure>,
    }

    impl GatewayMethodInvoker for RecordingInvoker {
        fn invoke(
            &self,
            method: &str,
            params: Option<Value>,
        ) -> Result<Value, ActiveGatewayMethodFailure> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            self.outcome.clone()
        }
    }

    struct ScriptedInvoker {
        calls: Mutex<Vec<(String, Option<Value>)>>,
        outcomes: Mutex<VecDeque<Result<Value, ActiveGatewayMethodFailure>>>,
    }

    impl GatewayMethodInvoker for ScriptedInvoker {
        fn invoke(
            &self,
            method: &str,
            params: Option<Value>,
        ) -> Result<Value, ActiveGatewayMethodFailure> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("missing scripted Gateway outcome")
        }
    }

    fn request(action: &str, input: Value) -> OpenClawExecutionRequest {
        OpenClawExecutionRequest::new("runtime-execution-123", action, input).unwrap()
    }

    #[test]
    fn filesystem_scan_runs_agent_wait_and_returns_persisted_result() {
        let history = json!({"messages": [
            {
                "role": "toolResult",
                "toolName": "exec",
                "isError": false,
                "content": [{
                    "type": "text",
                    "text": "./.DS_Store\n./report.docx"
                }]
            },
            {"role": "assistant", "content": "scan result"}
        ]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "run-123"})),
                Ok(json!({"runId": "run-123", "status": "timeout"})),
                Ok(json!({"runId": "run-123", "status": "ok"})),
                Ok(history.clone()),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request("filesystem.scan", json!({"path": "/safe/example"})),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({"path": "/safe/example", "entries": [".DS_Store", "report.docx"]})
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "agent.wait", "chat.history",]
        );
        assert_eq!(calls[0].1.as_ref().unwrap()["agentId"], "ai-os-files");
        assert_eq!(calls[0].1.as_ref().unwrap()["thinking"], "off");
        assert!(calls[0].1.as_ref().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("/safe/example"));
        assert!(calls[0].1.as_ref().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("NOT an OpenClaw tool name"));
        assert!(calls[0].1.as_ref().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("/usr/bin/find . -mindepth 1 -maxdepth 1 -print"));
        assert_eq!(calls[1].1.as_ref().unwrap()["runId"], "run-123");
        assert!(calls.iter().all(|call| call.0 != "filesystem.scan"));
    }

    #[test]
    fn filesystem_output_accepts_nested_openclaw_transcript_messages() {
        let history = json!({
            "messages": [{
                "type": "message",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tool-call-123",
                    "toolName": "exec",
                    "content": [{
                        "type": "text",
                        "text": "./.DS_Store\n./report.docx"
                    }],
                    "details": {
                        "status": "completed",
                        "exitCode": 0
                    },
                    "isError": false
                }
            }]
        });

        assert_eq!(
            filesystem_scan_output(&history, "/safe/example"),
            Some(json!({
                "path": "/safe/example",
                "entries": [".DS_Store", "report.docx"]
            }))
        );
    }

    #[test]
    fn filesystem_scan_rejects_command_error_reported_as_successful_tool_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": "find: -printf: unknown primary or operator"}]
        }]});

        assert_eq!(filesystem_scan_output(&history, "/safe/example"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn spreadsheet_create_rejects_invalid_path_without_gateway_call() {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({"unexpected": true})),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "spreadsheet.create",
                json!({
                    "path": "relative/report.xlsx",
                    "content": "Name\tValue\nAlpha\t42"
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(!error.retryable);
        assert!(error.message.contains("absolute file path"));
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn spreadsheet_create_uses_excel_and_returns_created_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_SPREADSHEET_CREATED=4096\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "spreadsheet-create-run"})),
                Ok(json!({"runId": "spreadsheet-create-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "spreadsheet.create",
                json!({
                    "path": "/safe/report file.xlsx",
                    "content": "Name\tValue\nAlpha\t42"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/report file.xlsx",
                "status": "created",
                "bytesWritten": 4096,
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("spreadsheet.create is NOT an OpenClaw tool name"));
        assert!(message.contains("Microsoft Excel"));
        assert!(message.contains("com.microsoft.Excel/Data/Library/Caches"));
        assert!(message.contains("never overwrite"));
        assert!(calls.iter().all(|call| call.0 != "spreadsheet.create"));
    }

    #[test]
    fn spreadsheet_read_uses_excel_and_returns_table_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text":
                "AIOS_SHEET=Sheet1\nAIOS_ROWS=2\nAIOS_COLUMNS=2\nAIOS_CONTENT_BEGIN\nName\tValue\nAlpha\t42.0\n"
            }]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "spreadsheet-read-run"})),
                Ok(json!({"runId": "spreadsheet-read-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "spreadsheet.read",
                json!({"path": "/safe/report file.xlsx"}),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/report file.xlsx",
                "sheet": "Sheet1",
                "status": "table",
                "rows": 2,
                "columns": 2,
                "truncated": false,
                "content": "Name\tValue\nAlpha\t42.0\n",
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("spreadsheet.read is NOT an OpenClaw tool name"));
        assert!(message.contains("Microsoft Excel"));
        assert!(message.contains("close openedWorkbook saving no"));
        assert!(message.contains("/usr/bin/head -c 65536"));
        assert!(calls.iter().all(|call| call.0 != "spreadsheet.read"));
    }

    #[test]
    fn spreadsheet_read_helpers_use_excel_and_map_table_results() {
        let command = spreadsheet_read_command("/safe/report file.xlsx");

        assert!(command.contains("/usr/bin/osascript"));
        assert!(command.contains("Microsoft Excel"));
        assert!(command.contains("value of used range"));
        assert!(command.contains("close openedWorkbook saving no"));
        assert!(command.contains("/usr/bin/head -c 65536"));

        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text":
                "AIOS_SHEET=Sheet1\nAIOS_ROWS=2\nAIOS_COLUMNS=2\nAIOS_CONTENT_BEGIN\nName\tValue\nAlpha\t42.0\n"
            }]
        }]});
        assert_eq!(
            spreadsheet_read_output(&history, "/safe/report.xlsx").unwrap(),
            json!({
                "path": "/safe/report.xlsx",
                "sheet": "Sheet1",
                "status": "table",
                "rows": 2,
                "columns": 2,
                "truncated": false,
                "content": "Name\tValue\nAlpha\t42.0\n",
            })
        );

        let failed = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_FAILED=-1728:Workbook unavailable"}]
        }]});
        assert_eq!(
            spreadsheet_read_output(&failed, "/safe/report.xlsx").unwrap(),
            json!({
                "path": "/safe/report.xlsx",
                "status": "failed",
                "error": "-1728:Workbook unavailable",
            })
        );
    }

    #[test]
    fn spreadsheet_create_helpers_enforce_container_save_and_map_results() {
        let command = spreadsheet_create_command("/safe/report.xlsx", "Name\tValue\nAlpha\t42");

        assert!(command.contains("Microsoft Excel"));
        assert!(command.contains("com.microsoft.Excel/Data/Library/Caches"));
        assert!(command.contains("Excel XML file format"));
        assert!(command.contains("/bin/mv -n"));
        assert!(command.contains("AIOS_SPREADSHEET_CREATED="));

        let created = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_SPREADSHEET_CREATED=4096\n"}]
        }]});
        assert_eq!(
            spreadsheet_create_output(&created, "/safe/report.xlsx").unwrap(),
            json!({
                "path": "/safe/report.xlsx",
                "status": "created",
                "bytesWritten": 4096,
            })
        );

        for (marker, status) in [("AIOS_EXISTS\n", "exists"), ("AIOS_FAILED\n", "failed")] {
            let history = json!({"messages": [{
                "role": "toolResult",
                "toolName": "exec",
                "isError": false,
                "content": [{"text": marker}]
            }]});
            assert_eq!(
                spreadsheet_create_output(&history, "/safe/report.xlsx").unwrap(),
                json!({"path": "/safe/report.xlsx", "status": status})
            );
        }
    }

    #[test]
    fn spreadsheet_read_input_accepts_workbooks_and_rejects_invalid_paths() {
        for path in ["/safe/report.xlsx", "/safe/legacy.XLS"] {
            let request = request("spreadsheet.read", json!({"path": path}));

            assert_eq!(spreadsheet_read_input(&request).unwrap(), (path, "/safe"));
        }

        for path in ["relative.xlsx", "/safe/report.csv", "/safe/no-extension"] {
            let request = request("spreadsheet.read", json!({"path": path}));
            let error = spreadsheet_read_input(&request).unwrap_err();

            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
            assert!(!error.retryable);
        }
    }

    #[test]
    fn spreadsheet_create_input_enforces_safe_create_contract() {
        let valid = request(
            "spreadsheet.create",
            json!({
                "path": "/safe/report.xlsx",
                "content": "Name\tValue\nAlpha\t42"
            }),
        );
        assert_eq!(
            spreadsheet_create_input(&valid).unwrap(),
            ("/safe/report.xlsx", "Name\tValue\nAlpha\t42", "/safe")
        );

        for input in [
            json!({"path": "relative.xlsx", "content": "A\tB"}),
            json!({"path": "/safe/report.csv", "content": "A\tB"}),
            json!({"path": "/safe/report.xlsx", "content": "A\tB", "overwrite": true}),
            json!({"path": "/safe/report.xlsx"}),
        ] {
            let error =
                spreadsheet_create_input(&request("spreadsheet.create", input)).unwrap_err();
            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
            assert!(!error.retryable);
        }

        let oversized = "x".repeat(MAX_FILE_WRITE_BYTES + 1);
        let oversized_request = request(
            "spreadsheet.create",
            json!({"path": "/safe/report.xlsx", "content": oversized}),
        );
        let error = spreadsheet_create_input(&oversized_request).unwrap_err();
        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(error.message.contains("4096 byte limit"));
    }

    #[test]
    fn document_convert_uses_native_textutil_and_returns_converted_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_DOCUMENT_CONVERTED=3072\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "document-convert-run"})),
                Ok(json!({"runId": "document-convert-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "document.convert",
                json!({
                    "source": "/safe/source file.doc",
                    "destination": "/safe/result file.docx"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "source": "/safe/source file.doc",
                "destination": "/safe/result file.docx",
                "status": "converted",
                "bytesWritten": 3072,
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("document.convert is NOT an OpenClaw tool name"));
        assert!(message.contains("/usr/bin/textutil -convert docx"));
        assert!(message.contains("/bin/ln"));
        assert!(message.contains("never overwrite"));
        assert!(calls.iter().all(|call| call.0 != "document.convert"));
    }

    #[test]
    fn document_convert_native_helpers_enforce_safe_conversion_and_map_results() {
        let command = document_convert_command(
            "/safe/source file.doc",
            "/safe/result file.docx",
            "/safe",
            "docx",
        );

        assert!(command.contains("/usr/bin/textutil -convert docx"));
        assert!(command.contains("[ ! -f \"$source\" ]"));
        assert!(command.contains("/usr/bin/mktemp -d"));
        assert!(command.contains("/bin/ln"));
        assert!(command.contains("AIOS_SOURCE_MISSING"));
        assert!(command.contains("AIOS_EXISTS"));

        let converted = json!({"messages": [{
            "role": "toolResult", "toolName": "exec", "isError": false,
            "content": [{"text": "AIOS_DOCUMENT_CONVERTED=3072\n"}]
        }]});
        assert_eq!(
            document_convert_output(&converted, "/safe/source.doc", "/safe/result.docx",).unwrap(),
            json!({
                "source": "/safe/source.doc",
                "destination": "/safe/result.docx",
                "status": "converted",
                "bytesWritten": 3072,
            })
        );

        for (marker, status) in [
            ("AIOS_SOURCE_MISSING", "source_missing"),
            ("AIOS_EXISTS", "exists"),
            ("AIOS_FAILED", "failed"),
        ] {
            let history = json!({"messages": [{
                "role": "toolResult", "toolName": "exec", "isError": false,
                "content": [{"text": format!("{marker}\n")}]
            }]});
            assert_eq!(
                document_convert_output(&history, "/safe/source.doc", "/safe/result.docx",)
                    .unwrap()["status"],
                json!(status)
            );
        }
    }

    #[test]
    fn document_convert_input_enforces_safe_conversion_contract() {
        let valid = request(
            "document.convert",
            json!({
                "source": "/safe/source.doc",
                "destination": "/safe/result.docx"
            }),
        );
        assert_eq!(
            document_convert_input(&valid).unwrap(),
            (
                "/safe/source.doc",
                "/safe/result.docx",
                "/safe",
                "docx".to_owned(),
            )
        );

        for input in [
            json!({"source": "relative.doc", "destination": "/safe/result.docx"}),
            json!({"source": "/safe/file.docx", "destination": "/safe/file.docx"}),
            json!({"source": "/safe/source.txt", "destination": "/safe/result.docx"}),
            json!({
                "source": "/safe/source.doc",
                "destination": "/safe/result.docx",
                "overwrite": true
            }),
        ] {
            let request = request("document.convert", input);
            let error = document_convert_input(&request).unwrap_err();
            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
            assert!(!error.retryable);
        }
    }

    #[test]
    fn document_create_uses_native_textutil_and_returns_created_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_DOCUMENT_CREATED=2048\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "document-create-run"})),
                Ok(json!({"runId": "document-create-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "document.create",
                json!({
                    "path": "/safe/example/new report.docx",
                    "content": "Document body"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/example/new report.docx",
                "status": "created",
                "bytesWritten": 2048,
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("document.create is NOT an OpenClaw tool name"));
        assert!(message.contains("/usr/bin/textutil -convert docx"));
        assert!(message.contains("/bin/ln"));
        assert!(message.contains("never overwrite"));
        assert!(calls.iter().all(|call| call.0 != "document.create"));
    }

    #[test]
    fn document_create_native_helpers_enforce_no_overwrite_and_map_results() {
        let command =
            document_create_command("/safe/O'Reilly report.docx", "Document body", "/safe");

        assert!(command.contains("/usr/bin/textutil -convert docx"));
        assert!(command.contains("/usr/bin/mktemp -d"));
        assert!(command.contains("/bin/ln"));
        assert!(command.contains("AIOS_EXISTS"));
        assert!(command.contains(r#"target='/safe/O'\''Reilly report.docx'"#));

        let created = json!({"messages": [{
            "role": "toolResult", "toolName": "exec", "isError": false,
            "content": [{"text": "AIOS_DOCUMENT_CREATED=2048\n"}]
        }]});
        let exists = json!({"messages": [{
            "role": "toolResult", "toolName": "exec", "isError": false,
            "content": [{"text": "AIOS_EXISTS\n"}]
        }]});
        let failed = json!({"messages": [{
            "role": "toolResult", "toolName": "exec", "isError": false,
            "content": [{"text": "AIOS_FAILED\n"}]
        }]});

        assert_eq!(
            document_create_output(&created, "/safe/report.docx").unwrap(),
            json!({
                "path": "/safe/report.docx",
                "status": "created",
                "bytesWritten": 2048,
            })
        );
        assert_eq!(
            document_create_output(&exists, "/safe/report.docx").unwrap(),
            json!({"path": "/safe/report.docx", "status": "exists"})
        );
        assert_eq!(
            document_create_output(&failed, "/safe/report.docx").unwrap(),
            json!({"path": "/safe/report.docx", "status": "failed"})
        );
    }

    #[test]
    fn document_create_input_enforces_safe_create_contract() {
        let valid = request(
            "document.create",
            json!({"path": "/safe/report.docx", "content": "Document body"}),
        );
        assert_eq!(
            document_create_input(&valid).unwrap(),
            ("/safe/report.docx", "Document body", "/safe")
        );

        for input in [
            json!({"path": "relative.docx", "content": "body"}),
            json!({"path": "/safe/report.docx", "content": "body", "overwrite": true}),
            json!({"path": "/safe/report.txt", "content": "body"}),
        ] {
            let error = document_create_input(&request("document.create", input)).unwrap_err();
            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
            assert!(!error.retryable);
        }

        let oversized = "x".repeat(MAX_FILE_WRITE_BYTES + 1);
        let request = request(
            "document.create",
            json!({"path": "/safe/report.docx", "content": oversized}),
        );
        let error = document_create_input(&request).unwrap_err();
        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(error.message.contains("4096 byte limit"));
    }

    #[test]
    fn document_read_rejects_relative_path_without_gateway_call() {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({"unexpected": true})),
        };

        let error = execute_with_invoker(
            &invoker,
            &request("document.read", json!({"path": "relative/report.docx"})),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        assert!(!error.retryable);
        assert!(error.message.contains("absolute file path"));
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn document_read_uses_native_textutil_and_returns_text_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": "Document body\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "document-read-run"})),
                Ok(json!({"runId": "document-read-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "document.read",
                json!({
                    "path": "/safe/example/read me.docx"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/example/read me.docx",
                "status": "text",
                "content": "Document body\n",
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("document.read is NOT an OpenClaw tool name"));
        assert!(message.contains("/usr/bin/textutil -convert txt -stdout"));
        assert!(message.contains("/usr/bin/head -c 65536"));
        assert!(calls.iter().all(|call| call.0 != "document.read"));
    }

    #[test]
    fn filesystem_read_runs_agent_and_returns_limited_text_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{
                "type": "text",
                "text": "AIOS_MIME=text/plain\nAIOS_SIZE=12\nAIOS_CONTENT_BEGIN\nhello world\n"
            }]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "read-run"})),
                Ok(json!({"runId": "read-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "filesystem.read",
                json!({"path": "/safe/example/read me.txt"}),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/example/read me.txt",
                "mimeType": "text/plain",
                "size": 12,
                "status": "text",
                "truncated": false,
                "content": "hello world\n",
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("filesystem.read is NOT an OpenClaw tool name"));
        assert!(message.contains("/usr/bin/file -b --mime-type"));
        assert!(message.contains("/usr/bin/head -c 65536"));
        assert!(calls.iter().all(|call| call.0 != "filesystem.read"));
    }

    #[test]
    fn filesystem_read_reports_binary_and_oversized_results_without_content() {
        let binary = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_MIME=image/jpeg\nAIOS_SIZE=2048\nAIOS_CONTENT_BEGIN\nAIOS_UNSUPPORTED\n"}]
        }]});
        let oversized = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_MIME=text/plain\nAIOS_SIZE=1000001\nAIOS_CONTENT_BEGIN\nAIOS_TOO_LARGE\n"}]
        }]});

        assert_eq!(
            filesystem_read_output(&binary, "/safe/image.jpeg").unwrap(),
            json!({
                "path": "/safe/image.jpeg",
                "mimeType": "image/jpeg",
                "size": 2048,
                "status": "unsupported",
            })
        );
        assert_eq!(
            filesystem_read_output(&oversized, "/safe/large.txt").unwrap(),
            json!({
                "path": "/safe/large.txt",
                "mimeType": "text/plain",
                "size": 1000001,
                "status": "too_large",
                "limitBytes": 1000000,
            })
        );
    }

    #[test]
    fn filesystem_read_shell_quotes_single_quotes_in_paths() {
        assert_eq!(
            shell_quote("/safe/O'Reilly.txt"),
            "'/safe/O'\\''Reilly.txt'"
        );
    }

    #[test]
    fn filesystem_write_runs_agent_and_returns_created_file_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": "AIOS_WRITTEN=11\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "write-run"})),
                Ok(json!({"runId": "write-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "filesystem.write",
                json!({
                    "path": "/safe/example/new file.txt",
                    "content": "hello world",
                    "overwrite": false,
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/example/new file.txt",
                "status": "written",
                "bytesWritten": 11,
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("filesystem.write is NOT an OpenClaw tool name"));
        assert!(message.contains("set -C"));
        assert!(message.contains("never overwrite"));
        assert!(calls.iter().all(|call| call.0 != "filesystem.write"));
    }

    #[test]
    fn filesystem_write_reports_existing_and_failed_targets() {
        let existing = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_EXISTS\n"}]
        }]});
        let failed = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"text": "AIOS_FAILED\n"}]
        }]});

        assert_eq!(
            filesystem_write_output(&existing, "/safe/existing.txt").unwrap(),
            json!({"path": "/safe/existing.txt", "status": "exists"})
        );
        assert_eq!(
            filesystem_write_output(&failed, "/safe/failed.txt").unwrap(),
            json!({"path": "/safe/failed.txt", "status": "failed"})
        );
    }

    #[test]
    fn filesystem_write_rejects_overwrite_oversize_and_relative_paths() {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: ActiveGatewayFailureKind::Protocol,
                message: "unexpected Gateway call".to_owned(),
            }),
        };
        for (input, expected_message) in [
            (
                json!({"path": "/safe/file.txt", "content": "replacement", "overwrite": true}),
                "filesystem.write overwrite is not permitted",
            ),
            (
                json!({"path": "/safe/file.txt", "content": "x".repeat(4097)}),
                "filesystem.write content exceeds the 4096 byte limit",
            ),
            (
                json!({"path": "relative/file.txt", "content": "content"}),
                "filesystem.write requires an absolute file path",
            ),
        ] {
            let error =
                execute_with_invoker(&invoker, &request("filesystem.write", input), &mut |_| {})
                    .unwrap_err();
            assert_eq!(
                error.kind,
                OpenClawExecutionErrorKind::InvalidRequest,
                "{expected_message}"
            );
            assert!(error.message.ends_with(expected_message));
        }
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn filesystem_move_runs_agent_and_returns_moved_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": "AIOS_MOVED\n"}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"status": "accepted", "runId": "move-run"})),
                Ok(json!({"runId": "move-run", "status": "ok"})),
                Ok(history),
            ])),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "filesystem.move",
                json!({
                    "source": "/safe/source file.txt",
                    "destination": "/safe/archive/destination file.txt",
                    "overwrite": false,
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "source": "/safe/source file.txt",
                "destination": "/safe/archive/destination file.txt",
                "status": "moved",
            })
        );
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait", "chat.history"]
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.contains("filesystem.move is NOT an OpenClaw tool name"));
        assert!(message.contains("/bin/mv -n"));
        assert!(message.contains("never overwrite"));
        assert!(calls.iter().all(|call| call.0 != "filesystem.move"));
    }

    #[test]
    fn filesystem_move_reports_fail_closed_statuses() {
        for (marker, status) in [
            ("AIOS_SOURCE_MISSING", "source_missing"),
            ("AIOS_DESTINATION_EXISTS", "destination_exists"),
            ("AIOS_FAILED", "failed"),
        ] {
            let history = json!({"messages": [{
                "role": "toolResult",
                "toolName": "exec",
                "isError": false,
                "content": [{"text": marker}]
            }]});
            assert_eq!(
                filesystem_move_output(&history, "/safe/source.txt", "/safe/destination.txt")
                    .unwrap(),
                json!({
                    "source": "/safe/source.txt",
                    "destination": "/safe/destination.txt",
                    "status": status,
                })
            );
        }
    }

    #[test]
    fn filesystem_move_rejects_overwrite_relative_and_same_paths() {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: ActiveGatewayFailureKind::Protocol,
                message: "unexpected Gateway call".to_owned(),
            }),
        };
        for input in [
            json!({"source": "/safe/a.txt", "destination": "/safe/b.txt", "overwrite": true}),
            json!({"source": "relative/a.txt", "destination": "/safe/b.txt"}),
            json!({"source": "/safe/a.txt", "destination": "relative/b.txt"}),
            json!({"source": "/safe/a.txt", "destination": "/safe/a.txt"}),
        ] {
            let error =
                execute_with_invoker(&invoker, &request("filesystem.move", input), &mut |_| {})
                    .unwrap_err();
            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
        }
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn maps_action_and_input_without_transmitting_execution_id() {
        let input = json!({"path": "/safe/example"});
        let request = request("sessions.create", input.clone());
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({"files": 3})),
        };

        execute_with_invoker(&invoker, &request, &mut |_| {}).unwrap();

        assert_eq!(
            *invoker.calls.lock().unwrap(),
            vec![("sessions.create".to_owned(), Some(input.clone()))]
        );
        assert_eq!(request.input, input);
        assert!(
            !format!("{:?}", invoker.calls.lock().unwrap()).contains(request.execution_id.as_str())
        );
    }

    #[test]
    fn maps_successful_payload_to_execution_result() {
        let payload = json!({"files": ["a", "b"]});
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(payload.clone()),
        };

        let result = execute_with_invoker(
            &invoker,
            &request("sessions.create", json!({})),
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(result.output, payload);
        assert_eq!(result.summary, None);
    }

    #[test]
    fn production_adapter_emits_no_progress() {
        struct NoNetworkInvoker;

        impl GatewayMethodInvoker for NoNetworkInvoker {
            fn invoke(
                &self,
                _method: &str,
                _params: Option<Value>,
            ) -> Result<Value, ActiveGatewayMethodFailure> {
                Ok(json!({"ok": true}))
            }
        }

        let request = request("sessions.create", json!({}));
        let mut progress = Vec::new();
        let result = execute_with_invoker(&NoNetworkInvoker, &request, &mut |update| {
            progress.push(update)
        });

        assert!(result.is_ok());
        assert!(progress.is_empty());
    }

    fn assert_failure_mapping(
        gateway_kind: ActiveGatewayFailureKind,
        expected_kind: OpenClawExecutionErrorKind,
        expected_retryable: bool,
    ) {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: gateway_kind,
                message: "Safe Gateway failure.".to_owned(),
            }),
        };

        let error = execute_with_invoker(
            &invoker,
            &request("sessions.create", json!({})),
            &mut |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind, expected_kind);
        assert_eq!(error.retryable, expected_retryable);
        assert!(error.message.ends_with("Safe Gateway failure."));
    }

    #[test]
    fn maps_unauthorized_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unauthorized,
            OpenClawExecutionErrorKind::AuthenticationRequired,
            false,
        );
    }

    #[test]
    fn maps_pairing_required_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::PairingRequired,
            OpenClawExecutionErrorKind::PairingRequired,
            false,
        );
    }

    #[test]
    fn maps_unreachable_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unreachable,
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            true,
        );
    }

    #[test]
    fn maps_protocol_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Protocol,
            OpenClawExecutionErrorKind::ProtocolFailure,
            false,
        );
    }

    #[test]
    fn maps_no_active_server_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::NoActiveServer,
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            false,
        );
    }

    #[test]
    fn maps_unknown_failure() {
        assert_failure_mapping(
            ActiveGatewayFailureKind::Unknown,
            OpenClawExecutionErrorKind::ExecutionFailed,
            false,
        );
    }

    #[test]
    fn runtime_error_does_not_include_unrelated_request_credential() {
        let credential = "request-credential-secret";
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Err(ActiveGatewayMethodFailure {
                kind: ActiveGatewayFailureKind::Protocol,
                message: "Gateway protocol failed.".to_owned(),
            }),
        };

        let error = execute_with_invoker(
            &invoker,
            &request("sessions.create", json!({"credential": credential})),
            &mut |_| {},
        )
        .unwrap_err();

        assert!(!error.to_string().contains(credential));
        assert!(!format!("{error:?}").contains(credential));
    }

    #[test]
    fn download_agent_fallback_is_limited_to_retryable_file_verification_failure() {
        let file_verification_failure = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ProtocolFailure,
            "OpenClaw completed without creating a file in the selected destination.",
            true,
        );
        assert!(should_retry_download_with_next_agent(
            &file_verification_failure
        ));

        let protocol_failure_not_marked_retryable = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ProtocolFailure,
            "Malformed OpenClaw response.",
            false,
        );
        assert!(!should_retry_download_with_next_agent(
            &protocol_failure_not_marked_retryable
        ));

        let connection_failure = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ConnectionUnavailable,
            "OpenClaw gateway unavailable.",
            true,
        );
        assert!(!should_retry_download_with_next_agent(&connection_failure));

        let invalid_request = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "Invalid download request.",
            false,
        );
        assert!(!should_retry_download_with_next_agent(&invalid_request));

        let permission_denied = OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::PermissionDenied,
            "Permission denied.",
            false,
        );
        assert!(!should_retry_download_with_next_agent(&permission_denied));
    }

    #[test]
    fn download_verification_finds_files_created_in_nested_directories() {
        let destination = tempfile::tempdir().unwrap();
        let nested = destination.path().join("downloaded-bundle");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("file.bin"), b"complete").unwrap();
        fs::write(destination.path().join("empty.txt"), b"").unwrap();
        fs::write(
            destination.path().join("login-page"),
            b"<!DOCTYPE html><html><title>Login</title></html>",
        )
        .unwrap();

        assert_eq!(
            download_directory_files(destination.path()).unwrap(),
            HashSet::from(["downloaded-bundle/file.bin".to_owned()])
        );
    }

    #[test]
    fn thunder_wrapper_is_decoded_and_requires_a_downloaded_file() {
        let destination = tempfile::tempdir().unwrap();
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": ""}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "download-run"})),
                Ok(json!({"status": "ok"})),
                Ok(history),
            ])),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "thunder://QUFodHRwczovL2V4YW1wbGUuY29tL2ZpbGUuemlwWlo=",
                    "destination": destination.path(),
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();
        let calls = invoker.calls.lock().unwrap();

        assert!(error.message.contains("without creating a file"));
        assert_eq!(
            calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
            vec!["agent", "agent.wait"]
        );
        assert!(calls[0].1.as_ref().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("https://example.com/file.zip"));
        assert!(!calls[0].1.as_ref().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("Thunder.app"));
    }

    #[test]
    fn cloud_share_delegates_provider_choice_to_installed_skills() {
        let destination = tempfile::tempdir().unwrap();
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": ""}]
        }]});
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"commands": [{
                    "source": "skill",
                    "textAliases": ["/baidu_drive"]
                }]})),
                Ok(json!({"runId": "cloud-share-run"})),
                Ok(json!({"status": "ok"})),
                Ok(history),
            ])),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "https://cloud.example/share/safe-test",
                    "destination": destination.path(),
                    "extractionCode": "a1b2",
                    "selectionHint": "download the approximately 10 MB DMG",
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();
        let calls = invoker.calls.lock().unwrap();

        assert!(error.message.contains("without creating a file"));
        assert_eq!(calls[0].0, "commands.list");
        let message = calls[1].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message.starts_with("/baidu_drive "));
        let task_message = message.split_once(' ').unwrap().1;
        assert!(message.contains("a1b2"));
        assert!(message.contains("approximately 10 MB DMG"));
        assert!(message.contains("Never download the entire share"));
        assert!(message.contains("very next tool call MUST execute"));
        assert!(message.contains("ai-os-runtime-execution-123"));
        assert!(message.contains("Do not read a browser Skill"));
        // Provider neutrality is the property under test: AI-OS must not name or
        // imply a specific cloud provider. Assert on that, not on prompt wording —
        // a reworded prompt is not a regression.
        for provider in [
            "baidu",
            "pan.baidu",
            "aliyun",
            "aliyundrive",
            "quark",
            "115",
            "pikpak",
            "onedrive",
            "dropbox",
            "gdrive",
            "google drive",
        ] {
            assert!(
                !task_message.to_lowercase().contains(provider),
                "prompt must stay provider-neutral but named {provider}"
            );
        }
        assert!(
            message.contains("Skill"),
            "prompt must mention Skills at all"
        );
        assert!(
            message.find("inspect the OpenClaw Skills")
                < message.find("call exec with /usr/bin/curl"),
            "cloud-share execution must try an installed Skill before curl"
        );
        assert!(message.contains("direct share-link download command"));
        assert!(message.contains("do not use curl or browser tools"));
        assert!(message.contains("$HOME/.agents/skills"));
        assert!(message.contains("never quote the command name and subcommand together"));
        assert!(message.contains("Do not repeat the same failed command"));
        assert_eq!(
            calls[1].1.as_ref().unwrap()["agentId"],
            "ai-os-exec-standard"
        );
    }

    #[test]
    fn interactive_web_download_reuses_authenticated_session_without_user_clicks() {
        let destination = tempfile::tempdir().unwrap();
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"commands": []})),
                Ok(json!({"runId": "web-download-run"})),
                Ok(json!({"status": "ok"})),
            ])),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "https://files.example/share/test-file",
                    "destination": destination.path(),
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();
        let calls = invoker.calls.lock().unwrap();
        let params = calls[1].1.as_ref().unwrap();
        let message = params["message"].as_str().unwrap();

        assert!(error.message.contains("without creating a file"));
        assert_eq!(params["agentId"], "ai-os-exec-standard");
        assert_eq!(params["timeout"], 14_400);
        assert!(message.contains("persistent browser profile"));
        assert!(message.contains("authenticated or VIP session"));
        assert!(message.contains("JavaScript, buttons, waits"));
        assert!(message.contains("hidden forms"));
        assert!(message.contains("without asking the user to click"));
        assert!(message.contains("authentication-required error"));
        assert!(message.contains("complete file"));
    }

    #[test]
    fn resource_query_is_content_neutral_and_requires_a_file() {
        let destination = tempfile::tempdir().unwrap();
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "search-run"})),
                Ok(json!({"status": "ok"})),
            ])),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "Ubuntu 24.04 desktop ISO",
                    "destination": destination.path(),
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();
        let calls = invoker.calls.lock().unwrap();

        assert!(error.message.contains("without creating a file"));
        assert_eq!(
            calls[0].1.as_ref().unwrap()["agentId"],
            "ai-os-exec-standard"
        );
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();
        assert!(message
            .contains("without performing content, copyright, filename, or NSFW classification"));
        assert!(message.contains("Ubuntu 24.04 desktop ISO"));
    }

    #[test]
    fn download_rejects_invalid_destination_before_openclaw_execution() {
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({})),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "https://example.com/file.zip",
                    "destination": "relative/path",
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();

        assert!(error.message.contains("existing absolute directory"));
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn download_rejects_unsupported_source_before_openclaw_execution() {
        let destination = tempfile::tempdir().unwrap();
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({})),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "thunder://invalid-wrapper",
                    "destination": destination.path(),
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();

        assert!(error.message.ends_with("Download source is unsupported."));
        assert!(invoker.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn p2p_download_prefers_noninteractive_thunder_and_never_accepts_gui_submission() {
        let destination = tempfile::tempdir().unwrap();
        let invoker = ScriptedInvoker {
            calls: Mutex::new(Vec::new()),
            outcomes: Mutex::new(VecDeque::from(vec![
                Ok(json!({"runId": "p2p-run"})),
                Ok(json!({"status": "ok"})),
            ])),
        };

        let error = execute_with_invoker(
            &invoker,
            &request(
                "download.start",
                json!({
                    "source": "ed2k://example",
                    "destination": destination.path(),
                }),
            ),
            &mut |_| {},
        )
        .unwrap_err();
        let calls = invoker.calls.lock().unwrap();
        let message = calls[0].1.as_ref().unwrap()["message"].as_str().unwrap();

        assert!(error.message.contains("without creating a file"));
        assert!(message.contains("non-interactive Thunder Skill, CLI, or local API"));
        assert!(message.contains("Never open the Thunder GUI"));
        assert!(message.contains("ED2K must fail with the real unsupported-tool reason"));
        assert!(message.contains("do not claim success for merely submitting a task"));
    }
}
