use std::{
    collections::HashSet,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};

use serde::Deserialize;
use tauri::{AppHandle, State};

use super::{
    executor::{
        emit_best_effort, execute_validated_request, RuntimeExecutionState, TauriEventEmitter,
    },
    lifecycle::{validate_runtime_lifecycle_request, RuntimeLifecycleRequest},
    models::{
        NormalizedRuntimeError, RuntimeBulkOutcome, RuntimeBulkResult, RuntimeErrorCode,
        RuntimeOperationAction, RuntimeOperationAdmission, RuntimeOperationProgress,
        RuntimeOperationResult, RuntimeOperationState,
    },
    operations::{RuntimeOperationManager, RuntimeOperationProgressUpdate, RUNTIME_BULK_ID},
};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeBulkItemRequest {
    runtime_id: String,
    endpoint_url: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartRuntimeBulkOperationRequest {
    action: RuntimeOperationAction,
    runtimes: Vec<RuntimeBulkItemRequest>,
}

fn invalid_request(message: &str) -> NormalizedRuntimeError {
    NormalizedRuntimeError {
        code: RuntimeErrorCode::InvalidConfiguration,
        message: message.to_string(),
        retryable: false,
    }
}

fn validate_request(
    request: &StartRuntimeBulkOperationRequest,
) -> Result<(), NormalizedRuntimeError> {
    if !matches!(
        request.action,
        RuntimeOperationAction::Start | RuntimeOperationAction::Stop
    ) {
        return Err(invalid_request(
            "Bulk operations support start and stop only.",
        ));
    }
    if request.runtimes.is_empty() {
        return Err(invalid_request(
            "A bulk operation requires at least one runtime.",
        ));
    }
    let mut ids = HashSet::new();
    if request
        .runtimes
        .iter()
        .any(|runtime| !ids.insert(runtime.runtime_id.as_str()))
    {
        return Err(invalid_request(
            "A runtime may appear only once in a bulk operation.",
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn start_runtime_bulk_operation(
    app: AppHandle,
    state: State<'_, RuntimeExecutionState>,
    request: StartRuntimeBulkOperationRequest,
) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
    validate_request(&request)?;
    let manager = state.manager();
    let admission = manager.admit_operation(RUNTIME_BULK_ID, request.action, false)?;
    let queued = match admission {
        RuntimeOperationAdmission::Accepted { ref operation } => operation.clone(),
        other => return Ok(other),
    };
    emit_best_effort(&TauriEventEmitter::new(app.clone()), queued.clone());
    let operation_id = queued.operation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emitter = TauriEventEmitter::new(app);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            run_bulk(&manager, &operation_id, request, &emitter)
        }));
        if outcome.is_err() {
            let error = NormalizedRuntimeError {
                code: RuntimeErrorCode::OperationTaskFailed,
                message: "The runtime operation task failed.".to_string(),
                retryable: true,
            };
            if let Ok(snapshot) = manager.transition(
                &operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(error),
            ) {
                emit_best_effort(&emitter, snapshot);
            }
        }
    });
    Ok(RuntimeOperationAdmission::Accepted { operation: queued })
}

fn run_bulk(
    manager: &Arc<RuntimeOperationManager>,
    operation_id: &str,
    request: StartRuntimeBulkOperationRequest,
    emitter: &TauriEventEmitter,
) {
    let Ok(running) = manager.transition(operation_id, RuntimeOperationState::Running, None, None)
    else {
        return;
    };
    emit_best_effort(emitter, running);

    let total = request.runtimes.len() as u32;
    let mut outcomes = Vec::with_capacity(request.runtimes.len());
    for (index, item) in request.runtimes.into_iter().enumerate() {
        let lifecycle_request = RuntimeLifecycleRequest {
            runtime_id: item.runtime_id.clone(),
            action: request.action,
            endpoint_url: item.endpoint_url,
        };
        let execution = validate_runtime_lifecycle_request(lifecycle_request)
            .and_then(|validated| execute_validated_request(&validated, &mut |_| {}));
        outcomes.push(RuntimeBulkOutcome {
            runtime_id: item.runtime_id,
            succeeded: execution.is_ok(),
            error: execution.err(),
        });
        let progress = RuntimeOperationProgress {
            phase: "executing".to_string(),
            completed_units: Some(index as u32 + 1),
            total_units: Some(total),
            message: "Runtime bulk operation is in progress.".to_string(),
        };
        if let Ok(RuntimeOperationProgressUpdate::Applied(snapshot)) =
            manager.update_progress(operation_id, progress)
        {
            emit_best_effort(emitter, snapshot);
        }
    }

    let failed = outcomes.iter().filter(|outcome| !outcome.succeeded).count() as u32;
    let result = RuntimeBulkResult {
        total,
        succeeded: total - failed,
        failed,
        outcomes,
    };
    if let Ok(snapshot) = manager.transition(
        operation_id,
        RuntimeOperationState::Succeeded,
        Some(RuntimeOperationResult {
            message: "Runtime bulk operation completed.".to_string(),
            bulk: Some(result),
        }),
        None,
    ) {
        emit_best_effort(emitter, snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(ids: &[&str]) -> StartRuntimeBulkOperationRequest {
        StartRuntimeBulkOperationRequest {
            action: RuntimeOperationAction::Start,
            runtimes: ids
                .iter()
                .map(|id| RuntimeBulkItemRequest {
                    runtime_id: (*id).to_string(),
                    endpoint_url: None,
                })
                .collect(),
        }
    }

    #[test]
    fn rejects_empty_requests() {
        assert_eq!(
            validate_request(&request(&[])).unwrap_err().code,
            RuntimeErrorCode::InvalidConfiguration
        );
    }

    #[test]
    fn rejects_duplicate_runtime_ids() {
        assert_eq!(
            validate_request(&request(&["ollama", "ollama"]))
                .unwrap_err()
                .code,
            RuntimeErrorCode::InvalidConfiguration
        );
    }

    #[test]
    fn accepts_ordered_unique_runtime_ids() {
        assert!(validate_request(&request(&["docker-desktop", "ollama"])).is_ok());
    }
}
