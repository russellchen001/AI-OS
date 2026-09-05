use super::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
    OpenClawExecutionProgress, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use crate::document::provider::OfficeProviderId;
use crate::document::registry::resolve_office_provider;
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
const SPREADSHEET_EDIT_ACTION: &str = "spreadsheet.edit";
const PRESENTATION_READ_ACTION: &str = "presentation.read";
const PRESENTATION_CREATE_ACTION: &str = "presentation.create";
/// Three adapters were implemented, verified by their own gate steps, and
/// declared by no capability at all, so nothing could call them. Word's edit
/// produces an edited copy with tables, images and heading formatting;
/// PowerPoint's edits slides and exports PDF. They are exposed here.
const DOCUMENT_EDIT_ACTION: &str = "document.edit";
const SPREADSHEET_CONVERT_ACTION: &str = "spreadsheet.convert";
/// Rearranging pages is a PDF operation, and PDF is the one format no Office or
/// iWork application here reads. macOS reads and writes it without either.
const DOCUMENT_MERGE_ACTION: &str = "document.merge";
const DOCUMENT_SPLIT_ACTION: &str = "document.split";
/// Turning a page, locking a file and unlocking it again are the same kind of
/// work as merging and splitting: they are done to the PDF itself, by the same
/// reader, on any machine.
const DOCUMENT_ROTATE_ACTION: &str = "document.rotate";
const DOCUMENT_ENCRYPT_ACTION: &str = "document.encrypt";
const DOCUMENT_DECRYPT_ACTION: &str = "document.decrypt";
/// Marks on a document, and values in its form, are part of the document too.
const DOCUMENT_ANNOTATE_ACTION: &str = "document.annotate";
const DOCUMENT_FILL_ACTION: &str = "document.fill";
/// Changing what a page says, and what it no longer says. Same layer again:
/// the file itself, read and written in Rust, on any machine.
const DOCUMENT_REDACT_ACTION: &str = "document.redact";
const DOCUMENT_STAMP_ACTION: &str = "document.stamp";
const DOCUMENT_REPLACE_ACTION: &str = "document.replace";
const PRESENTATION_EDIT_ACTION: &str = "presentation.edit";
const PRESENTATION_CONVERT_ACTION: &str = "presentation.convert";
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
const MAX_SPREADSHEET_READ_SHEETS: usize = 16;
const MAX_SPREADSHEET_READ_ROWS: usize = 200;
const MAX_SPREADSHEET_READ_COLUMNS: usize = 64;
const MAX_SPREADSHEET_READ_CELL_CHARS: usize = 256;
const MAX_SPREADSHEET_PROTOCOL_CHARS: usize = 60_000;
/// Formulas are reported sparsely -- only cells whose text begins with
/// `=` -- so this caps a pathological sheet rather than a normal one.
const MAX_SPREADSHEET_READ_FORMULAS: usize = 512;

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
    if request.action.as_str() == SPREADSHEET_EDIT_ACTION {
        return execute_spreadsheet_edit(request);
    }

    if request.action.as_str() == PRESENTATION_READ_ACTION {
        return execute_presentation_read(invoker, request);
    }
    if request.action.as_str() == PRESENTATION_CREATE_ACTION {
        return execute_presentation_create(invoker, request);
    }
    if request.action.as_str() == DOCUMENT_EDIT_ACTION {
        return execute_document_edit(request);
    }
    if request.action.as_str() == PRESENTATION_EDIT_ACTION {
        return execute_presentation_edit(request);
    }
    if request.action.as_str() == PRESENTATION_CONVERT_ACTION {
        return execute_presentation_convert(request);
    }
    if request.action.as_str() == SPREADSHEET_CONVERT_ACTION {
        return execute_spreadsheet_convert(request);
    }
    if request.action.as_str() == DOCUMENT_MERGE_ACTION {
        return execute_document_merge(request);
    }
    if request.action.as_str() == DOCUMENT_SPLIT_ACTION {
        return execute_document_split(request);
    }
    if request.action.as_str() == DOCUMENT_ROTATE_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_ROTATE_ACTION,
            "source",
            crate::document::pdf::rotate_pdf_document,
            "AI-OS turned the pages.",
        );
    }
    if request.action.as_str() == DOCUMENT_ENCRYPT_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_ENCRYPT_ACTION,
            "source",
            crate::document::pdf::encrypt_pdf_document,
            "AI-OS locked the PDF with a password.",
        );
    }
    if request.action.as_str() == DOCUMENT_DECRYPT_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_DECRYPT_ACTION,
            "source",
            crate::document::pdf::decrypt_pdf_document,
            "AI-OS removed the password from the PDF.",
        );
    }
    if request.action.as_str() == DOCUMENT_ANNOTATE_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_ANNOTATE_ACTION,
            "source",
            crate::document::pdf::annotate_pdf_document,
            "AI-OS left the notes on the PDF.",
        );
    }
    if request.action.as_str() == DOCUMENT_FILL_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_FILL_ACTION,
            "source",
            crate::document::pdf::fill_pdf_form,
            "AI-OS filled in the form.",
        );
    }
    if request.action.as_str() == DOCUMENT_REDACT_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_REDACT_ACTION,
            "source",
            crate::document::pdf::redact_pdf_document,
            "AI-OS took the words out of the PDF.",
        );
    }
    if request.action.as_str() == DOCUMENT_STAMP_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_STAMP_ACTION,
            "source",
            crate::document::pdf::stamp_pdf_document,
            "AI-OS stamped the PDF.",
        );
    }
    if request.action.as_str() == DOCUMENT_REPLACE_ACTION {
        return execute_local_pdf(
            request,
            DOCUMENT_REPLACE_ACTION,
            "source",
            crate::document::pdf::replace_in_pdf_document,
            "AI-OS changed the words in the PDF.",
        );
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
    use crate::document::resolver::OfficeApplication;

    let route = office_route(DOCUMENT_CREATE_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(DOCUMENT_CREATE_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleDocs => {
            let output =
                run_cloud(crate::google_workspace::office::document_create(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Google document.".to_owned()),
            });
        }
        OfficeApplication::ApplePages => {
            let output = crate::document::pages::create_pages_document(&request.input)
                .map_err(map_pages_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Pages document.".to_owned()),
            });
        }
        // Word writes a real Word document, so it answers for its own formats
        // whenever it is installed -- the same rule document.read follows, and
        // the reason this route stopped handing every .docx to plain-text
        // conversion.
        //
        // Both of them. Word used to save DOCX bytes whatever the path was
        // called, so a .doc came out mislabelled and this arm had to exclude
        // it; the adapter now picks its format from the extension and writes
        // genuine OLE2 for a .doc, so there is nothing left to exclude.
        OfficeApplication::MicrosoftWord => {
            let output = crate::document::word::create_word_document(&request.input)
                .map_err(map_word_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Word document.".to_owned()),
            });
        }
        OfficeApplication::MacosNative => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no document.create adapter."),
                false,
            ));
        }
    }

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
    use crate::document::resolver::OfficeApplication;

    let route = route_with_destination(
        DOCUMENT_CONVERT_ACTION,
        &request.input,
        "source",
        Some("destination"),
    )
    .ok_or_else(|| no_conversion_route(DOCUMENT_CONVERT_ACTION, &request.input))?;

    match route.application {
        // Pages is the only application on the machine that can write a
        // .pages, so it owns every conversion into iWork's own format -- and it
        // exports Word format and PDF out of one.
        OfficeApplication::ApplePages => {
            let output = crate::document::iwork_convert::convert_with_iwork(
                crate::document::iwork_convert::IworkApplication::Pages,
                &request.input,
            )
            .map_err(map_iwork_convert_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS converted the document with Pages.".to_owned()),
            });
        }
        // Word converts in every direction it declares: to PDF, the old .doc to
        // .docx, and -- the one thing on this machine that can do it -- a PDF
        // back into an editable document.
        OfficeApplication::MicrosoftWord => {
            let output = crate::document::word::convert_word_document(&request.input)
                .map_err(map_word_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS converted the document with Word.".to_owned()),
            });
        }
        // Conversion between the Microsoft word-processing formats, which needs
        // no application at all.
        OfficeApplication::MacosNative => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no document.convert adapter."),
                false,
            ));
        }
    }

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

