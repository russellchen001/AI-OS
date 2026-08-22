use super::provider::{FileProvider, FileRequest, FileResponse};
use crate::runtime::openclaw_execution::{OpenClawExecutionAdapter, OpenClawExecutionRequest};
use crate::runtime::openclaw_gateway_adapter::OpenClawGatewayExecutionAdapter;

pub struct OpenClawFilesystemProvider;

impl FileProvider for OpenClawFilesystemProvider {
    fn id(&self) -> &'static str {
        "openclaw-filesystem"
    }

    fn execute(&self, request: FileRequest) -> Result<FileResponse, String> {
        let adapter = OpenClawGatewayExecutionAdapter;

        let execution_request = OpenClawExecutionRequest::new(
            "filesystem-provider-runtime",
            request.capability,
            request.input,
        )
        .map_err(|error| error.message)?;

        let mut progress = |_progress| {};

        let result = adapter
            .execute(&execution_request, &mut progress)
            .map_err(|error| error.message)?;

        Ok(FileResponse {
            output: result.output,
        })
    }
}

pub fn list_filesystem_providers() -> Vec<&'static str> {
    vec!["openclaw-filesystem"]
}

pub fn resolve_filesystem_provider(id: &str) -> Option<Box<dyn FileProvider>> {
    match id {
        "openclaw-filesystem" => Some(Box::new(OpenClawFilesystemProvider)),

        _ => None,
    }
}
