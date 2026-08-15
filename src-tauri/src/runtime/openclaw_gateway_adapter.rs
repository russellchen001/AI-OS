use super::openclaw_execution::{
    OpenClawExecutionAdapter, OpenClawExecutionError, OpenClawExecutionErrorKind,
    OpenClawExecutionProgress, OpenClawExecutionRequest, OpenClawExecutionResult,
};
use crate::openclaw::{
    invoke_active_gateway_method, ActiveGatewayFailureKind, ActiveGatewayMethodFailure,
};
use serde_json::Value;
use std::path::Path;

const FILESYSTEM_SCAN_ACTION: &str = "filesystem.scan";
const FILESYSTEM_READ_ACTION: &str = "filesystem.read";
const FILESYSTEM_WRITE_ACTION: &str = "filesystem.write";
const FILESYSTEM_AGENT_ID: &str = "ai-os-files";
const AGENT_WAIT_ATTEMPTS: usize = 35;
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
    if request.action.as_str() == FILESYSTEM_WRITE_ACTION {
        return execute_filesystem_write(invoker, request);
    }

    invoker
        .invoke(request.action.as_str(), Some(request.input.clone()))
        .map(|payload| OpenClawExecutionResult {
            output: payload,
            summary: None,
        })
        .map_err(map_gateway_failure)
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

fn filesystem_scan_output(history: &Value, path: &str) -> Option<Value> {
    let messages = history.get("messages")?.as_array()?;
    let tool_result = messages.iter().rev().find(|message| {
        message.get("role").and_then(Value::as_str) == Some("toolResult")
            && message.get("toolName").and_then(Value::as_str) == Some("exec")
            && message.get("isError").and_then(Value::as_bool) != Some(true)
    })?;
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn filesystem_read_output(history: &Value, path: &str) -> Option<Value> {
    let messages = history.get("messages")?.as_array()?;
    let tool_result = messages.iter().rev().find(|message| {
        message.get("role").and_then(Value::as_str) == Some("toolResult")
            && message.get("toolName").and_then(Value::as_str) == Some("exec")
            && message.get("isError").and_then(Value::as_bool) != Some(true)
    })?;
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
    let messages = history.get("messages")?.as_array()?;
    let text = messages
        .iter()
        .rev()
        .find(|message| {
            message.get("role").and_then(Value::as_str) == Some("toolResult")
                && message.get("toolName").and_then(Value::as_str) == Some("exec")
                && message.get("isError").and_then(Value::as_bool) != Some(true)
        })?
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
    fn filesystem_scan_rejects_command_error_reported_as_successful_tool_result() {
        let history = json!({"messages": [{
            "role": "toolResult",
            "toolName": "exec",
            "isError": false,
            "content": [{"type": "text", "text": "find: -printf: unknown primary or operator"}]
        }]});

        assert_eq!(filesystem_scan_output(&history, "/safe/example"), None);
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
        assert_eq!(shell_quote("/safe/O'Reilly.txt"), "'/safe/O'\\''Reilly.txt'");
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
            let error = execute_with_invoker(
                &invoker,
                &request("filesystem.write", input),
                &mut |_| {},
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                OpenClawExecutionErrorKind::InvalidRequest,
                "{expected_message}"
            );
            assert_eq!(error.message, expected_message);
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
        assert_eq!(error.message, "Safe Gateway failure.");
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
}