fn map_powerpoint_error(
    error: crate::document::powerpoint::PowerPointError,
) -> OpenClawExecutionError {
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

fn map_structured_error(
    error: crate::document::structured::StructuredError,
) -> OpenClawExecutionError {
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

/// Whether the Microsoft Office provider is installed and therefore preferred.
fn map_pages_error(error: crate::document::pages::PagesError) -> OpenClawExecutionError {
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

fn map_numbers_error(error: crate::document::numbers::NumbersError) -> OpenClawExecutionError {
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

/// The lowercased extension of a path field, if there is one.
///
/// The provider registry is ordered by priority, so Microsoft Office wins every
/// capability it declares for as long as it is installed -- and a `.pages` file
/// would then be handed to an adapter that cannot open it. A format-aware
/// resolver already exists in `document::resolver`, but nothing calls it yet;
/// until it is wired in, the two iWork formats are dispatched by their own
/// extension, which is the narrow and testable part of the same idea.
fn request_format(input: &Value, field: &str) -> Option<String> {
    Path::new(input.get(field)?.as_str()?.trim())
        .extension()?
        .to_str()
        .map(str::to_ascii_lowercase)
}

fn is_format(input: &Value, field: &str, expected: &str) -> bool {
    request_format(input, field).as_deref() == Some(expected)
}

fn map_iwork_convert_error(
    error: crate::document::iwork_convert::IworkConvertError,
) -> OpenClawExecutionError {
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

fn map_pdf_error(error: crate::document::pdf::PdfError) -> OpenClawExecutionError {
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

fn map_excel_error(error: crate::document::excel::ExcelError) -> OpenClawExecutionError {
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

fn map_word_error(error: crate::document::word::WordError) -> OpenClawExecutionError {
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

/// Which adapter answers this request.
///
/// Routing used to be three different mechanisms: a chain of extension checks
/// at the top of each entry point, a priority-ordered registry lookup below it,
/// and an `is Office installed` test in between. They disagreed -- the same
/// .docx returned Word's shape or plain text depending on an application
/// neither path used -- and a complete format-aware resolver sat in
/// `document::resolver` that nothing called. This is that resolver, called.
fn office_route(
    capability: &str,
    input: &Value,
    field: &str,
) -> Option<crate::document::resolver::OfficeRoute> {
    route_with_destination(capability, input, field, None)
}

/// Conversion is the one capability that cannot be routed on the source alone.
///
/// The owner settled what convert means: the DESTINATION decides. `.pdf` is an
/// export and anything else is a cross-suite conversion, so `.docx` to `.pdf`
/// is Word's and `.docx` to `.pages` is Pages' -- because Word cannot write a
/// `.pages` at all, and macOS conversion cannot write a PDF.
fn route_with_destination(
    capability: &str,
    input: &Value,
    field: &str,
    destination_field: Option<&str>,
) -> Option<crate::document::resolver::OfficeRoute> {
    let format = request_format(input, field);
    let destination = destination_field.and_then(|field| request_format(input, field));

    crate::document::resolver::resolve_office_route(
        &crate::document::resolver::OfficeRouteRequest {
            capability,
            location: requested_location(input),
            format: format.as_deref(),
            destination_format: destination.as_deref(),
            preferred_application: None,
        },
        &crate::document::resolver::office_candidates(),
    )
}

/// Whether this request is about a file on this machine or a cloud resource.
///
/// A Drive file id is not a path, so the request has to say which it means.
/// It is a cloud request when it carries `resource.fileId`, or when it says
/// `provider: "google-workspace"` -- which is the only way a CREATE can say so,
/// because a file that does not exist yet has no id.
///
/// Everything else is local, so no existing caller changes behaviour.
fn requested_location(input: &Value) -> crate::document::resolver::OfficeResourceLocation {
    use crate::document::resolver::OfficeResourceLocation;

    let named_provider = input
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|provider| provider.eq_ignore_ascii_case("google-workspace"));

    let carries_file_id = input
        .pointer("/resource/fileId")
        .or_else(|| input.get("fileId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if named_provider || carries_file_id {
        OfficeResourceLocation::GoogleCloud
    } else {
        OfficeResourceLocation::Local
    }
}

/// A cloud adapter, run from the synchronous gateway.
///
/// The Office capabilities are synchronous and the Google adapters are async.
/// `tauri::async_runtime::block_on` is the bridge `download/registry.rs`
/// already uses for the same reason.
fn run_cloud<F>(future: F) -> Result<Value, OpenClawExecutionError>
where
    F: std::future::Future<Output = Result<Value, String>>,
{
    tauri::async_runtime::block_on(future).map_err(|message| {
        // A Google failure is a request problem when it is about what was
        // asked for, and an execution problem when it is about reaching the
        // service. The messages come from one place, so they can be told apart.
        let invalid = message.contains("requires") || message.contains("was not found");

        OpenClawExecutionError::new(
            if invalid {
                OpenClawExecutionErrorKind::InvalidRequest
            } else {
                OpenClawExecutionErrorKind::ExecutionFailed
            },
            message,
            false,
        )
    })
}

/// The error a caller gets when no installed provider can answer for this file.
///
/// It names the format, because "no provider supports document.read" is not
/// actionable when the real answer is that this machine has nothing that reads
/// a .pages.
fn no_route(capability: &str, input: &Value, field: &str) -> OpenClawExecutionError {
    let format = request_format(input, field);

    OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        match format {
            Some(format) => format!(
                "No available Office Provider supports {capability} for a .{format} file."
            ),
            None => format!("No available Office Provider supports {capability}."),
        },
        false,
    )
}

/// The same, naming what was asked for AND what it was asked to become.
///
/// "No provider supports document.convert" is not actionable when the real
/// answer is that this machine has nothing that turns a .pages into a .docx.
fn no_conversion_route(capability: &str, input: &Value) -> OpenClawExecutionError {
    let from = request_format(input, "source");
    let to = request_format(input, "destination");

    OpenClawExecutionError::new(
        OpenClawExecutionErrorKind::ExecutionFailed,
        match (from, to) {
            (Some(from), Some(to)) => format!(
                "No available Office Provider can convert a .{from} to a .{to} on this machine."
            ),
            _ => format!("No available Office Provider supports {capability}."),
        },
        false,
    )
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
    use crate::document::resolver::OfficeApplication;

    let route = office_route(PRESENTATION_READ_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(PRESENTATION_READ_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleSlides => {
            let output = run_cloud(crate::google_workspace::office::presentation_read(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS read the Google presentation.".to_owned()),
            });
        }
        // PowerPoint reads its own format best; the structured layer reads the
        // same .pptx when PowerPoint is absent, and a .ppt is the old binary
        // format that only PowerPoint reads.
        OfficeApplication::MicrosoftPowerPoint => {
            let output = crate::document::powerpoint::read_powerpoint_presentation(&request.input)
                .map_err(map_powerpoint_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS completed the PowerPoint presentation read.".to_owned()),
            });
        }
        OfficeApplication::StructuredFile => {
            let output = crate::document::structured::read_structured_presentation(&request.input)
                .map_err(map_structured_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some(
                    "AI-OS read the presentation directly from the file, without PowerPoint."
                        .to_owned(),
                ),
            });
        }
        OfficeApplication::AppleKeynote => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no presentation.read adapter."),
                false,
            ));
        }
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
    use crate::document::resolver::OfficeApplication;

    let route = office_route(PRESENTATION_CREATE_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(PRESENTATION_CREATE_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleSlides => {
            let output = run_cloud(crate::google_workspace::office::presentation_create(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Google presentation.".to_owned()),
            });
        }
        OfficeApplication::MicrosoftPowerPoint => {
            let output =
                crate::document::powerpoint::create_powerpoint_presentation(&request.input)
                    .map_err(map_powerpoint_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the PowerPoint presentation.".to_owned()),
            });
        }
        OfficeApplication::AppleKeynote => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no presentation.create adapter."),
                false,
            ));
        }
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
    use crate::document::resolver::OfficeApplication;

    let route = office_route(SPREADSHEET_CREATE_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(SPREADSHEET_CREATE_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleSheets => {
            let output = run_cloud(crate::google_workspace::office::spreadsheet_create(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Google spreadsheet.".to_owned()),
            });
        }
        OfficeApplication::AppleNumbers => {
            let output = crate::document::numbers::create_numbers_spreadsheet(&request.input)
                .map_err(map_numbers_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS created the Numbers spreadsheet.".to_owned()),
            });
        }
        // Same floor as the read side: Excel writes its own format with the
        // highest fidelity, but its absence must not remove the capability. The
        // workbook the structured layer writes is proven to open in Excel.
        OfficeApplication::StructuredFile => {
            let output = crate::document::structured::create_structured_spreadsheet(&request.input)
                .map_err(map_structured_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some(
                    "AI-OS wrote the spreadsheet directly to the file, without Excel.".to_owned(),
                ),
            });
        }
        OfficeApplication::MicrosoftExcel => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no spreadsheet.create adapter."),
                false,
            ));
        }
    }

    let (path, session_key, run_id) = start_spreadsheet_create(invoker, request)?;
    finish_spreadsheet_create(invoker, &path, &session_key, &run_id)
}

fn execute_spreadsheet_edit(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    // The one Office capability with a single adapter: editing a workbook in
    // place needs a spreadsheet application, and the structured layer can only
    // read and create. It says so rather than pretending.
    let route = office_route(SPREADSHEET_EDIT_ACTION, &request.input, "source")
        .ok_or_else(|| no_route(SPREADSHEET_EDIT_ACTION, &request.input, "source"))?;

    // Google's edit is a values write that validates itself by reading back,
    // which is the same contract the Excel adapter holds itself to.
    if route.application == OfficeApplication::GoogleSheets {
        let output = run_cloud(crate::google_workspace::office::spreadsheet_edit(&request.input))?;

        return Ok(OpenClawExecutionResult {
            output,
            summary: Some("AI-OS wrote and validated the Google spreadsheet.".to_owned()),
        });
    }

    if route.application != OfficeApplication::MicrosoftExcel {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no spreadsheet.edit adapter.",
                route.application
            ),
            false,
        ));
    }

    let output = crate::document::excel::edit_excel_workbook(&request.input).map_err(|error| {
        OpenClawExecutionError::new(
            if error.invalid_request {
                OpenClawExecutionErrorKind::InvalidRequest
            } else {
                OpenClawExecutionErrorKind::ExecutionFailed
            },
            error.message,
            false,
        )
    })?;
    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS completed and validated the Excel spreadsheet edit copy.".to_owned()),
    })
}

/// Editing a document in place is not what any of these adapters do.
///
/// Every one of them reads a source and writes a separate destination, leaving
/// the source byte-identical -- which is what makes an edit safe to offer at
/// all. The capability is named `edit` because that is what the caller wants;
/// the contract is a copy.
fn execute_document_edit(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = office_route(DOCUMENT_EDIT_ACTION, &request.input, "source")
        .ok_or_else(|| no_route(DOCUMENT_EDIT_ACTION, &request.input, "source"))?;

    if route.application != OfficeApplication::MicrosoftWord {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no document.edit adapter.",
                route.application
            ),
            false,
        ));
    }

    let output =
        crate::document::word::edit_word_document(&request.input).map_err(map_word_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS completed and validated the Word document edit copy.".to_owned()),
    })
}

fn execute_presentation_edit(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = office_route(PRESENTATION_EDIT_ACTION, &request.input, "source")
        .ok_or_else(|| no_route(PRESENTATION_EDIT_ACTION, &request.input, "source"))?;

    if route.application != OfficeApplication::MicrosoftPowerPoint {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no presentation.edit adapter.",
                route.application
            ),
            false,
        ));
    }

    let output = crate::document::powerpoint::edit_powerpoint_presentation(&request.input)
        .map_err(map_powerpoint_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some(
            "AI-OS completed and validated the PowerPoint presentation edit copy.".to_owned(),
        ),
    })
}

/// `presentation.convert` means one thing and says so: export to PDF.
///
/// This is deliberately unlike `document.convert`, which means DOC/DOCX
/// conversion through macOS and PDF export through Pages, and which is why that
/// capability is still not routed through the resolver.
fn execute_presentation_convert(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = route_with_destination(
        PRESENTATION_CONVERT_ACTION,
        &request.input,
        "source",
        Some("destination"),
    )
    .ok_or_else(|| no_conversion_route(PRESENTATION_CONVERT_ACTION, &request.input))?;

    match route.application {
        OfficeApplication::MicrosoftPowerPoint => {
            let output = crate::document::powerpoint::export_powerpoint_pdf(&request.input)
                .map_err(map_powerpoint_error)?;

            Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS exported the presentation to PDF.".to_owned()),
            })
        }
        // Keynote owns .key in both directions: nothing else writes one, and
        // nothing else reads one.
        OfficeApplication::AppleKeynote => {
            let output = crate::document::iwork_convert::convert_with_iwork(
                crate::document::iwork_convert::IworkApplication::Keynote,
                &request.input,
            )
            .map_err(map_iwork_convert_error)?;

            Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS converted the presentation with Keynote.".to_owned()),
            })
        }
        other => Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!("Office application {other:?} has no presentation.convert adapter."),
            false,
        )),
    }
}

