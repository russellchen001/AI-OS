use tauri::{AppHandle, State};

use super::{
    executor::{start_accepted_operation, RuntimeExecutionState},
    lifecycle::{validate_runtime_lifecycle_request, RuntimeLifecycleRequest},
    models::{NormalizedRuntimeError, RuntimeOperationAdmission, RuntimeOperationSnapshot},
};

#[tauri::command]
pub(crate) fn start_runtime_operation(
    app: AppHandle,
    state: State<'_, RuntimeExecutionState>,
    request: RuntimeLifecycleRequest,
) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
    let validated = validate_runtime_lifecycle_request(request)?;
    start_accepted_operation(state.manager(), validated, app)
}

#[tauri::command]
pub(crate) fn get_runtime_operation(
    state: State<'_, RuntimeExecutionState>,
    operation_id: String,
) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
    state.manager().get_operation(&operation_id)
}

#[tauri::command]
pub(crate) fn cancel_runtime_operation(
    state: State<'_, RuntimeExecutionState>,
    operation_id: String,
) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
    state.manager().request_cancellation(&operation_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::runtime::models::{RuntimeErrorCode, RuntimeOperationAction};

    #[test]
    fn request_uses_canonical_camel_case_serialization() {
        let request: RuntimeLifecycleRequest = serde_json::from_value(json!({
            "runtimeId": "ollama",
            "action": "open",
            "endpointUrl": "http://localhost:11434"
        }))
        .unwrap();
        assert_eq!(request.runtime_id, "ollama");
        assert_eq!(request.action, RuntimeOperationAction::Open);
        assert!(format!("{request:?}").contains("endpoint_present: true"));
        assert!(!format!("{request:?}").contains("localhost"));
    }

    #[test]
    fn invalid_preflight_is_canonical_and_contains_no_sensitive_endpoint() {
        let request: RuntimeLifecycleRequest = serde_json::from_value(json!({
            "runtimeId": "ollama",
            "action": "open",
            "endpointUrl": "https://user:secret@example.com/private?token=secret"
        }))
        .unwrap();
        let error = validate_runtime_lifecycle_request(request).unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::InvalidConfiguration);
        let serialized = serde_json::to_string(&error).unwrap();
        for forbidden in ["secret", "example.com", "token", "/private"] {
            assert!(!serialized.contains(forbidden));
        }
    }
}
