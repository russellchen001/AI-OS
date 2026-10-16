use dashboard_lib::cloud_drive_baidu::BaiduResolvedShare;
use dashboard_lib::cloud_drive_core::{
    destination_path, download_file, download_file_with_client, select_share_files,
    DownloadDescriptor, ShareEntry, ShareLink, ShareSelection,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

static BAIDU_SESSIONS: OnceLock<Mutex<HashMap<String, BaiduResolvedShare>>> = OnceLock::new();

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line);
        if serde_json::to_writer(&mut stdout, &response).is_err()
            || stdout.write_all(b"\n").is_err()
            || stdout.flush().is_err()
        {
            break;
        }
    }
}

fn handle_line(line: &str) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return rpc_error(Value::Null, -32700, "Parse error"),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "ai-os-cloud-drive-mcp", "version": "0.1.0" }
            }
        }),
        "notifications/initialized" => Value::Null,
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": tools() }
        }),
        "tools/call" => call_tool(id, request.get("params").cloned().unwrap_or_default()),
        _ => rpc_error(id, -32601, "Method not found"),
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "inspect_share_link",
            "description": "Recognize one of the nine supported cloud-drive share links and normalize its extraction code without opening an account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "extractionCode": { "type": "string" }
                },
                "required": ["url"]
            }
        },
        {
            "name": "download_resolved_file",
            "description": "Download one provider-resolved HTTPS file to an absolute Mac or mounted NAS directory with bounded retry and integrity verification.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "descriptor": { "type": "object" },
                    "destination": { "type": "string" },
                    "retryLimit": { "type": "integer", "minimum": 1, "maximum": 5 }
                },
                "required": ["descriptor", "destination"]
            }
        },
        {
            "name": "select_share_files",
            "description": "Select files from a provider-normalized share tree by all items, stable item ids, or a name hint; selecting a folder expands its files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entries": { "type": "array" },
                    "selection": { "type": "object" }
                },
                "required": ["entries", "selection"]
            }
        },
        {
            "name": "resolve_baidu_share",
            "description": "Open a Baidu share with its extraction code, recursively list the real share tree and retain authorization only in process memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "extractionCode": { "type": "string" }
                },
                "required": ["url", "extractionCode"]
            }
        },
        {
            "name": "download_baidu_share_file",
            "description": "Resolve one listed Baidu share file to a private descriptor and download it through the AI-OS native HTTP engine.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "itemId": { "type": "string" },
                    "destination": { "type": "string" },
                    "retryLimit": { "type": "integer", "minimum": 1, "maximum": 5 }
                },
                "required": ["sessionId", "itemId", "destination"]
            }
        },
        {
            "name": "download_authorized_baidu_share_file",
            "description": "Use a backend-supplied, memory-only Baidu authorization to resolve a share and download one real file through the native HTTP engine in a single MCP call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "extractionCode": { "type": "string" },
                    "authorizationCookie": { "type": "string" },
                    "itemId": { "type": "string" },
                    "destination": { "type": "string" },
                    "retryLimit": { "type": "integer", "minimum": 1, "maximum": 5 }
                },
                "required": ["url", "extractionCode", "authorizationCookie", "itemId", "destination"]
            }
        }
    ])
}

fn call_tool(id: Value, params: Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or_default();
    let result = match name {
        "inspect_share_link" => inspect_share_link(arguments),
        "download_resolved_file" => download(arguments),
        "select_share_files" => select_files(arguments),
        "resolve_baidu_share" => resolve_baidu_share(arguments),
        "download_baidu_share_file" => download_baidu_share_file(arguments),
        "download_authorized_baidu_share_file" => download_authorized_baidu_share_file(arguments),
        _ => Err("Requested Cloud Drive MCP tool is not allowed.".to_owned()),
    };
    match result {
        Ok(value) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": value.to_string() }] }
        }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": true,
                "content": [{ "type": "text", "text": error }]
            }
        }),
    }
}

fn select_files(arguments: Value) -> Result<Value, String> {
    let entries: Vec<ShareEntry> = serde_json::from_value(
        arguments
            .get("entries")
            .cloned()
            .ok_or_else(|| "Normalized share entries are required.".to_owned())?,
    )
    .map_err(|_| "Normalized share entries are malformed.".to_owned())?;
    let selection: ShareSelection = serde_json::from_value(
        arguments
            .get("selection")
            .cloned()
            .ok_or_else(|| "Share selection is required.".to_owned())?,
    )
    .map_err(|_| "Share selection is malformed.".to_owned())?;
    serde_json::to_value(select_share_files(&entries, &selection)?)
        .map_err(|error| format!("Unable to serialize selected files: {error}"))
}

fn inspect_share_link(arguments: Value) -> Result<Value, String> {
    let url = arguments
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Share URL is required.".to_owned())?;
    let code = arguments.get("extractionCode").and_then(Value::as_str);
    serde_json::to_value(ShareLink::parse(url, code)?)
        .map_err(|error| format!("Unable to serialize share link: {error}"))
}

