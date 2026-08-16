use super::provider::{
    BrowserProvider,
    BrowserRequest,
    BrowserResponse,
};

use crate::mcp_runtime::call_mcp_tool;

use serde_json::Value;


struct McpBrowserProvider;


impl BrowserProvider for McpBrowserProvider {

    fn id(&self) -> &str {
        "mcp-browser"
    }


    fn execute(
        &self,
        request: BrowserRequest,
    ) -> Result<BrowserResponse, String> {

        let command = request
            .input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("python3")
            .to_string();

        let args = request
            .input
            .get("args")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();


        let result = call_mcp_tool(
            command,
            args,
            request.action.clone(),
            request.input.clone(),
        )?;


        Ok(BrowserResponse {
            provider: self.id().to_string(),
            output: serde_json::to_value(result).map_err(|error| error.to_string())?,
        })
    }
}


pub fn resolve_browser_provider(
    id: &str,
) -> Option<Box<dyn BrowserProvider>> {

    match id {
        "mcp-browser" =>
            Some(Box::new(McpBrowserProvider)),

        _ => None,
    }
}


pub fn list_browser_providers() -> Vec<String> {
    vec![
        "mcp-browser".to_string(),
    ]
}
