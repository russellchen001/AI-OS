use super::{
    provider::{BrowserRequest, BrowserResponse},
    registry::resolve_browser_provider,
};

use serde_json::Value;

pub fn execute_browser_capability(capability: &str, input: Value) -> Result<Value, String> {
    let provider = resolve_browser_provider("mcp-browser")
        .ok_or_else(|| "No browser provider available.".to_string())?;

    let action = capability.split('.').last().unwrap_or("search").to_string();

    let response = provider.execute(BrowserRequest { action, input })?;

    Ok(serde_json::json!({
        "provider": response.provider,
        "output": response.output
    }))
}