/// Run one of the PDF-only capabilities.
///
/// Every one of them resolves the same way and fails the same way, so the
/// routing lives here once instead of being copied per capability -- which is
/// how a capability ends up declared and unreachable.
fn execute_local_pdf(
    request: &OpenClawExecutionRequest,
    action: &str,
    routed_on: &str,
    run: fn(&serde_json::Value) -> Result<serde_json::Value, crate::document::pdf::PdfError>,
    summary: &str,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = office_route(action, &request.input, routed_on)
        .ok_or_else(|| no_route(action, &request.input, routed_on))?;

    if route.application != OfficeApplication::LocalPdf {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no {action} adapter.",
                route.application
            ),
            false,
        ));
    }

    let output = run(&request.input).map_err(map_pdf_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some(summary.to_owned()),
    })
}

fn execute_document_merge(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    // Routed on the destination, because a merge has many sources and one
    // answer, and the format they must all be is the one it writes.
    let route = office_route(DOCUMENT_MERGE_ACTION, &request.input, "destination")
        .ok_or_else(|| no_route(DOCUMENT_MERGE_ACTION, &request.input, "destination"))?;

    if route.application != OfficeApplication::LocalPdf {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no document.merge adapter.",
                route.application
            ),
            false,
        ));
    }

    let output =
        crate::document::pdf::merge_pdf_documents(&request.input).map_err(map_pdf_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS merged the PDFs.".to_owned()),
    })
}

fn execute_document_split(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = office_route(DOCUMENT_SPLIT_ACTION, &request.input, "source")
        .ok_or_else(|| no_route(DOCUMENT_SPLIT_ACTION, &request.input, "source"))?;

    if route.application != OfficeApplication::LocalPdf {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!(
                "Office application {:?} has no document.split adapter.",
                route.application
            ),
            false,
        ));
    }

    let output =
        crate::document::pdf::split_pdf_document(&request.input).map_err(map_pdf_error)?;

    Ok(OpenClawExecutionResult {
        output,
        summary: Some("AI-OS extracted the pages into a new PDF.".to_owned()),
    })
}

fn execute_spreadsheet_convert(
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = route_with_destination(
        SPREADSHEET_CONVERT_ACTION,
        &request.input,
        "source",
        Some("destination"),
    )
    .ok_or_else(|| no_conversion_route(SPREADSHEET_CONVERT_ACTION, &request.input))?;

    match route.application {
        OfficeApplication::MicrosoftExcel => {
            let output = crate::document::excel::export_excel_pdf(&request.input)
                .map_err(map_excel_error)?;

            Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS exported the spreadsheet to PDF with Excel.".to_owned()),
            })
        }
        OfficeApplication::AppleNumbers => {
            let output = crate::document::iwork_convert::convert_with_iwork(
                crate::document::iwork_convert::IworkApplication::Numbers,
                &request.input,
            )
            .map_err(map_iwork_convert_error)?;

            Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS converted the spreadsheet with Numbers.".to_owned()),
            })
        }
        other => Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::ExecutionFailed,
            format!("Office application {other:?} has no spreadsheet.convert adapter."),
            false,
        )),
    }
}

fn execute_spreadsheet_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<OpenClawExecutionResult, OpenClawExecutionError> {
    use crate::document::resolver::OfficeApplication;

    let route = office_route(SPREADSHEET_READ_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(SPREADSHEET_READ_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleSheets => {
            let output = run_cloud(crate::google_workspace::office::spreadsheet_read(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS read the Google spreadsheet.".to_owned()),
            });
        }
        OfficeApplication::AppleNumbers => {
            let output = crate::document::numbers::read_numbers_spreadsheet(&request.input)
                .map_err(map_numbers_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS completed the Numbers spreadsheet read.".to_owned()),
            });
        }
        // Excel reads its own format with the highest fidelity -- it is the only
        // path that resolves formulas -- so it is preferred whenever it is
        // installed. When it is not, the capability does not disappear: .xlsx is
        // a ZIP of XML and the structured layer reads the file directly. This is
        // the difference between an Office capability and a set of
        // per-application integrations.
        OfficeApplication::StructuredFile => {
            let output = crate::document::structured::read_structured_spreadsheet(&request.input)
                .map_err(map_structured_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some(
                    "AI-OS read the spreadsheet directly from the file, without Excel.".to_owned(),
                ),
            });
        }
        OfficeApplication::MicrosoftExcel => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no spreadsheet.read adapter."),
                false,
            ));
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum SpreadsheetReadSelection {
    First,
    Named(String),
    Selected(Vec<String>),
    All,
}

fn validate_spreadsheet_sheet_name(value: &str) -> Result<String, OpenClawExecutionError> {
    let value = value.trim();

    if value.is_empty()
        || value.chars().count() > 31
        || value.chars().any(char::is_control)
        || value
            .chars()
            .any(|character| "[]:*?/\\\\".contains(character))
    {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.read requires valid worksheet names",
            false,
        ));
    }

    Ok(value.to_owned())
}

fn spreadsheet_read_selection(
    request: &OpenClawExecutionRequest,
) -> Result<SpreadsheetReadSelection, OpenClawExecutionError> {
    let sheet_value = request.input.get("sheet");
    let sheets_value = request.input.get("sheets");

    let all_sheets = match request.input.get("allSheets") {
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.read allSheets must be boolean",
                false,
            ))
        }
        None => false,
    };

    let active_selectors = usize::from(sheet_value.is_some())
        + usize::from(sheets_value.is_some())
        + usize::from(all_sheets);

    if active_selectors > 1 {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.read accepts only one of sheet, sheets, or allSheets",
            false,
        ));
    }

    if let Some(value) = sheet_value {
        let sheet = value.as_str().ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.read sheet must be a worksheet name",
                false,
            )
        })?;

        return Ok(SpreadsheetReadSelection::Named(
            validate_spreadsheet_sheet_name(sheet)?,
        ));
    }

    if let Some(value) = sheets_value {
        let values = value.as_array().ok_or_else(|| {
            OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                "spreadsheet.read sheets must be an array of worksheet names",
                false,
            )
        })?;

        if values.is_empty() || values.len() > MAX_SPREADSHEET_READ_SHEETS {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::InvalidRequest,
                format!(
                    "spreadsheet.read sheets must contain between 1 and {} names",
                    MAX_SPREADSHEET_READ_SHEETS
                ),
                false,
            ));
        }

        let mut seen = HashSet::new();
        let mut names = Vec::with_capacity(values.len());

        for value in values {
            let raw = value.as_str().ok_or_else(|| {
                OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::InvalidRequest,
                    "spreadsheet.read sheets must contain only worksheet names",
                    false,
                )
            })?;

            let name = validate_spreadsheet_sheet_name(raw)?;

            if !seen.insert(name.clone()) {
                return Err(OpenClawExecutionError::new(
                    OpenClawExecutionErrorKind::InvalidRequest,
                    "spreadsheet.read sheets must not contain duplicates",
                    false,
                ));
            }

            names.push(name);
        }

        return Ok(SpreadsheetReadSelection::Selected(names));
    }

    if all_sheets {
        return Ok(SpreadsheetReadSelection::All);
    }

    Ok(SpreadsheetReadSelection::First)
}

/// Formula-aware read is opt-in. Without it the output is byte-for-byte what
/// it was before, which is what keeps every existing consumer working.
fn spreadsheet_read_include_formulas(
    request: &OpenClawExecutionRequest,
) -> Result<bool, OpenClawExecutionError> {
    match request.input.get("includeFormulas") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "spreadsheet.read includeFormulas must be boolean",
            false,
        )),
    }
}

