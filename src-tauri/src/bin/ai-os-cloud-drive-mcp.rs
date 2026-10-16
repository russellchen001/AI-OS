use dashboard_lib::cloud_drive_core::{
    destination_path, download_file, select_share_files, DownloadDescriptor, ShareEntry, ShareLink,
    ShareSelection,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

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
                "select_share_files"
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
