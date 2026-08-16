use serde_json::Value;

#[derive(Debug, Clone)]
pub struct BrowserRequest {
    pub action: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct BrowserResponse {
    pub provider: String,
    pub output: Value,
}

pub trait BrowserProvider: Send + Sync {
    fn id(&self) -> &str;

    fn execute(
        &self,
        request: BrowserRequest,
    ) -> Result<BrowserResponse, String>;
}
