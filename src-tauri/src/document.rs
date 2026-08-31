pub(crate) mod keynote;
pub(crate) mod provider;
pub(crate) mod registry;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DocumentResult {
    pub success: bool,
    pub path: Option<String>,
    pub mime: Option<String>,
    pub message: String,
}

pub(crate) fn create_document_result(
    path: Option<String>,
    mime: Option<String>,
    message: String,
) -> DocumentResult {
    DocumentResult {
        success: true,
        path,
        mime,
        message,
    }
}

#[tauri::command]
pub(crate) fn list_document_capabilities() -> DocumentResult {
    create_document_result(
        None,
        None,
        "Document skill foundation ready: document.read, document.create, document.convert"
            .to_owned(),
    )
}