fn download(arguments: Value) -> Result<Value, String> {
    let descriptor: DownloadDescriptor = serde_json::from_value(
        arguments
            .get("descriptor")
            .cloned()
            .ok_or_else(|| "Download descriptor is required.".to_owned())?,
    )
    .map_err(|_| "Download descriptor is malformed.".to_owned())?;
    let destination = arguments
        .get("destination")
        .and_then(Value::as_str)
        .ok_or_else(|| "Download destination is required.".to_owned())?;
    let retry_limit = arguments
        .get("retryLimit")
        .and_then(Value::as_u64)
        .unwrap_or(3) as u8;
    serde_json::to_value(download_file(
        &descriptor,
        &destination_path(destination)?,
        retry_limit,
    )?)
    .map_err(|error| format!("Unable to serialize download receipt: {error}"))
}

fn resolve_baidu_share(arguments: Value) -> Result<Value, String> {
    let url = arguments
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Baidu share URL is required.".to_owned())?;
    let code = arguments
        .get("extractionCode")
        .and_then(Value::as_str)
        .ok_or_else(|| "Baidu extraction code is required.".to_owned())?;
    let resolved = BaiduResolvedShare::resolve(ShareLink::parse(url, Some(code))?)?;
    let entries = serde_json::to_value(resolved.entries())
        .map_err(|error| format!("Unable to serialize Baidu share tree: {error}"))?;
    let session_id = Uuid::new_v4().to_string();
    let sessions = BAIDU_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = sessions
        .lock()
        .map_err(|_| "Baidu session store is unavailable.".to_owned())?;
    if sessions.len() >= 8 {
        sessions.clear();
    }
    sessions.insert(session_id.clone(), resolved);
    Ok(json!({
        "provider": "baidu",
        "sessionId": session_id,
        "entries": entries,
        "authorizationStorage": "process-memory-only"
    }))
}

fn download_baidu_share_file(arguments: Value) -> Result<Value, String> {
    let session_id = arguments
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Baidu session id is required.".to_owned())?;
    let item_id = arguments
        .get("itemId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Baidu item id is required.".to_owned())?;
    let destination = arguments
        .get("destination")
        .and_then(Value::as_str)
        .ok_or_else(|| "Download destination is required.".to_owned())?;
    let retry_limit = arguments
        .get("retryLimit")
        .and_then(Value::as_u64)
        .unwrap_or(3) as u8;
    let sessions = BAIDU_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let sessions = sessions
        .lock()
        .map_err(|_| "Baidu session store is unavailable.".to_owned())?;
    let resolved = sessions
        .get(session_id)
        .ok_or_else(|| "Baidu session expired; resolve the share again.".to_owned())?;
    let descriptor = resolved.descriptor(item_id)?;
    let descriptor_summary = json!({
        "provider": descriptor.provider,
        "fileName": descriptor.file_name.clone(),
        "expectedSize": descriptor.expected_size,
        "authorization": "process-memory-only"
    });
    let receipt = download_file_with_client(
        resolved.client(),
        &descriptor,
        &destination_path(destination)?,
        retry_limit,
        |_| {},
    )?;
    Ok(json!({ "descriptor": descriptor_summary, "receipt": receipt }))
}

fn download_authorized_baidu_share_file(arguments: Value) -> Result<Value, String> {
    let required = |name: &str| {
        arguments
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Baidu {name} is required."))
    };
    let share = ShareLink::parse(required("url")?, Some(required("extractionCode")?))?;
    let resolved = BaiduResolvedShare::resolve_with_authorization(
        share,
        Some(required("authorizationCookie")?),
    )?;
    let descriptor = resolved.descriptor(required("itemId")?)?;
    let destination = destination_path(required("destination")?)?;
    let retry_limit = arguments
        .get("retryLimit")
        .and_then(Value::as_u64)
        .unwrap_or(3) as u8;
    let descriptor_summary = json!({
        "provider": descriptor.provider,
        "fileName": descriptor.file_name.clone(),
        "expectedSize": descriptor.expected_size,
        "authorization": "process-memory-only"
    });
    let receipt = download_file_with_client(
        resolved.client(),
        &descriptor,
        &destination,
        retry_limit,
        |_| {},
    )?;
    Ok(json!({
        "entries": resolved.entries(),
        "descriptor": descriptor_summary,
        "receipt": receipt
    }))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_bounded_download_tools() {
        let response = handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        let names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "inspect_share_link",
                "download_resolved_file",
                "select_share_files",
                "resolve_baidu_share",
                "download_baidu_share_file",
                "download_authorized_baidu_share_file"
            ]
        );
    }

    #[test]
    fn inspect_tool_returns_provider_without_account_management() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspect_share_link","arguments":{"url":"https://pan.quark.cn/s/SafeShare_2"}}}"#,
        );
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("quark"));
        assert!(!text.contains("upload"));
    }
}