fn start_spreadsheet_read(
    invoker: &dyn GatewayMethodInvoker,
    request: &OpenClawExecutionRequest,
) -> Result<(String, String, String), OpenClawExecutionError> {
    let (path, workdir) = spreadsheet_read_input(request)?;
    let selection = spreadsheet_read_selection(request)?;
    let include_formulas = spreadsheet_read_include_formulas(request)?;

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

    let command = spreadsheet_read_command_for_selection(path, &selection, include_formulas);

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

/// Everything between the truncation flag and the content block.
///
/// The keys after `AIOS_TRUNCATED=` are OPTIONAL on purpose: a reader that
/// demanded them would reject output from a read that did not ask for
/// formulas, and would have made the existing protocol tests useless as a
/// compatibility guard.
fn parse_sheet_head(head: &str) -> (bool, Option<String>, Vec<Value>, bool) {
    let mut lines = head.lines();
    let truncated = lines.next().map(str::trim) == Some("true");

    let mut used_range = None;
    let mut declared = None;
    let mut formulas = Vec::new();
    let mut inside = false;

    for line in lines {
        if let Some(value) = line.strip_prefix("AIOS_USED_RANGE=") {
            used_range = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("AIOS_FORMULA_COUNT=") {
            declared = value.trim().parse::<usize>().ok();
        } else if line.trim() == "AIOS_FORMULAS_BEGIN" {
            inside = true;
        } else if line.trim() == "AIOS_FORMULAS_END" {
            inside = false;
        } else if inside {
            let mut fields = line.splitn(3, '\t');
            if let (Some(row), Some(column), Some(formula)) =
                (fields.next(), fields.next(), fields.next())
            {
                if let (Ok(row), Ok(column)) =
                    (row.trim().parse::<u64>(), column.trim().parse::<u64>())
                {
                    formulas.push(serde_json::json!({
                        "row": row,
                        "column": column,
                        "formula": formula,
                    }));
                }
            }
        }
    }

    // A declared count that does not match what arrived means the payload was
    // cut, which is a truncation the caller has to know about.
    let short = declared.is_some_and(|declared| declared != formulas.len());

    (truncated || short, used_range, formulas, declared.is_some())
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

    // Backward-compatible Phase A protocol.
    if !text.contains("AIOS_WORKSHEET_COUNT=") {
        let (_, payload) = text.split_once("AIOS_SHEET=")?;
        let (sheet, payload) = payload.split_once("\nAIOS_ROWS=")?;
        let (rows, payload) = payload.split_once("\nAIOS_COLUMNS=")?;
        let (columns, content) = payload.split_once("\nAIOS_CONTENT_BEGIN\n")?;

        let rows = rows.trim().parse::<u64>().ok()?;
        let columns = columns.trim().parse::<u64>().ok()?;

        return Some(serde_json::json!({
            "path": path,
            "sheet": sheet.trim(),
            "status": "table",
            "rows": rows,
            "columns": columns,
            "truncated": text.len() >= MAX_FILE_OUTPUT_BYTES as usize,
            "content": content,
        }));
    }

    let (_, payload) = text.split_once("AIOS_WORKSHEET_COUNT=")?;
    let (worksheet_count, payload) = payload.split_once("\nAIOS_WORKSHEETS=")?;
    let (worksheet_names, payload) = payload.split_once("\nAIOS_SELECTION_TRUNCATED=")?;
    let (selection_truncated, blocks) = payload.split_once("\nAIOS_SHEET_BEGIN\n")?;

    let worksheet_count = worksheet_count.trim().parse::<u64>().ok()?;
    let worksheet_names = if worksheet_names.is_empty() {
        Vec::new()
    } else {
        worksheet_names
            .split('\u{1f}')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    let selection_truncated = selection_truncated.trim() == "true";

    let mut sheets = Vec::new();

    for block in blocks.split("\nAIOS_SHEET_BEGIN\n") {
        let (block, _) = block.split_once("\nAIOS_SHEET_END")?;

        let (_, payload) = block.split_once("AIOS_SHEET=")?;
        let (sheet, payload) = payload.split_once("\nAIOS_ROWS=")?;
        let (rows, payload) = payload.split_once("\nAIOS_COLUMNS=")?;
        let (columns, payload) = payload.split_once("\nAIOS_TOTAL_ROWS=")?;
        let (total_rows, payload) = payload.split_once("\nAIOS_TOTAL_COLUMNS=")?;
        let (total_columns, payload) = payload.split_once("\nAIOS_TRUNCATED=")?;
        let (head, content) = payload.split_once("\nAIOS_CONTENT_BEGIN\n")?;

        let (sheet_truncated, used_range, formulas, formulas_requested) = parse_sheet_head(head);

        let mut entry = serde_json::json!({
            "name": sheet.trim(),
            "rows": rows.trim().parse::<u64>().ok()?,
            "columns": columns.trim().parse::<u64>().ok()?,
            "totalRows": total_rows.trim().parse::<u64>().ok()?,
            "totalColumns": total_columns.trim().parse::<u64>().ok()?,
            "truncated": sheet_truncated,
            "content": content,
        });

        if let Some(used_range) = used_range {
            entry["usedRange"] = Value::String(used_range);
        }

        // An empty array and an absent key mean different things: no formulas
        // on the sheet, versus formulas never asked for.
        if formulas_requested {
            entry["formulas"] = Value::Array(formulas);
        }

        sheets.push(entry);
    }

    if sheets.len() == 1 {
        let sheet = sheets.first()?;

        let mut single = serde_json::json!({
            "path": path,
            "sheet": sheet.get("name")?,
            "status": "table",
            "rows": sheet.get("rows")?,
            "columns": sheet.get("columns")?,
            "totalRows": sheet.get("totalRows")?,
            "totalColumns": sheet.get("totalColumns")?,
            "worksheetCount": worksheet_count,
            "worksheetNames": worksheet_names,
            "truncated": selection_truncated
                || sheet.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "content": sheet.get("content")?,
        });

        for carried in ["usedRange", "formulas"] {
            if let Some(value) = sheet.get(carried) {
                single[carried] = value.clone();
            }
        }

        return Some(single);
    }

    Some(serde_json::json!({
        "path": path,
        "status": "workbook",
        "worksheetCount": worksheet_count,
        "worksheetNames": worksheet_names,
        "sheets": sheets,
        "truncated": selection_truncated
            || sheets.iter().any(|sheet| {
                sheet.get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
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

/// The Excel read command, for tests in other modules that need Excel's own
/// answer for the same file.
#[cfg(all(test, target_os = "macos"))]
pub(crate) fn spreadsheet_read_command_for_test(path: &str) -> String {
    spreadsheet_read_command_for_selection(path, &SpreadsheetReadSelection::First, false)
}

fn spreadsheet_read_command(path: &str) -> String {
    spreadsheet_read_command_for_selection(path, &SpreadsheetReadSelection::First, false)
}

fn spreadsheet_read_command_for_selection(
    path: &str,
    selection: &SpreadsheetReadSelection,
    include_formulas: bool,
) -> String {
    let (mode, requested) = match selection {
        SpreadsheetReadSelection::First => ("first", String::new()),
        SpreadsheetReadSelection::Named(name) => ("named", name.clone()),
        SpreadsheetReadSelection::Selected(names) => ("selected", names.join("\u{1f}")),
        SpreadsheetReadSelection::All => ("all", String::new()),
    };

    format!(
        r#"set -o pipefail
/usr/bin/osascript - {} {} {} {} <<'AIOS_APPLESCRIPT' | /usr/bin/head -c {}
on splitText(sourceText, delimiterText)
    if sourceText is "" then return {{}}
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to delimiterText
    set resultItems to text items of sourceText
    set AppleScript's text item delimiters to oldDelimiters
    return resultItems
end splitText

on joinText(sourceList, delimiterText)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to delimiterText
    set resultText to sourceList as text
    set AppleScript's text item delimiters to oldDelimiters
    return resultText
end joinText

on replaceText(sourceText, oldText, newText)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to oldText
    set parts to text items of sourceText
    set AppleScript's text item delimiters to newText
    set resultText to parts as text
    set AppleScript's text item delimiters to oldDelimiters
    return resultText
end replaceText

on safeCell(cellValue)
    if cellValue is missing value then return ""

    set rendered to cellValue as text
    set rendered to my replaceText(rendered, tab, " ")
    set rendered to my replaceText(rendered, return, " ")
    set rendered to my replaceText(rendered, linefeed, " ")

    if (count characters of rendered) > {MAX_SPREADSHEET_READ_CELL_CHARS} then
        set rendered to text 1 thru {MAX_SPREADSHEET_READ_CELL_CHARS} of rendered
    end if

    return rendered
end safeCell

on joinRow(rowValues, columnLimit)
    set resultValues to {{}}

    repeat with columnIndex from 1 to columnLimit
        if columnIndex <= (count of rowValues) then
            set end of resultValues to my safeCell(contents of item columnIndex of rowValues)
        else
            set end of resultValues to ""
        end if
    end repeat

    return my joinText(resultValues, tab)
end joinRow

on worksheetNames(targetWorkbook)
    tell application "Microsoft Excel"
        set resultNames to {{}}

        repeat with worksheetIndex from 1 to count of worksheets of targetWorkbook
            set end of resultNames to (name of worksheet worksheetIndex of targetWorkbook as text)
        end repeat

        return resultNames
    end tell
end worksheetNames

on containsText(sourceList, targetText)
    repeat with sourceItem in sourceList
        if (contents of sourceItem as text) is targetText then return true
    end repeat

    return false
end containsText

on run argv
    set workbookPath to item 1 of argv
    set readMode to item 2 of argv
    set requestedText to item 3 of argv
    set includeFormulas to (item 4 of argv is "formulas")

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

            if openedWorkbook is missing value then
                error "Excel did not open the workbook"
            end if

            set allWorksheetNames to my worksheetNames(openedWorkbook)
            set worksheetCount to count of allWorksheetNames
            set requestedNames to {{}}
            set selectionTruncated to false

            if readMode is "first" then
                if worksheetCount < 1 then error "Workbook has no worksheets"
                set requestedNames to {{item 1 of allWorksheetNames}}

            else if readMode is "named" then
                if not my containsText(allWorksheetNames, requestedText) then
                    error "Worksheet not found: " & requestedText number -2801
                end if

                set requestedNames to {{requestedText}}

            else if readMode is "selected" then
                set requestedNames to my splitText(requestedText, ASCII character 31)

                repeat with requestedName in requestedNames
                    set requestedNameText to contents of requestedName as text

                    if not my containsText(allWorksheetNames, requestedNameText) then
                        error "Worksheet not found: " & requestedNameText number -2801
                    end if
                end repeat

            else if readMode is "all" then
                set selectionLimit to worksheetCount

                if selectionLimit > {MAX_SPREADSHEET_READ_SHEETS} then
                    set selectionLimit to {MAX_SPREADSHEET_READ_SHEETS}
                    set selectionTruncated to true
                end if

                repeat with worksheetIndex from 1 to selectionLimit
                    set end of requestedNames to item worksheetIndex of allWorksheetNames
                end repeat

            else
                error "Unsupported spreadsheet read mode" number -2802
            end if

            set outputText to "AIOS_WORKSHEET_COUNT=" & worksheetCount & linefeed
            set outputText to outputText & "AIOS_WORKSHEETS=" & my joinText(allWorksheetNames, ASCII character 31) & linefeed
            set outputText to outputText & "AIOS_SELECTION_TRUNCATED=" & (selectionTruncated as text)

            repeat with requestedName in requestedNames
                set sheetName to contents of requestedName as text

                if not (exists worksheet sheetName of openedWorkbook) then
                    error "Worksheet not found: " & sheetName number -2801
                end if

                set usedFormulas to {{}}

                tell worksheet sheetName of openedWorkbook
                    set usedValues to value of used range
                    set usedAddress to (get address of used range) as text

                    -- A real probe showed `formula of used range` comes back in
                    -- the same 2D list shape as `value of used range`, and in
                    -- English even on a Chinese-locale Excel. It is only asked
                    -- for when the caller wants it, so the default read costs
                    -- exactly what it always did.
                    if includeFormulas then
                        set usedFormulas to formula of used range
                    end if
                end tell

                if class of usedValues is not list then
                    set usedValues to {{usedValues}}
                else if (count of usedValues) > 0 then
                    if class of item 1 of usedValues is not list then
                        set usedValues to {{usedValues}}
                    end if
                end if

                if includeFormulas then
                    if class of usedFormulas is not list then
                        set usedFormulas to {{usedFormulas}}
                    else if (count of usedFormulas) > 0 then
                        if class of item 1 of usedFormulas is not list then
                            set usedFormulas to {{usedFormulas}}
                        end if
                    end if
                end if

                set totalRows to count of usedValues

                if totalRows > 0 then
                    set totalColumns to count of item 1 of usedValues
                else
                    set totalColumns to 0
                end if

                set rowLimit to totalRows
                set columnLimit to totalColumns
                set sheetTruncated to false

                if rowLimit > {MAX_SPREADSHEET_READ_ROWS} then
                    set rowLimit to {MAX_SPREADSHEET_READ_ROWS}
                    set sheetTruncated to true
                end if

                if columnLimit > {MAX_SPREADSHEET_READ_COLUMNS} then
                    set columnLimit to {MAX_SPREADSHEET_READ_COLUMNS}
                    set sheetTruncated to true
                end if

                set sheetContent to ""
                set returnedRows to 0

                repeat with rowIndex from 1 to rowLimit
                    set rowValues to item rowIndex of usedValues
                    set rowText to my joinRow(contents of rowValues, columnLimit)

                    if ((count characters of outputText) + (count characters of sheetContent) + (count characters of rowText)) > {MAX_SPREADSHEET_PROTOCOL_CHARS} then
                        set sheetTruncated to true
                        set selectionTruncated to true
                        exit repeat
                    end if

                    set sheetContent to sheetContent & rowText & linefeed
                    set returnedRows to returnedRows + 1
                end repeat

                -- Sparse on purpose. A cell holding a constant reports that
                -- constant as its formula, so the only thing that marks a real
                -- formula is the leading "=", and reporting every cell would
                -- double the payload to say nothing.
                --
                -- The separator is `ASCII character 9`, not `tab`. Inside a
                -- `tell application "Microsoft Excel"` block Excel's own
                -- terminology wins, and `tab` resolves to an Excel term that
                -- renders as the literal text "tab". The joinRow helpers above
                -- can use `tab` safely only because they live outside the tell.
                set formulaLines to ""
                set formulaCount to 0

                if includeFormulas then
                    repeat with rowIndex from 1 to rowLimit
                        if rowIndex > (count of usedFormulas) then exit repeat
                        set formulaRow to contents of item rowIndex of usedFormulas

                        repeat with columnIndex from 1 to columnLimit
                            if columnIndex <= (count of formulaRow) then
                                set formulaText to my safeCell(contents of item columnIndex of formulaRow)

                                if formulaText starts with "=" then
                                    if formulaCount < {MAX_SPREADSHEET_READ_FORMULAS} then
                                        set formulaLines to formulaLines & (rowIndex as text) & (ASCII character 9) & (columnIndex as text) & (ASCII character 9) & formulaText & linefeed
                                        set formulaCount to formulaCount + 1
                                    else
                                        set sheetTruncated to true
                                    end if
                                end if
                            end if
                        end repeat
                    end repeat

                    if ((count characters of outputText) + (count characters of sheetContent) + (count characters of formulaLines)) > {MAX_SPREADSHEET_PROTOCOL_CHARS} then
                        set formulaLines to ""
                        set formulaCount to 0
                        set sheetTruncated to true
                        set selectionTruncated to true
                    end if
                end if

                set outputText to outputText & linefeed & "AIOS_SHEET_BEGIN" & linefeed
                set outputText to outputText & "AIOS_SHEET=" & sheetName & linefeed
                set outputText to outputText & "AIOS_ROWS=" & returnedRows & linefeed
                set outputText to outputText & "AIOS_COLUMNS=" & columnLimit & linefeed
                set outputText to outputText & "AIOS_TOTAL_ROWS=" & totalRows & linefeed
                set outputText to outputText & "AIOS_TOTAL_COLUMNS=" & totalColumns & linefeed
                set outputText to outputText & "AIOS_TRUNCATED=" & (sheetTruncated as text) & linefeed
                set outputText to outputText & "AIOS_USED_RANGE=" & usedAddress & linefeed

                -- Emitted only when asked. Always emitting an empty block would
                -- make "no formulas on this sheet" indistinguishable from "the
                -- caller never asked", which are different answers.
                if includeFormulas then
                    set outputText to outputText & "AIOS_FORMULA_COUNT=" & (formulaCount as text) & linefeed
                    set outputText to outputText & "AIOS_FORMULAS_BEGIN" & linefeed
                    set outputText to outputText & formulaLines
                    set outputText to outputText & "AIOS_FORMULAS_END" & linefeed
                end if
                set outputText to outputText & "AIOS_CONTENT_BEGIN" & linefeed
                set outputText to outputText & sheetContent
                set outputText to outputText & "AIOS_SHEET_END"
            end repeat

            close openedWorkbook saving no
            set openedWorkbook to missing value

            if selectionTruncated then
                set outputText to my replaceText(outputText, "AIOS_SELECTION_TRUNCATED=false", "AIOS_SELECTION_TRUNCATED=true")
            end if

            return outputText

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
        shell_quote(mode),
        shell_quote(&requested),
        shell_quote(if include_formulas { "formulas" } else { "values" }),
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
    // What `textutil` actually writes, which is the same set the read side
    // accepts. This path is the floor: Word answers for .docx when it is
    // installed, and these are the formats left to conversion.
    if !matches!(format.as_str(), "doc" | "docx" | "rtf" | "txt") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.create through macOS conversion supports DOC, DOCX, RTF and TXT files",
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

    use crate::document::resolver::OfficeApplication;

    let route = office_route(DOCUMENT_READ_ACTION, &request.input, "path")
        .ok_or_else(|| no_route(DOCUMENT_READ_ACTION, &request.input, "path"))?;

    match route.application {
        OfficeApplication::GoogleDocs => {
            let output = run_cloud(crate::google_workspace::office::document_read(&request.input))?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS read the Google document.".to_owned()),
            });
        }
        OfficeApplication::ApplePages => {
            let output = crate::document::pages::read_pages_document(&request.input)
                .map_err(map_pages_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS completed the Pages document read.".to_owned()),
            });
        }
        // Word reads its own format with the highest fidelity, so it answers
        // whenever it is installed.
        OfficeApplication::MicrosoftWord => {
            let output = crate::document::word::read_word_document(&request.input)
                .map_err(map_word_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS completed the Word document read.".to_owned()),
            });
        }
        // And when it is not, the capability does not disappear or change
        // shape: .docx is a ZIP of XML, and this returns the same fields Word
        // would have.
        OfficeApplication::StructuredFile => {
            let output = crate::document::structured::read_structured_document(&request.input)
                .map_err(map_structured_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some(
                    "AI-OS read the document directly from the file, without Word.".to_owned(),
                ),
            });
        }
        // PDF, which no Office or iWork application here reads, and which is
        // read in Rust rather than by whatever the operating system ships. A
        // page with no text of its own is recognised where the platform can,
        // and the result says which pages those were -- recognised text is not
        // the same kind of evidence as text the file declares.
        OfficeApplication::LocalPdf => {
            let output = crate::document::pdf::read_pdf_document(&request.input)
                .map_err(map_pdf_error)?;

            return Ok(OpenClawExecutionResult {
                output,
                summary: Some("AI-OS read the PDF.".to_owned()),
            });
        }
        // The floor: the only path that reads the old binary .doc with no Word
        // installed. It converts to plain text, which is why it is an import
        // route and never outranks an application on its own format.
        OfficeApplication::MacosNative => {}
        other => {
            return Err(OpenClawExecutionError::new(
                OpenClawExecutionErrorKind::ExecutionFailed,
                format!("Office application {other:?} has no document.read adapter."),
                false,
            ));
        }
    }

    let extension = document_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !matches!(extension.as_str(), "doc" | "docx" | "rtf" | "txt") {
        return Err(OpenClawExecutionError::new(
            OpenClawExecutionErrorKind::InvalidRequest,
            "document.read through macOS conversion supports DOC, DOCX, RTF and TXT files",
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

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn spreadsheet_edit_runtime_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("runtime-edit.xlsx");
        let invoker = RecordingInvoker {
            calls: Mutex::new(Vec::new()),
            outcome: Ok(json!({"unexpected": true})),
        };

        let result = execute_with_invoker(
            &invoker,
            &request(
                "spreadsheet.edit",
                json!({
                    "source": fixture,
                    "destination": destination,
                    "operations": [
                        {"type":"set_cell","sheet":"Sheet1","row":2,"column":4,"value":12345},
                        {"type":"set_formula","sheet":"Sheet1","row":3,"column":4,"formula":"=1+2"},
                        {"type":"clear_cell","sheet":"Sheet1","row":2,"column":1}
                    ]
                }),
            ),
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(result.output["capability"], "spreadsheet.edit");
        assert_eq!(result.output["selectedProvider"], "MicrosoftOffice");
        assert_eq!(result.output["operationResult"]["status"], "edited-copy");
        assert!(invoker.calls.lock().unwrap().is_empty());
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

    use std::process::Command;

    #[test]
    fn spreadsheet_read_phase_b_selection_contract_is_bounded_and_fail_closed() {
        assert_eq!(
            spreadsheet_read_selection(&request(
                "spreadsheet.read",
                json!({"path":"/safe/report.xlsx"})
            ))
            .unwrap(),
            SpreadsheetReadSelection::First
        );

        assert_eq!(
            spreadsheet_read_selection(&request(
                "spreadsheet.read",
                json!({"path":"/safe/report.xlsx","sheet":"Summary"})
            ))
            .unwrap(),
            SpreadsheetReadSelection::Named("Summary".to_owned())
        );

        assert_eq!(
            spreadsheet_read_selection(&request(
                "spreadsheet.read",
                json!({"path":"/safe/report.xlsx","sheets":["Sales","Summary"]})
            ))
            .unwrap(),
            SpreadsheetReadSelection::Selected(vec!["Sales".to_owned(), "Summary".to_owned()])
        );

        assert_eq!(
            spreadsheet_read_selection(&request(
                "spreadsheet.read",
                json!({"path":"/safe/report.xlsx","allSheets":true})
            ))
            .unwrap(),
            SpreadsheetReadSelection::All
        );

        for input in [
            json!({"path":"/safe/report.xlsx","sheet":"","allSheets":false}),
            json!({"path":"/safe/report.xlsx","sheet":"Bad/Name"}),
            json!({"path":"/safe/report.xlsx","sheets":[]}),
            json!({"path":"/safe/report.xlsx","sheets":["Sales","Sales"]}),
            json!({"path":"/safe/report.xlsx","sheet":"Sales","allSheets":true}),
            json!({"path":"/safe/report.xlsx","sheet":"Sales","sheets":["Summary"]}),
            json!({"path":"/safe/report.xlsx","allSheets":"yes"}),
        ] {
            let error =
                spreadsheet_read_selection(&request("spreadsheet.read", input)).unwrap_err();
            assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
            assert!(!error.retryable);
        }

        let too_many = (0..=MAX_SPREADSHEET_READ_SHEETS)
            .map(|index| format!("S{index}"))
            .collect::<Vec<_>>();

        let error = spreadsheet_read_selection(&request(
            "spreadsheet.read",
            json!({"path":"/safe/report.xlsx","sheets":too_many}),
        ))
        .unwrap_err();

        assert_eq!(error.kind, OpenClawExecutionErrorKind::InvalidRequest);
    }

    #[test]
    fn spreadsheet_read_phase_b_command_encodes_selection_and_bounds() {
        let first = spreadsheet_read_command("/safe/report file.xlsx");

        assert!(first.contains("first"));
        assert!(first.contains("AIOS_WORKSHEET_COUNT="));
        assert!(first.contains("AIOS_WORKSHEETS="));
        assert!(first.contains("AIOS_SHEET_BEGIN"));
        assert!(first.contains("AIOS_TOTAL_ROWS="));
        assert!(first.contains("AIOS_TOTAL_COLUMNS="));
        assert!(first.contains("AIOS_SELECTION_TRUNCATED="));
        assert!(first.contains("close openedWorkbook saving no"));
        assert!(first.contains(&MAX_SPREADSHEET_READ_SHEETS.to_string()));
        assert!(first.contains(&MAX_SPREADSHEET_READ_ROWS.to_string()));
        assert!(first.contains(&MAX_SPREADSHEET_READ_COLUMNS.to_string()));
        assert!(first.contains(&MAX_SPREADSHEET_PROTOCOL_CHARS.to_string()));

        let named = spreadsheet_read_command_for_selection(
            "/safe/report.xlsx",
            &SpreadsheetReadSelection::Named("Summary".to_owned()),
            false,
        );

        assert!(named.contains("named"));
        assert!(named.contains("Summary"));

        let selected = spreadsheet_read_command_for_selection(
            "/safe/report.xlsx",
            &SpreadsheetReadSelection::Selected(vec!["Sales".to_owned(), "Summary".to_owned()]),
            false,
        );

        assert!(selected.contains("selected"));
        assert!(selected.contains("Sales"));
        assert!(selected.contains("Summary"));
    }

    #[test]
    fn spreadsheet_read_phase_b_parser_returns_workbook_and_single_sheet_shapes() {
        let multi = format!(
            "AIOS_WORKSHEET_COUNT=2\nAIOS_WORKSHEETS=Sales\u{1f}Summary\nAIOS_SELECTION_TRUNCATED=false\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=Sales\nAIOS_ROWS=2\nAIOS_COLUMNS=2\nAIOS_TOTAL_ROWS=2\nAIOS_TOTAL_COLUMNS=2\nAIOS_TRUNCATED=false\nAIOS_CONTENT_BEGIN\nName\tValue\nAlpha\t42\nAIOS_SHEET_END\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=Summary\nAIOS_ROWS=2\nAIOS_COLUMNS=2\nAIOS_TOTAL_ROWS=2\nAIOS_TOTAL_COLUMNS=2\nAIOS_TRUNCATED=false\nAIOS_CONTENT_BEGIN\nMetric\tValue\nTotal\t42\nAIOS_SHEET_END"
        );

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":multi}]
        }]});

        let parsed = spreadsheet_read_output(&history, "/safe/report.xlsx").unwrap();

        assert_eq!(parsed["status"], "workbook");
        assert_eq!(parsed["worksheetCount"], 2);
        assert_eq!(parsed["worksheetNames"], json!(["Sales", "Summary"]));
        assert_eq!(parsed["sheets"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["sheets"][0]["name"], "Sales");
        assert_eq!(parsed["sheets"][1]["name"], "Summary");
        assert_eq!(parsed["truncated"], false);

        let single = "AIOS_WORKSHEET_COUNT=2\nAIOS_WORKSHEETS=Sales\u{1f}Summary\nAIOS_SELECTION_TRUNCATED=false\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=Summary\nAIOS_ROWS=2\nAIOS_COLUMNS=2\nAIOS_TOTAL_ROWS=2\nAIOS_TOTAL_COLUMNS=2\nAIOS_TRUNCATED=false\nAIOS_CONTENT_BEGIN\nMetric\tValue\nTotal\t42\nAIOS_SHEET_END";

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":single}]
        }]});

        let parsed = spreadsheet_read_output(&history, "/safe/report.xlsx").unwrap();

        assert_eq!(parsed["status"], "table");
        assert_eq!(parsed["sheet"], "Summary");
        assert_eq!(parsed["worksheetCount"], 2);
        assert_eq!(parsed["worksheetNames"], json!(["Sales", "Summary"]));
        assert_eq!(parsed["rows"], 2);
        assert_eq!(parsed["columns"], 2);
        assert_eq!(parsed["totalRows"], 2);
        assert_eq!(parsed["totalColumns"], 2);
    }

    /// A directory Excel can always read from.
    ///
    /// A read E2E opens the workbook in its OWN osascript invocation, so it does
    /// not inherit the implicit access macOS grants an app for a file that app
    /// just wrote. Reading from a temp directory therefore makes Excel raise a
    /// "please locate this file" grant prompt that no automated run can answer,
    /// and that prompt then blocks every later Excel automation until a person
    /// dismisses it. The edit-side E2Es do not hit this: they reopen their
    /// output inside the same invocation that saved it.
    #[cfg(target_os = "macos")]
    fn excel_readable_workspace(label: &str) -> std::path::PathBuf {
        let root = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join("Library/Containers/com.microsoft.Excel/Data/Library/Caches/com.microsoft.Excel")
            .join(format!("ai-os-{label}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// The realistic workflow: build a sales sheet the way someone actually
    /// would, then read it back through the OTHER capability.
    ///
    /// This is the only test that crosses the two halves of the Excel work. The
    /// per-phase E2Es validate the edit adapter against its own reopened copy;
    /// this one proves the file is also correct to a separate `spreadsheet.read`
    /// invocation, which is what a caller actually gets.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn excel_phase_h_realistic_workflow_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = excel_readable_workspace("phase-h");
        let workbook = root.join("phase-h-sales.xlsx");
        let _ = fs::remove_file(&workbook);

        // Region  Q1  Q2  Total
        // North   10  20  =SUM(B2:C2) -> 30
        // South   50   5  =SUM(B3:C3) -> 55
        // East    30  30  =SUM(B4:C4) -> 60
        //
        // Then the ordinary things someone does next: sort by the total, make
        // the header stand out, give the numbers a format, widen the label
        // column, chart it, and filter to the rows that matter.
        //
        // The order is deliberate. Sorting moves cells, so everything written
        // before it is reported as displaced, and everything that has to be
        // asserted at a fixed address comes after it.
        let result = crate::document::excel::edit_excel_workbook(&json!({
            "source": fixture,
            "destination": workbook,
            "operations": [
                {"type":"add_worksheet","name":"Sales"},

                {"type":"set_cell","sheet":"Sales","row":1,"column":1,"value":"Region"},
                {"type":"set_cell","sheet":"Sales","row":1,"column":2,"value":"Q1"},
                {"type":"set_cell","sheet":"Sales","row":1,"column":3,"value":"Q2"},
                {"type":"set_cell","sheet":"Sales","row":1,"column":4,"value":"Total"},

                {"type":"set_cell","sheet":"Sales","row":2,"column":1,"value":"North"},
                {"type":"set_cell","sheet":"Sales","row":2,"column":2,"value":10},
                {"type":"set_cell","sheet":"Sales","row":2,"column":3,"value":20},
                {"type":"set_formula","sheet":"Sales","row":2,"column":4,"formula":"=SUM(B2:C2)"},

                {"type":"set_cell","sheet":"Sales","row":3,"column":1,"value":"South"},
                {"type":"set_cell","sheet":"Sales","row":3,"column":2,"value":50},
                {"type":"set_cell","sheet":"Sales","row":3,"column":3,"value":5},
                {"type":"set_formula","sheet":"Sales","row":3,"column":4,"formula":"=SUM(B3:C3)"},

                {"type":"set_cell","sheet":"Sales","row":4,"column":1,"value":"East"},
                {"type":"set_cell","sheet":"Sales","row":4,"column":2,"value":30},
                {"type":"set_cell","sheet":"Sales","row":4,"column":3,"value":30},
                {"type":"set_formula","sheet":"Sales","row":4,"column":4,"formula":"=SUM(B4:C4)"},

                {"type":"sort_range","sheet":"Sales",
                 "startRow":1,"startColumn":1,"endRow":4,"endColumn":4,
                 "keyColumn":4,"order":"descending","hasHeader":true},

                {"type":"format_cells","sheet":"Sales",
                 "startRow":1,"startColumn":1,"endRow":1,"endColumn":4,
                 "bold":true,"fillColorIndex":15},
                {"type":"format_cells","sheet":"Sales",
                 "startRow":2,"startColumn":2,"endRow":4,"endColumn":4,
                 "numberFormat":"#,##0"},
                {"type":"set_column_width","sheet":"Sales","column":1,"width":18},

                {"type":"add_chart","sheet":"Sales","startRow":1,"startColumn":1,
                 "endRow":4,"endColumn":2,"chartType":"column","name":"Q1ByRegion"},

                {"type":"apply_filter","sheet":"Sales",
                 "startRow":1,"startColumn":1,"endRow":4,"endColumn":4,
                 "field":4,"criteria":">40"}
            ]
        }))
        .unwrap();

        assert_eq!(result["operationResult"]["status"], "edited-copy");
        assert_eq!(
            fs::read(&fixture).unwrap(),
            original,
            "the source workbook must be preserved"
        );

        let validation: Vec<String> = result["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();

        let entry = |prefix: &str| -> String {
            validation
                .iter()
                .find(|value| value.starts_with(prefix))
                .unwrap_or_else(|| panic!("missing {prefix} in {validation:#?}"))
                .clone()
        };

        // Sorting descending by Total puts East (60) above South (55). The probe
        // reads the key column, whose values are numbers, and Excel renders those
        // with a decimal -- so this checks the ordering, not the rendering.
        let sorted = entry("sort_range:Sales!A1:D4:");
        assert!(
            sorted.contains("D2=60") && sorted.contains("D3=55"),
            "descending sort by Total did not reorder the rows: {sorted}"
        );

        // Everything written before the sort moved with it.
        assert!(
            validation
                .iter()
                .any(|value| value == "set_cell:Sales!A2=displaced-by-moved-content"),
            "writes made before a sort must be reported as displaced: {validation:#?}"
        );

        // Everything after the sort is asserted at a fixed address.
        assert_eq!(
            entry("format_cells:Sales!A1:D1:"),
            "format_cells:Sales!A1:D1:bold=true;fill=15;"
        );
        assert_eq!(
            entry("format_cells:Sales!B2:D4:"),
            "format_cells:Sales!B2:D4:number=#,##0;"
        );

        let width = entry("set_column_width:Sales!1=");
        let measured: f64 = width
            .trim_start_matches("set_column_width:Sales!1=")
            .parse()
            .unwrap_or_else(|_| panic!("unparseable width in {width}"));
        assert!((measured - 18.0).abs() <= 0.5, "{width} is not about 18");

        assert_eq!(
            entry("add_chart:Sales!A1:B4:"),
            "add_chart:Sales!A1:B4:name=Q1ByRegion,type=column clustered,series=1,\
f1==SERIES(Sales!$B$1,Sales!$A$2:$A$4,Sales!$B$2:$B$4,1)"
        );

        // Total > 40 keeps East (60) in row 2 and hides North (30) in row 4.
        assert_eq!(
            entry("apply_filter:Sales!A1:D4:"),
            "apply_filter:Sales!A1:D4:mode=true,row2hidden=false,row4hidden=true"
        );

        // Now the part no per-phase test covers: read the finished file back
        // through the other capability, in its own osascript invocation.
        let path = workbook.to_str().unwrap();
        let output = Command::new("/bin/bash")
            .arg("-lc")
            .arg(spreadsheet_read_command_for_selection(
                path,
                &SpreadsheetReadSelection::Named("Sales".to_owned()),
                true,
            ))
            .output()
            .unwrap();
        assert!(output.status.success());

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        eprintln!("--- phase-h read ---\n{text}\n--- end ---");

        let read = spreadsheet_read_output(
            &json!({"messages":[{
                "role":"toolResult",
                "toolName":"exec",
                "isError":false,
                "content":[{"text": text}]
            }]}),
            path,
        )
        .unwrap();

        assert_eq!(read["sheet"], "Sales");
        assert_eq!(read["usedRange"], "$A$1:$D$4");

        // The sort is visible in the file itself, not just in the adapter's own
        // report: the rows come back in descending order of Total.
        let content = read["content"].as_str().unwrap();
        let position = |needle: &str| {
            content
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} missing from:\n{content}"))
        };
        assert!(
            position("East") < position("South") && position("South") < position("North"),
            "rows are not in descending Total order:\n{content}"
        );

        // A filtered row is hidden, not deleted: North is still in the file.
        assert!(content.contains("North"));

        // Excel rewrote each formula's relative references as the sort moved its
        // row, so the totals still add up their own row rather than the row they
        // were written on.
        let formulas = read["formulas"].as_array().unwrap();
        assert_eq!(formulas.len(), 3, "expected one total per region: {formulas:#?}");
        for (index, expected_row) in [2u64, 3, 4].into_iter().enumerate() {
            assert_eq!(formulas[index]["row"], expected_row);
            assert_eq!(formulas[index]["column"], 4);
            assert_eq!(
                formulas[index]["formula"],
                format!("=SUM(B{expected_row}:C{expected_row})")
            );
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn spreadsheet_read_phase_b_real_multi_sheet_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();
        let root = excel_readable_workspace("phase-b-read");
        let multi = root.join("phase-b-multi-read.xlsx");
        let _ = fs::remove_file(&multi);

        crate::document::excel::edit_excel_workbook(&json!({
            "source": fixture,
            "destination": multi,
            "operations": [
                {"type":"add_worksheet","name":"Summary"},
                {"type":"set_cell","sheet":"Summary","row":1,"column":1,"value":"Metric"},
                {"type":"set_cell","sheet":"Summary","row":1,"column":2,"value":"Value"},
                {"type":"set_cell","sheet":"Summary","row":2,"column":1,"value":"Total"},
                {"type":"set_cell","sheet":"Summary","row":2,"column":2,"value":"NinetyNine"}
            ]
        }))
        .unwrap();

        assert_eq!(fs::read(&fixture).unwrap(), original);

        let multi_path = multi.to_str().unwrap();

        let all_command =
            spreadsheet_read_command_for_selection(multi_path, &SpreadsheetReadSelection::All, false);

        let all_output = Command::new("/bin/bash")
            .arg("-lc")
            .arg(all_command)
            .output()
            .unwrap();

        assert!(all_output.status.success());

        let all_text = String::from_utf8_lossy(&all_output.stdout).to_string();

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":all_text}]
        }]});

        let all = spreadsheet_read_output(&history, multi_path).unwrap();

        assert_eq!(all["status"], "workbook");
        assert_eq!(all["worksheetCount"], 2);
        assert_eq!(all["worksheetNames"].as_array().unwrap().len(), 2);
        assert!(all["worksheetNames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "Sheet1"));
        assert!(all["worksheetNames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "Summary"));

        let sheets = all["sheets"].as_array().unwrap();

        assert_eq!(sheets.len(), 2);

        let summary = sheets
            .iter()
            .find(|sheet| sheet["name"] == "Summary")
            .unwrap();

        assert!(summary["content"]
            .as_str()
            .unwrap()
            .contains("Metric\tValue"));
        assert!(summary["content"]
            .as_str()
            .unwrap()
            .contains("Total\tNinetyNine"));

        let named_command = spreadsheet_read_command_for_selection(
            multi_path,
            &SpreadsheetReadSelection::Named("Summary".to_owned()),
            false,
        );

        let named_output = Command::new("/bin/bash")
            .arg("-lc")
            .arg(named_command)
            .output()
            .unwrap();

        assert!(named_output.status.success());

        let named_text = String::from_utf8_lossy(&named_output.stdout).to_string();

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":named_text}]
        }]});

        let named = spreadsheet_read_output(&history, multi_path).unwrap();

        assert_eq!(named["status"], "table");
        assert_eq!(named["sheet"], "Summary");
        assert_eq!(named["worksheetCount"], 2);
        assert!(named["content"].as_str().unwrap().contains("Metric\tValue"));

        let selected_command = spreadsheet_read_command_for_selection(
            multi_path,
            &SpreadsheetReadSelection::Selected(vec!["Summary".to_owned(), "Sheet1".to_owned()]),
            false,
        );

        let selected_output = Command::new("/bin/bash")
            .arg("-lc")
            .arg(selected_command)
            .output()
            .unwrap();

        assert!(selected_output.status.success());

        let selected_text = String::from_utf8_lossy(&selected_output.stdout).to_string();

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":selected_text}]
        }]});

        let selected = spreadsheet_read_output(&history, multi_path).unwrap();

        assert_eq!(selected["status"], "workbook");
        assert_eq!(selected["sheets"].as_array().unwrap().len(), 2);

        let missing_command = spreadsheet_read_command_for_selection(
            multi_path,
            &SpreadsheetReadSelection::Named("Missing".to_owned()),
            false,
        );

        let missing_output = Command::new("/bin/bash")
            .arg("-lc")
            .arg(missing_command)
            .output()
            .unwrap();

        assert!(missing_output.status.success());

        let missing_text = String::from_utf8_lossy(&missing_output.stdout).to_string();

        let history = json!({"messages":[{
            "role":"toolResult",
            "toolName":"exec",
            "isError":false,
            "content":[{"text":missing_text}]
        }]});

        let missing = spreadsheet_read_output(&history, multi_path).unwrap();

        assert_eq!(missing["status"], "failed");
        assert!(missing["error"]
            .as_str()
            .unwrap()
            .contains("Worksheet not found"));
    }

    #[test]
    fn iwork_formats_route_by_their_own_extension() {
        // The registry is ordered by priority, so Microsoft Office wins every
        // capability it declares while it is installed. Without this dispatch a
        // .pages file is handed to an adapter that cannot open it.
        assert!(is_format(&json!({"path": "/safe/report.pages"}), "path", "pages"));
        assert!(is_format(&json!({"path": "/safe/REPORT.PAGES"}), "path", "pages"));
        assert!(is_format(&json!({"path": "/safe/book.numbers"}), "path", "numbers"));
        assert!(is_format(
            &json!({"source": "/safe/report.pages", "destination": "/safe/report.pdf"}),
            "source",
            "pages"
        ));

        // Everything else falls through to the provider list, unchanged.
        for other in ["/safe/report.docx", "/safe/book.xlsx", "/safe/deck.key", "/safe/plain"] {
            assert!(!is_format(&json!({"path": other}), "path", "pages"));
            assert!(!is_format(&json!({"path": other}), "path", "numbers"));
        }

        // The presentation route had the same defect in reverse: it resolved
        // straight to Apple iWork, so a .pptx reached the Keynote adapter and
        // was refused for not being a .key -- while PowerPoint's own proven
        // adapter could not be reached at all.
        assert!(is_format(&json!({"path": "/safe/deck.pptx"}), "path", "pptx"));
        assert!(is_format(&json!({"path": "/safe/deck.ppt"}), "path", "ppt"));
        assert!(!is_format(&json!({"path": "/safe/deck.key"}), "path", "pptx"));
        assert!(!is_format(&json!({"path": "/safe/deck.key"}), "path", "ppt"));

        // A missing or non-string field is not a format.
        assert!(!is_format(&json!({}), "path", "pages"));
        assert!(!is_format(&json!({"path": 42}), "path", "pages"));
        assert_eq!(request_format(&json!({"path": "/safe/a.PAGES"}), "path").as_deref(), Some("pages"));
    }

    #[test]
    fn iwork_declares_only_what_it_can_execute() {
        let iwork = crate::document::registry::office_providers()
            .into_iter()
            .find(|provider| provider.id == OfficeProviderId::AppleIwork)
            .unwrap();

        for executable in [
            "document.read",
            "document.create",
            "document.convert",
            "spreadsheet.read",
            "spreadsheet.create",
            "spreadsheet.convert",
            "presentation.read",
            "presentation.create",
            "presentation.convert",
        ] {
            assert!(iwork.supports(executable), "iWork should declare {executable}");
        }

        // Numbers has a read and a create adapter but no edit one. Declaring a
        // capability the provider cannot execute is what made the Provider
        // Matrix overstate iWork; a registry declaration is not evidence.
        assert!(!iwork.supports("spreadsheet.edit"));
    }

    #[test]
    fn spreadsheet_read_include_formulas_is_opt_in_and_typed() {
        let read = |input: Value| request(SPREADSHEET_READ_ACTION, input);

        // Absent and null both mean the read behaves exactly as it always did.
        assert!(!spreadsheet_read_include_formulas(&read(json!({"path": "/safe/a.xlsx"}))).unwrap());
        assert!(!spreadsheet_read_include_formulas(&read(
            json!({"path": "/safe/a.xlsx", "includeFormulas": null})
        ))
        .unwrap());
        assert!(spreadsheet_read_include_formulas(&read(
            json!({"path": "/safe/a.xlsx", "includeFormulas": true})
        ))
        .unwrap());

        // A truthy string is not a boolean. Accepting it would silently change
        // the shape of the response for a caller who did not ask.
        for rejected in [json!("true"), json!(1), json!([]), json!({})] {
            assert!(spreadsheet_read_include_formulas(&read(
                json!({"path": "/safe/a.xlsx", "includeFormulas": rejected})
            ))
            .is_err());
        }
    }

    #[test]
    fn spreadsheet_read_phase_g_command_asks_excel_for_formulas_only_when_requested() {
        let without =
            spreadsheet_read_command_for_selection("/safe/a.xlsx", &SpreadsheetReadSelection::First, false);
        let with =
            spreadsheet_read_command_for_selection("/safe/a.xlsx", &SpreadsheetReadSelection::First, true);

        assert!(without.contains("'values'"));
        assert!(with.contains("'formulas'"));

        // The AppleScript is the same either way; the argv decides. A second
        // script would be a second thing to keep correct.
        assert!(without.contains("formula of used range"));
        assert!(with.contains("formula of used range"));
        assert!(with.contains("AIOS_FORMULAS_BEGIN"));
    }

    #[test]
    fn spreadsheet_read_phase_g_parser_reports_formulas_sparsely() {
        let history = |text: &str| {
            json!({"messages":[{
                "role":"toolResult",
                "toolName":"exec",
                "isError":false,
                "content":[{"text": text}]
            }]})
        };

        let with_formulas = "AIOS_WORKSHEET_COUNT=1\nAIOS_WORKSHEETS=Sheet1\nAIOS_SELECTION_TRUNCATED=false\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=Sheet1\nAIOS_ROWS=4\nAIOS_COLUMNS=2\nAIOS_TOTAL_ROWS=4\nAIOS_TOTAL_COLUMNS=2\n\
AIOS_TRUNCATED=false\nAIOS_USED_RANGE=$A$1:$B$4\nAIOS_FORMULA_COUNT=1\nAIOS_FORMULAS_BEGIN\n\
4\t2\t=SUM(B2:B3)\nAIOS_FORMULAS_END\nAIOS_CONTENT_BEGIN\nName\tScore\nBravo\t2\nAlpha\t1\nTotal\t3\nAIOS_SHEET_END";

        let parsed = spreadsheet_read_output(&history(with_formulas), "/safe/a.xlsx").unwrap();

        assert_eq!(parsed["usedRange"], "$A$1:$B$4");
        assert_eq!(parsed["formulas"][0]["row"], 4);
        assert_eq!(parsed["formulas"][0]["column"], 2);
        assert_eq!(parsed["formulas"][0]["formula"], "=SUM(B2:B3)");
        assert_eq!(parsed["formulas"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["truncated"], false);

        // Asked for and none present is an empty array, which is not the same
        // answer as never asked.
        let none_present = with_formulas
            .replace("AIOS_FORMULA_COUNT=1", "AIOS_FORMULA_COUNT=0")
            .replace("4\t2\t=SUM(B2:B3)\n", "");
        let parsed = spreadsheet_read_output(&history(&none_present), "/safe/a.xlsx").unwrap();
        assert_eq!(parsed["formulas"], json!([]));

        // Never asked: the key is absent, and the pre-Phase-G protocol still
        // parses unchanged.
        let never_asked = "AIOS_WORKSHEET_COUNT=1\nAIOS_WORKSHEETS=Sheet1\nAIOS_SELECTION_TRUNCATED=false\n\
AIOS_SHEET_BEGIN\nAIOS_SHEET=Sheet1\nAIOS_ROWS=1\nAIOS_COLUMNS=1\nAIOS_TOTAL_ROWS=1\nAIOS_TOTAL_COLUMNS=1\n\
AIOS_TRUNCATED=false\nAIOS_CONTENT_BEGIN\nName\nAIOS_SHEET_END";
        let parsed = spreadsheet_read_output(&history(never_asked), "/safe/a.xlsx").unwrap();
        assert!(parsed.get("formulas").is_none());
        assert!(parsed.get("usedRange").is_none());
        // The trailing newline belongs to the AIOS_SHEET_END delimiter, so the
        // content ends at the last cell.
        assert_eq!(parsed["content"], "Name");

        // A declared count that does not match what arrived means the payload
        // was cut on the way out, and the caller has to be told.
        let cut = with_formulas.replace("AIOS_FORMULA_COUNT=1", "AIOS_FORMULA_COUNT=9");
        let parsed = spreadsheet_read_output(&history(&cut), "/safe/a.xlsx").unwrap();
        assert_eq!(parsed["truncated"], true);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires Microsoft Excel and AI_OS_EXCEL_EDIT_FIXTURE"]
    fn spreadsheet_read_phase_g_formula_real_e2e() {
        let fixture = std::env::var("AI_OS_EXCEL_EDIT_FIXTURE").expect("preserved fixture path");

        let original = fs::read(&fixture).unwrap();

        let root = excel_readable_workspace("phase-g");
        let workbook = root.join("phase-g-formula-read.xlsx");
        let _ = fs::remove_file(&workbook);

        crate::document::excel::edit_excel_workbook(&json!({
            "source": fixture,
            "destination": workbook,
            "operations": [
                {"type":"add_worksheet","name":"Computed"},
                {"type":"set_cell","sheet":"Computed","row":1,"column":1,"value":"Name"},
                {"type":"set_cell","sheet":"Computed","row":1,"column":2,"value":"Score"},
                {"type":"set_cell","sheet":"Computed","row":2,"column":1,"value":"Bravo"},
                {"type":"set_cell","sheet":"Computed","row":2,"column":2,"value":2},
                {"type":"set_cell","sheet":"Computed","row":3,"column":1,"value":"Alpha"},
                {"type":"set_cell","sheet":"Computed","row":3,"column":2,"value":1},
                {"type":"set_cell","sheet":"Computed","row":4,"column":1,"value":"Total"},
                {"type":"set_formula","sheet":"Computed","row":4,"column":2,"formula":"=SUM(B2:B3)"}
            ]
        }))
        .unwrap();

        assert_eq!(fs::read(&fixture).unwrap(), original);

        let path = workbook.to_str().unwrap();
        assert!(
            workbook.is_file(),
            "the edit adapter did not leave a workbook at {path}"
        );
        eprintln!("phase-g workbook: {path} ({} bytes)", fs::metadata(&workbook).unwrap().len());

        let selection = SpreadsheetReadSelection::Named("Computed".to_owned());

        let run = |include_formulas: bool| {
            let output = Command::new("/bin/bash")
                .arg("-lc")
                .arg(spreadsheet_read_command_for_selection(
                    path,
                    &selection,
                    include_formulas,
                ))
                .output()
                .unwrap();
            assert!(output.status.success());
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            eprintln!("--- raw read output (includeFormulas={include_formulas}) ---\n{text}\n--- end ---");
            spreadsheet_read_output(
                &json!({"messages":[{
                    "role":"toolResult",
                    "toolName":"exec",
                    "isError":false,
                    "content":[{"text": text}]
                }]}),
                path,
            )
            .unwrap()
        };

        // Values only: unchanged behaviour, and no formula keys at all.
        let values = run(false);
        assert_eq!(values["sheet"], "Computed", "values read returned {values:#?}");
        assert!(values["content"].as_str().unwrap().contains("Total"));

        // The used range is always known, but formulas are opt-in: no key at
        // all, rather than an empty list that would read as "none present".
        assert_eq!(values["usedRange"], "$A$1:$B$4");
        assert!(values.get("formulas").is_none());

        // Formula-aware: the computed VALUE still comes back in the grid, and
        // the formula is reported separately at its own coordinates.
        let formulas = run(true);
        assert_eq!(formulas["usedRange"], "$A$1:$B$4");
        assert!(formulas["content"].as_str().unwrap().contains("Total"));

        let reported = formulas["formulas"].as_array().unwrap();
        assert_eq!(
            reported.len(),
            1,
            "only the cell that actually holds a formula: {reported:?}"
        );
        assert_eq!(reported[0]["row"], 4);
        assert_eq!(reported[0]["column"], 2);
        assert_eq!(reported[0]["formula"], "=SUM(B2:B3)");

        let _ = fs::remove_dir_all(&root);
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
        // .docx to .doc, not .doc to .docx. Word's adapter now performs the
        // forward direction and therefore wins it; it does not write the old
        // binary format, so coming back is still macOS's own conversion, which
        // is what this test is about.
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
                    "source": "/safe/source file.docx",
                    "destination": "/safe/result file.doc"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "source": "/safe/source file.docx",
                "destination": "/safe/result file.doc",
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
        assert!(message.contains("/usr/bin/textutil -convert doc"));
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
    /// The floor, exercised on a format only it claims.
    ///
    /// Same reasoning as the read side: a `.docx` here would route to Word on a
    /// machine that has Word and to plain-text conversion on one that does not,
    /// so the assertion would depend on the machine running the test.
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
                    "path": "/safe/example/new report.rtf",
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
                "path": "/safe/example/new report.rtf",
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
        assert!(message.contains("/usr/bin/textutil -convert rtf"));
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

        for accepted in ["/safe/report.doc", "/safe/report.rtf", "/safe/report.txt"] {
            assert!(
                document_create_input(&request(
                    "document.create",
                    json!({"path": accepted, "content": "body"})
                ))
                .is_ok(),
                "{accepted} should be accepted by the conversion floor"
            );
        }

        for input in [
            json!({"path": "relative.docx", "content": "body"}),
            json!({"path": "/safe/report.docx", "content": "body", "overwrite": true}),
            // .txt and .rtf are accepted now -- textutil writes them, and this
            // path is the floor for the formats Word does not answer for. What
            // is still refused is a format nothing here converts to.
            json!({"path": "/safe/report.odt", "content": "body"}),
            json!({"path": "/safe/report", "content": "body"}),
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
    /// The floor, exercised on a format only it claims.
    ///
    /// A `.docx` here would route to Word on a machine that has Word and to the
    /// structured layer on one that does not, so the assertion would depend on
    /// the machine running the test rather than on the code. `.rtf` is read by
    /// nothing else, so this test says the same thing everywhere.
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
                    "path": "/safe/example/read me.rtf"
                }),
            ),
            &mut |_| {},
        )
        .unwrap();
        let calls = invoker.calls.lock().unwrap();

        assert_eq!(
            result.output,
            json!({
                "path": "/safe/example/read me.rtf",
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
