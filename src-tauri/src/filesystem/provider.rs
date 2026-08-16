use serde_json::Value;

pub struct FileRequest {
    pub capability: String,
    pub input: Value,
}

pub struct FileResponse {
    pub output: Value,
}

pub trait FileProvider: Send + Sync {
    fn id(&self) -> &'static str;

    fn execute(
        &self,
        request: FileRequest,
    ) -> Result<FileResponse, String>;
}
