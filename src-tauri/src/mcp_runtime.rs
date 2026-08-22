use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResponse {
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

fn spawn_stdio_server(command: &str, args: &[String]) -> Result<std::process::Child, String> {
    Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to start MCP server {}: {}", command, error))
}

fn send_json_rpc(command: &str, args: &[String], request: Value) -> Result<Value, String> {
    let mut child = spawn_stdio_server(command, args)?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "MCP stdin unavailable.".to_string())?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "MCP stdout unavailable.".to_string())?;

    let payload = serde_json::to_string(&request).map_err(|error| error.to_string())?;

    writeln!(stdin, "{}", payload).map_err(|error| error.to_string())?;

    drop(stdin);

    let mut reader = BufReader::new(stdout);

    let mut line = String::new();

    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;

    serde_json::from_str(&line).map_err(|error| format!("Invalid MCP response: {}", error))
}

#[tauri::command]
pub fn list_mcp_tools(command: String, args: Vec<String>) -> Result<Vec<McpTool>, String> {
    let response = send_json_rpc(
        &command,
        &args,
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/list",
            "params":{}
        }),
    )?;

    serde_json::from_value(response["result"]["tools"].clone()).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn call_mcp_tool(
    command: String,
    args: Vec<String>,
    tool_name: String,
    arguments: Value,
) -> Result<McpResponse, String> {
    match send_json_rpc(
        &command,
        &args,
        json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"tools/call",
            "params":{
                "name":tool_name,
                "arguments":arguments
            }
        }),
    ) {
        Ok(result) => Ok(McpResponse {
            success: true,
            result: Some(result),
            error: None,
        }),

        Err(error) => Ok(McpResponse {
            success: false,
            result: None,
            error: Some(error),
        }),
    }
}
