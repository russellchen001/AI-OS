use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::models::{
    NormalizedRuntimeError, RuntimeErrorCode, RuntimeOperationAction, RuntimeOperationAdmission,
    RuntimeOperationProgress, RuntimeOperationResult, RuntimeOperationSnapshot,
    RuntimeOperationState,
};
use super::registry;

const TERMINAL_RETENTION_MINUTES: i64 = 30;
const MAX_TERMINAL_OPERATIONS: usize = 200;
pub(crate) const MAX_ACTIVE_OPERATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeOperationProgressUpdate {
    Applied(RuntimeOperationSnapshot),
    Unchanged(RuntimeOperationSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeOperationCancellationUpdate {
    Applied(RuntimeOperationSnapshot),
    Unchanged(RuntimeOperationSnapshot),
}

impl RuntimeOperationCancellationUpdate {
    pub(crate) fn operation(&self) -> &RuntimeOperationSnapshot {
        match self {
            Self::Applied(operation) | Self::Unchanged(operation) => operation,
        }
    }
}

impl RuntimeOperationProgressUpdate {
    pub(crate) fn operation(&self) -> &RuntimeOperationSnapshot {
        match self {
            Self::Applied(operation) | Self::Unchanged(operation) => operation,
        }
    }
}

#[derive(Default)]
struct OperationStore {
    operations: HashMap<String, RuntimeOperationSnapshot>,
    lifecycle_slots: HashMap<String, String>,
    terminal_order: HashMap<String, u64>,
    next_terminal_order: u64,
}

pub struct RuntimeOperationManager {
    store: Mutex<OperationStore>,
}

impl Default for RuntimeOperationManager {
    fn default() -> Self {
        Self {
            store: Mutex::new(OperationStore::default()),
        }
    }
}

#[allow(dead_code)]
impl RuntimeOperationManager {
    pub fn admit_operation(
        &self,
        runtime_id: &str,
        action: RuntimeOperationAction,
        cancellable: bool,
    ) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
        self.admit_operation_at(runtime_id, action, cancellable, Utc::now())
    }

    pub fn create_operation(
        &self,
        runtime_id: &str,
        action: RuntimeOperationAction,
        cancellable: bool,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        match self.admit_operation(runtime_id, action, cancellable)? {
            RuntimeOperationAdmission::Accepted { operation } => Ok(operation),
            RuntimeOperationAdmission::Conflict { .. } => Err(safe_error(
                RuntimeErrorCode::OperationConflict,
                "A lifecycle operation is already active for this runtime.",
                true,
            )),
            RuntimeOperationAdmission::Rejected { error } => Err(error),
        }
    }

    pub fn get_operation(
        &self,
        operation_id: &str,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        self.get_operation_at(operation_id, Utc::now())
    }

    pub fn transition(
        &self,
        operation_id: &str,
        state: RuntimeOperationState,
        result: Option<RuntimeOperationResult>,
        error: Option<NormalizedRuntimeError>,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        self.transition_at(operation_id, state, result, error, Utc::now())
    }

    pub fn update_progress(
        &self,
        operation_id: &str,
        progress: RuntimeOperationProgress,
    ) -> Result<RuntimeOperationProgressUpdate, NormalizedRuntimeError> {
        self.update_progress_at(operation_id, progress, Utc::now())
    }

    pub fn request_cancellation(
        &self,
        operation_id: &str,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        self.request_cancellation_update(operation_id)
            .map(|update| update.operation().clone())
    }

    pub(crate) fn request_cancellation_update(
        &self,
        operation_id: &str,
    ) -> Result<RuntimeOperationCancellationUpdate, NormalizedRuntimeError> {
        self.request_cancellation_update_at(operation_id, Utc::now())
    }

    fn create_operation_at(
        &self,
        runtime_id: &str,
        action: RuntimeOperationAction,
        cancellable: bool,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        match self.admit_operation_at(runtime_id, action, cancellable, now)? {
            RuntimeOperationAdmission::Accepted { operation } => Ok(operation),
            RuntimeOperationAdmission::Conflict { .. } => Err(safe_error(
                RuntimeErrorCode::OperationConflict,
                "A lifecycle operation is already active for this runtime.",
                true,
            )),
            RuntimeOperationAdmission::Rejected { error } => Err(error),
        }
    }

    fn admit_operation_at(
        &self,
        runtime_id: &str,
        action: RuntimeOperationAction,
        cancellable: bool,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
        if !registry::contains_id(runtime_id) {
            return Err(operation_runtime_not_found());
        }

        let mut store = self.lock_store()?;
        cleanup(&mut store, now);

        if action.reserves_lifecycle_slot() {
            if let Some(operation_id) = store.lifecycle_slots.get(runtime_id).cloned() {
                let conflict = store.operations.get(&operation_id).filter(|snapshot| {
                    snapshot.runtime_id == runtime_id
                        && snapshot.action.reserves_lifecycle_slot()
                        && !snapshot.state.is_terminal()
                });
                if let Some(existing_operation) = conflict.cloned() {
                    return Ok(RuntimeOperationAdmission::Conflict { existing_operation });
                }
                store.lifecycle_slots.remove(runtime_id);
            }
        }

        if active_operation_count(&store) >= MAX_ACTIVE_OPERATIONS {
            return Ok(RuntimeOperationAdmission::Rejected {
                error: safe_error(
                    RuntimeErrorCode::OperationCapacityExceeded,
                    "Runtime operation capacity has been reached.",
                    true,
                ),
            });
        }

        let operation_id = Uuid::new_v4().to_string();
        let timestamp = now.to_rfc3339();
        let snapshot = RuntimeOperationSnapshot {
            operation_id: operation_id.clone(),
            runtime_id: runtime_id.to_string(),
            action,
            state: RuntimeOperationState::Queued,
            revision: 1,
            accepted_at: timestamp.clone(),
            started_at: None,
            updated_at: timestamp,
            completed_at: None,
            progress: None,
            cancellable,
            result: None,
            error: None,
        };

        store
            .operations
            .insert(operation_id.clone(), snapshot.clone());
        if action.reserves_lifecycle_slot() {
            store
                .lifecycle_slots
                .insert(runtime_id.to_string(), operation_id);
        }

        Ok(RuntimeOperationAdmission::Accepted {
            operation: snapshot,
        })
    }

    fn get_operation_at(
        &self,
        operation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        let mut store = self.lock_store()?;
        cleanup(&mut store, now);
        find_operation(&store, operation_id)
    }

    fn transition_at(
        &self,
        operation_id: &str,
        state: RuntimeOperationState,
        result: Option<RuntimeOperationResult>,
        error: Option<NormalizedRuntimeError>,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        if state == RuntimeOperationState::Cancelling {
            return Err(unsupported_transition());
        }

        let mut store = self.lock_store()?;
        apply_transition(&mut store, operation_id, state, result, error, now)?;
        cleanup(&mut store, now);
        find_operation(&store, operation_id)
    }

    fn update_progress_at(
        &self,
        operation_id: &str,
        progress: RuntimeOperationProgress,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationProgressUpdate, NormalizedRuntimeError> {
        let mut store = self.lock_store()?;
        cleanup(&mut store, now);
        let snapshot = store
            .operations
            .get_mut(operation_id)
            .ok_or_else(operation_not_found)?;

        if snapshot.state.is_terminal() {
            return Err(unsupported_transition());
        }

        if snapshot.progress.as_ref() == Some(&progress) {
            return Ok(RuntimeOperationProgressUpdate::Unchanged(snapshot.clone()));
        }

        snapshot.progress = Some(progress);
        snapshot.updated_at = now.to_rfc3339();
        snapshot.revision += 1;
        Ok(RuntimeOperationProgressUpdate::Applied(snapshot.clone()))
    }

    fn request_cancellation_at(
        &self,
        operation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
        self.request_cancellation_update_at(operation_id, now)
            .map(|update| update.operation().clone())
    }

    fn request_cancellation_update_at(
        &self,
        operation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<RuntimeOperationCancellationUpdate, NormalizedRuntimeError> {
        let mut store = self.lock_store()?;
        cleanup(&mut store, now);
        let snapshot = find_operation(&store, operation_id)?;

        match snapshot.state {
            RuntimeOperationState::Cancelled | RuntimeOperationState::Cancelling => {
                return Ok(RuntimeOperationCancellationUpdate::Unchanged(snapshot));
            }
            RuntimeOperationState::Succeeded | RuntimeOperationState::Failed => {
                return Err(safe_error(
                    RuntimeErrorCode::CancellationTooLate,
                    "The operation has already completed.",
                    false,
                ));
            }
            RuntimeOperationState::Queued | RuntimeOperationState::Running => {}
        }

        if !snapshot.cancellable {
            return Err(safe_error(
                RuntimeErrorCode::CancellationUnsupported,
                "This operation cannot be cancelled.",
                false,
            ));
        }

        apply_transition(
            &mut store,
            operation_id,
            RuntimeOperationState::Cancelling,
            None,
            None,
            now,
        )
        .map(RuntimeOperationCancellationUpdate::Applied)
    }

    fn lock_store(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, OperationStore>, NormalizedRuntimeError> {
        self.store.lock().map_err(|_| {
            safe_error(
                RuntimeErrorCode::OperationTaskFailed,
                "Runtime operation state is unavailable.",
                true,
            )
        })
    }
}

fn apply_transition(
    store: &mut OperationStore,
    operation_id: &str,
    next: RuntimeOperationState,
    result: Option<RuntimeOperationResult>,
    error: Option<NormalizedRuntimeError>,
    now: DateTime<Utc>,
) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
    let snapshot = store
        .operations
        .get_mut(operation_id)
        .ok_or_else(operation_not_found)?;

    if !legal_transition(snapshot.state, next) || !valid_payload(next, &result, &error) {
        return Err(unsupported_transition());
    }

    let timestamp = now.to_rfc3339();
    snapshot.state = next;
    snapshot.revision += 1;
    snapshot.updated_at = timestamp.clone();
    snapshot.result = result;
    snapshot.error = error;

    if next == RuntimeOperationState::Running && snapshot.started_at.is_none() {
        snapshot.started_at = Some(timestamp.clone());
    }
    if next.is_terminal() {
        snapshot.completed_at = Some(timestamp);
        snapshot.cancellable = false;
        let terminal_order = store.next_terminal_order;
        store.next_terminal_order = store.next_terminal_order.saturating_add(1);
        store
            .terminal_order
            .insert(snapshot.operation_id.clone(), terminal_order);
        if snapshot.action.reserves_lifecycle_slot()
            && store.lifecycle_slots.get(&snapshot.runtime_id) == Some(&snapshot.operation_id)
        {
            store.lifecycle_slots.remove(&snapshot.runtime_id);
        }
    }

    Ok(snapshot.clone())
}

fn legal_transition(current: RuntimeOperationState, next: RuntimeOperationState) -> bool {
    use RuntimeOperationState::{Cancelled, Cancelling, Failed, Queued, Running, Succeeded};

    matches!(
        (current, next),
        (Queued, Running)
            | (Queued, Cancelling)
            | (Queued, Failed)
            | (Running, Cancelling)
            | (Running, Succeeded)
            | (Running, Failed)
            | (Cancelling, Cancelled)
            | (Cancelling, Succeeded)
            | (Cancelling, Failed)
    )
}

fn valid_payload(
    state: RuntimeOperationState,
    result: &Option<RuntimeOperationResult>,
    error: &Option<NormalizedRuntimeError>,
) -> bool {
    match state {
        RuntimeOperationState::Succeeded => result.is_some() && error.is_none(),
        RuntimeOperationState::Failed => result.is_none() && error.is_some(),
        RuntimeOperationState::Cancelled
        | RuntimeOperationState::Queued
        | RuntimeOperationState::Running
        | RuntimeOperationState::Cancelling => result.is_none() && error.is_none(),
    }
}

fn cleanup(store: &mut OperationStore, now: DateTime<Utc>) {
    let expiry = now - Duration::minutes(TERMINAL_RETENTION_MINUTES);
    store.operations.retain(|_, snapshot| {
        if !snapshot.state.is_terminal() {
            return true;
        }

        snapshot
            .completed_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|completed| completed.with_timezone(&Utc) >= expiry)
            .unwrap_or(false)
    });

    store
        .terminal_order
        .retain(|operation_id, _| store.operations.contains_key(operation_id));

    let mut terminal = store
        .operations
        .values()
        .filter(|snapshot| snapshot.state.is_terminal())
        .filter_map(|snapshot| {
            store
                .terminal_order
                .get(&snapshot.operation_id)
                .map(|order| (snapshot.operation_id.clone(), *order))
        })
        .collect::<Vec<_>>();

    if terminal.len() > MAX_TERMINAL_OPERATIONS {
        terminal.sort_by_key(|(_, order)| *order);
        let remove_count = terminal.len() - MAX_TERMINAL_OPERATIONS;
        for (operation_id, _) in terminal.into_iter().take(remove_count) {
            store.operations.remove(&operation_id);
            store.terminal_order.remove(&operation_id);
        }
    }

    store.lifecycle_slots.retain(|runtime_id, operation_id| {
        store.operations.get(operation_id).is_some_and(|snapshot| {
            snapshot.runtime_id == *runtime_id
                && snapshot.action.reserves_lifecycle_slot()
                && !snapshot.state.is_terminal()
        })
    });
}

fn find_operation(
    store: &OperationStore,
    operation_id: &str,
) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
    store
        .operations
        .get(operation_id)
        .cloned()
        .ok_or_else(operation_not_found)
}

fn active_operation_count(store: &OperationStore) -> usize {
    store
        .operations
        .values()
        .filter(|snapshot| !snapshot.state.is_terminal())
        .count()
}

fn operation_runtime_not_found() -> NormalizedRuntimeError {
    safe_error(
        RuntimeErrorCode::RuntimeNotFound,
        "Runtime was not found.",
        false,
    )
}

fn operation_not_found() -> NormalizedRuntimeError {
    safe_error(
        RuntimeErrorCode::OperationNotFound,
        "Runtime operation was not found.",
        false,
    )
}

fn unsupported_transition() -> NormalizedRuntimeError {
    safe_error(
        RuntimeErrorCode::UnsupportedOperation,
        "The requested operation transition is not supported.",
        false,
    )
}

fn safe_error(code: RuntimeErrorCode, message: &str, retryable: bool) -> NormalizedRuntimeError {
    NormalizedRuntimeError {
        code,
        message: message.to_string(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    use serde_json::json;

    use super::*;

    fn time(minutes: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-19T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + Duration::minutes(minutes)
    }

    fn time_milliseconds(milliseconds: i64) -> DateTime<Utc> {
        time(0) + Duration::milliseconds(milliseconds)
    }

    fn result() -> RuntimeOperationResult {
        RuntimeOperationResult {
            message: "Runtime operation completed.".to_string(),
        }
    }

    fn failure() -> NormalizedRuntimeError {
        safe_error(
            RuntimeErrorCode::OperationFailed,
            "Runtime operation failed.",
            true,
        )
    }

    #[test]
    fn every_registered_runtime_id_can_be_admitted() {
        for definition in registry::definitions() {
            let manager = RuntimeOperationManager::default();
            let admission = manager
                .admit_operation(&definition.id, RuntimeOperationAction::Open, false)
                .unwrap();

            assert!(matches!(
                admission,
                RuntimeOperationAdmission::Accepted { operation }
                    if operation.runtime_id == definition.id
            ));
        }
    }

    #[test]
    fn unknown_runtime_id_is_rejected() {
        let error = RuntimeOperationManager::default()
            .admit_operation(
                "definitely-not-a-runtime",
                RuntimeOperationAction::Open,
                false,
            )
            .unwrap_err();

        assert_eq!(error.code, RuntimeErrorCode::RuntimeNotFound);
    }

    #[test]
    fn runtime_id_validation_is_case_sensitive() {
        let manager = RuntimeOperationManager::default();
        assert!(matches!(
            manager
                .admit_operation("ollama", RuntimeOperationAction::Open, false)
                .unwrap(),
            RuntimeOperationAdmission::Accepted { .. }
        ));

        let error = RuntimeOperationManager::default()
            .admit_operation("Ollama", RuntimeOperationAction::Open, false)
            .unwrap_err();
        assert_eq!(error.code, RuntimeErrorCode::RuntimeNotFound);
    }

    #[test]
    fn initial_snapshot_is_queued_at_revision_one() {
        let manager = RuntimeOperationManager::default();
        let snapshot = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();

        assert_eq!(snapshot.state, RuntimeOperationState::Queued);
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.accepted_at, snapshot.updated_at);
        assert!(snapshot.started_at.is_none());
        assert!(snapshot.completed_at.is_none());
        assert!(Uuid::parse_str(&snapshot.operation_id).is_ok());
    }

    #[test]
    fn queued_running_succeeded_preserves_invariants() {
        let manager = RuntimeOperationManager::default();
        let queued = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let running = manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let succeeded = manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Succeeded,
                Some(result()),
                None,
                time(2),
            )
            .unwrap();

        assert_eq!(running.revision, 2);
        assert_eq!(
            running.started_at.as_deref(),
            Some("2026-07-19T00:01:00+00:00")
        );
        assert_eq!(succeeded.revision, 3);
        assert!(succeeded.result.is_some());
        assert!(succeeded.error.is_none());
        assert!(succeeded.completed_at.is_some());
    }

    #[test]
    fn queued_running_failed_preserves_invariants() {
        let manager = RuntimeOperationManager::default();
        let queued = manager
            .create_operation_at(
                "docker-desktop",
                RuntimeOperationAction::Stop,
                false,
                time(0),
            )
            .unwrap();
        manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let failed = manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(2),
            )
            .unwrap();

        assert!(failed.result.is_none());
        assert!(failed.error.is_some());
        assert!(failed.completed_at.is_some());
    }

    #[test]
    fn cancelling_path_is_legal_and_repeated_cancellation_is_idempotent() {
        let manager = RuntimeOperationManager::default();
        let queued = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, true, time(0))
            .unwrap();
        let cancelling = manager
            .request_cancellation_at(&queued.operation_id, time(1))
            .unwrap();
        let repeated = manager
            .request_cancellation_at(&queued.operation_id, time(2))
            .unwrap();
        assert_eq!(cancelling.state, RuntimeOperationState::Cancelling);
        assert_eq!(repeated.revision, cancelling.revision);

        let cancelled = manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Cancelled,
                None,
                None,
                time(3),
            )
            .unwrap();
        let repeated_terminal = manager
            .request_cancellation_at(&queued.operation_id, time(4))
            .unwrap();
        assert_eq!(cancelled, repeated_terminal);
        assert!(manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Succeeded,
                Some(result()),
                None,
                time(5),
            )
            .is_err());
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Stop, false, time(5))
            .unwrap();
    }

    #[test]
    fn direct_cancelling_transition_cannot_bypass_authorization() {
        let manager = RuntimeOperationManager::default();
        let queued = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let queued_before = manager
            .get_operation_at(&queued.operation_id, time(0))
            .unwrap();

        assert!(manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Cancelling,
                None,
                None,
                time(1),
            )
            .is_err());
        assert_eq!(
            manager
                .get_operation_at(&queued.operation_id, time(1))
                .unwrap(),
            queued_before
        );
        assert_eq!(
            manager
                .create_operation_at("ollama", RuntimeOperationAction::Stop, false, time(1))
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationConflict
        );

        let running = manager
            .transition_at(
                &queued.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(2),
            )
            .unwrap();
        let running = manager
            .update_progress_at(
                &running.operation_id,
                RuntimeOperationProgress {
                    phase: "executing".to_string(),
                    completed_units: None,
                    total_units: None,
                    message: "Runtime operation is running.".to_string(),
                },
                time(3),
            )
            .unwrap();

        assert!(manager
            .transition_at(
                &running.operation().operation_id,
                RuntimeOperationState::Cancelling,
                None,
                None,
                time(4),
            )
            .is_err());
        assert_eq!(
            manager
                .get_operation_at(&running.operation().operation_id, time(4))
                .unwrap(),
            running.operation().clone()
        );
        assert_eq!(
            manager
                .create_operation_at("ollama", RuntimeOperationAction::Restart, false, time(4))
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationConflict
        );
    }

    #[test]
    fn request_cancellation_is_the_only_authorized_entry_to_cancelling() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, true, time(0))
            .unwrap();
        assert!(manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Cancelling,
                None,
                None,
                time(1),
            )
            .is_err());

        let cancelling = manager
            .request_cancellation_at(&operation.operation_id, time(1))
            .unwrap();
        assert_eq!(cancelling.state, RuntimeOperationState::Cancelling);
        assert_eq!(cancelling.revision, 2);
    }

    #[test]
    fn cancelling_may_race_to_succeeded_and_releases_slot_once() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, true, time(0))
            .unwrap();
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let cancelling = manager
            .request_cancellation_at(&operation.operation_id, time(2))
            .unwrap();
        let succeeded = manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Succeeded,
                Some(result()),
                None,
                time(3),
            )
            .unwrap();

        assert_eq!(succeeded.revision, cancelling.revision + 1);
        assert!(succeeded.completed_at.is_some());
        assert!(!succeeded.cancellable);
        for state in [
            RuntimeOperationState::Cancelled,
            RuntimeOperationState::Failed,
        ] {
            assert!(manager
                .transition_at(&operation.operation_id, state, None, None, time(4))
                .is_err());
        }
        assert_eq!(
            manager
                .get_operation_at(&operation.operation_id, time(4))
                .unwrap()
                .revision,
            succeeded.revision
        );
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Stop, false, time(4))
            .unwrap();
    }

    #[test]
    fn cancelling_may_race_to_failed_and_releases_slot_once() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at(
                "docker-desktop",
                RuntimeOperationAction::Stop,
                true,
                time(0),
            )
            .unwrap();
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let cancelling = manager
            .request_cancellation_at(&operation.operation_id, time(2))
            .unwrap();
        let failed = manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(3),
            )
            .unwrap();

        assert_eq!(failed.revision, cancelling.revision + 1);
        assert!(failed.completed_at.is_some());
        assert!(!failed.cancellable);
        manager
            .create_operation_at(
                "docker-desktop",
                RuntimeOperationAction::Start,
                false,
                time(4),
            )
            .unwrap();
    }

    #[test]
    fn terminal_state_rejects_later_transitions_without_revision_change() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let terminal = manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Succeeded,
                Some(result()),
                None,
                time(2),
            )
            .unwrap();

        for state in [
            RuntimeOperationState::Running,
            RuntimeOperationState::Cancelling,
            RuntimeOperationState::Succeeded,
            RuntimeOperationState::Failed,
            RuntimeOperationState::Cancelled,
        ] {
            assert!(manager
                .transition_at(&operation.operation_id, state, None, None, time(3))
                .is_err());
        }
        assert_eq!(
            manager
                .get_operation_at(&operation.operation_id, time(3))
                .unwrap()
                .revision,
            terminal.revision
        );
    }

    #[test]
    fn exactly_one_competing_terminal_transition_succeeds() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
                time(1),
            )
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();

        for succeeds in [true, false] {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let operation_id = operation.operation_id.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                if succeeds {
                    manager.transition_at(
                        &operation_id,
                        RuntimeOperationState::Succeeded,
                        Some(result()),
                        None,
                        time(2),
                    )
                } else {
                    manager.transition_at(
                        &operation_id,
                        RuntimeOperationState::Failed,
                        None,
                        Some(failure()),
                        time(2),
                    )
                }
            }));
        }

        barrier.wait();
        assert_eq!(
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(Result::is_ok)
                .count(),
            1
        );
    }

    #[test]
    fn progress_increments_revision_but_rejected_mutations_do_not() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let progress = manager
            .update_progress_at(
                &operation.operation_id,
                RuntimeOperationProgress {
                    phase: "validating".to_string(),
                    completed_units: None,
                    total_units: None,
                    message: "Validating runtime operation.".to_string(),
                },
                time(1),
            )
            .unwrap();
        assert_eq!(progress.operation().revision, 2);
        assert!(manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Succeeded,
                None,
                None,
                time(2),
            )
            .is_err());
        assert_eq!(
            manager
                .get_operation_at(&operation.operation_id, time(2))
                .unwrap()
                .revision,
            2
        );
    }

    #[test]
    fn conflicting_lifecycle_operations_are_excluded_per_runtime() {
        let manager = RuntimeOperationManager::default();
        let first = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let conflict = manager
            .create_operation_at("ollama", RuntimeOperationAction::Stop, false, time(0))
            .unwrap_err();
        let independent = manager
            .create_operation_at(
                "docker-desktop",
                RuntimeOperationAction::Start,
                false,
                time(0),
            )
            .unwrap();

        assert_eq!(conflict.code, RuntimeErrorCode::OperationConflict);
        assert_ne!(first.runtime_id, independent.runtime_id);
    }

    #[test]
    fn open_does_not_reserve_lifecycle_slot() {
        let manager = RuntimeOperationManager::default();
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Open, false, time(0))
            .unwrap();
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Open, false, time(0))
            .unwrap();
    }

    #[test]
    fn terminal_transition_releases_lifecycle_slot() {
        let manager = RuntimeOperationManager::default();
        let first = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        manager
            .transition_at(
                &first.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(1),
            )
            .unwrap();
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Stop, false, time(1))
            .unwrap();
    }

    #[test]
    fn cancellation_returns_typed_unsupported_too_late_and_not_found_errors() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        assert_eq!(
            manager
                .request_cancellation_at(&operation.operation_id, time(1))
                .unwrap_err()
                .code,
            RuntimeErrorCode::CancellationUnsupported
        );

        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(2),
            )
            .unwrap();
        assert_eq!(
            manager
                .request_cancellation_at(&operation.operation_id, time(3))
                .unwrap_err()
                .code,
            RuntimeErrorCode::CancellationTooLate
        );
        assert_eq!(
            manager
                .request_cancellation_at("missing", time(3))
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationNotFound
        );
    }

    #[test]
    fn active_operations_are_never_evicted() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        assert!(manager
            .get_operation_at(&operation.operation_id, time(10_000))
            .is_ok());
    }

    #[test]
    fn expired_terminal_operations_are_removed_without_stale_slots() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(1),
            )
            .unwrap();
        assert_eq!(
            manager
                .get_operation_at(&operation.operation_id, time(32))
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationNotFound
        );
        manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(32))
            .unwrap();
    }

    #[test]
    fn terminal_retention_is_capped_at_two_hundred() {
        let manager = RuntimeOperationManager::default();
        let mut first_id = String::new();
        let mut last_id = String::new();

        for index in 0..=MAX_TERMINAL_OPERATIONS {
            let operation = manager
                .create_operation_at(
                    "ollama",
                    RuntimeOperationAction::Start,
                    false,
                    time_milliseconds(index as i64),
                )
                .unwrap();
            if index == 0 {
                first_id = operation.operation_id.clone();
            }
            last_id = operation.operation_id.clone();
            manager
                .transition_at(
                    &operation.operation_id,
                    RuntimeOperationState::Failed,
                    None,
                    Some(failure()),
                    time_milliseconds(index as i64),
                )
                .unwrap();
        }

        assert!(manager
            .get_operation_at(&first_id, time_milliseconds(MAX_TERMINAL_OPERATIONS as i64),)
            .is_err());
        assert!(manager
            .get_operation_at(&last_id, time_milliseconds(MAX_TERMINAL_OPERATIONS as i64),)
            .is_ok());
    }

    fn accepted(admission: RuntimeOperationAdmission) -> RuntimeOperationSnapshot {
        match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            other => panic!("expected accepted admission, got {other:?}"),
        }
    }

    #[test]
    fn admission_returns_atomic_conflict_snapshot_before_capacity() {
        let manager = RuntimeOperationManager::default();
        let first = accepted(
            manager
                .admit_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
                .unwrap(),
        );
        for runtime in ["openclaw", "docker-desktop", "open-webui", "cherry-studio"] {
            for _ in 0..3 {
                accepted(
                    manager
                        .admit_operation_at(runtime, RuntimeOperationAction::Open, false, time(0))
                        .unwrap(),
                );
            }
        }
        for _ in 0..3 {
            accepted(
                manager
                    .admit_operation_at("ollama", RuntimeOperationAction::Open, false, time(0))
                    .unwrap(),
            );
        }

        let conflict = manager
            .admit_operation_at("ollama", RuntimeOperationAction::Stop, false, time(0))
            .unwrap();
        assert_eq!(
            conflict,
            RuntimeOperationAdmission::Conflict {
                existing_operation: first.clone(),
            }
        );
        assert_eq!(first.revision, 1);
    }

    #[test]
    fn open_counts_toward_capacity_without_reserving_lifecycle_slot() {
        let manager = RuntimeOperationManager::default();
        for index in 0..MAX_ACTIVE_OPERATIONS {
            let runtime = if index % 2 == 0 {
                "ollama"
            } else {
                "open-webui"
            };
            assert!(matches!(
                manager
                    .admit_operation_at(runtime, RuntimeOperationAction::Open, false, time(0))
                    .unwrap(),
                RuntimeOperationAdmission::Accepted { .. }
            ));
        }
        assert!(manager.store.lock().unwrap().lifecycle_slots.is_empty());
        let rejected = manager
            .admit_operation_at(
                "docker-desktop",
                RuntimeOperationAction::Open,
                false,
                time(0),
            )
            .unwrap();
        match rejected {
            RuntimeOperationAdmission::Rejected { error } => {
                assert_eq!(error.code, RuntimeErrorCode::OperationCapacityExceeded);
                assert!(error.retryable);
            }
            other => panic!("expected capacity rejection, got {other:?}"),
        }
    }

    #[test]
    fn terminal_transition_releases_capacity_immediately() {
        let manager = RuntimeOperationManager::default();
        let mut operations = Vec::new();
        for _ in 0..MAX_ACTIVE_OPERATIONS {
            operations.push(accepted(
                manager
                    .admit_operation_at("ollama", RuntimeOperationAction::Open, false, time(0))
                    .unwrap(),
            ));
        }
        manager
            .transition_at(
                &operations[0].operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(1),
            )
            .unwrap();
        assert!(matches!(
            manager
                .admit_operation_at(
                    "docker-desktop",
                    RuntimeOperationAction::Open,
                    false,
                    time(1),
                )
                .unwrap(),
            RuntimeOperationAdmission::Accepted { .. }
        ));
    }

    #[test]
    fn stale_lifecycle_slots_are_repaired_under_admission_lock() {
        for stale_kind in ["missing", "terminal", "wrong-runtime"] {
            let manager = RuntimeOperationManager::default();
            {
                let mut store = manager.store.lock().unwrap();
                match stale_kind {
                    "missing" => {
                        store
                            .lifecycle_slots
                            .insert("ollama".to_string(), "missing".to_string());
                    }
                    "terminal" => {
                        let operation = RuntimeOperationSnapshot {
                            operation_id: "terminal".to_string(),
                            runtime_id: "ollama".to_string(),
                            action: RuntimeOperationAction::Start,
                            state: RuntimeOperationState::Failed,
                            revision: 2,
                            accepted_at: time(0).to_rfc3339(),
                            started_at: None,
                            updated_at: time(0).to_rfc3339(),
                            completed_at: Some(time(0).to_rfc3339()),
                            progress: None,
                            cancellable: false,
                            result: None,
                            error: Some(failure()),
                        };
                        store.operations.insert("terminal".to_string(), operation);
                        store
                            .lifecycle_slots
                            .insert("ollama".to_string(), "terminal".to_string());
                    }
                    "wrong-runtime" => {
                        let operation = RuntimeOperationSnapshot {
                            operation_id: "wrong".to_string(),
                            runtime_id: "openclaw".to_string(),
                            action: RuntimeOperationAction::Start,
                            state: RuntimeOperationState::Queued,
                            revision: 1,
                            accepted_at: time(0).to_rfc3339(),
                            started_at: None,
                            updated_at: time(0).to_rfc3339(),
                            completed_at: None,
                            progress: None,
                            cancellable: false,
                            result: None,
                            error: None,
                        };
                        store.operations.insert("wrong".to_string(), operation);
                        store
                            .lifecycle_slots
                            .insert("ollama".to_string(), "wrong".to_string());
                    }
                    _ => unreachable!(),
                }
            }
            let operation = accepted(
                manager
                    .admit_operation_at("ollama", RuntimeOperationAction::Stop, false, time(1))
                    .unwrap(),
            );
            assert_eq!(
                manager.store.lock().unwrap().lifecycle_slots.get("ollama"),
                Some(&operation.operation_id)
            );
        }
    }

    #[test]
    fn duplicate_progress_is_explicitly_unchanged() {
        let manager = RuntimeOperationManager::default();
        let operation = manager
            .create_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let value = RuntimeOperationProgress {
            phase: "validating".to_string(),
            completed_units: None,
            total_units: None,
            message: "Validating runtime operation.".to_string(),
        };
        let applied = manager
            .update_progress_at(&operation.operation_id, value.clone(), time(1))
            .unwrap();
        let unchanged = manager
            .update_progress_at(&operation.operation_id, value, time(2))
            .unwrap();
        assert!(matches!(
            applied,
            RuntimeOperationProgressUpdate::Applied(_)
        ));
        assert!(matches!(
            unchanged,
            RuntimeOperationProgressUpdate::Unchanged(_)
        ));
        assert_eq!(applied.operation().revision, operation.revision + 1);
        assert_eq!(unchanged.operation().revision, applied.operation().revision);
        assert_eq!(
            unchanged.operation().updated_at,
            applied.operation().updated_at
        );
        manager
            .transition_at(
                &operation.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(failure()),
                time(3),
            )
            .unwrap();
        assert!(manager
            .update_progress_at(
                &operation.operation_id,
                RuntimeOperationProgress {
                    phase: "complete".to_string(),
                    completed_units: None,
                    total_units: None,
                    message: "Complete.".to_string(),
                },
                time(4),
            )
            .is_err());
    }

    #[test]
    fn concurrent_lifecycle_admission_has_one_owner_and_atomic_conflicts() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let barrier = Arc::new(Barrier::new(9));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                manager
                    .admit_operation("ollama", RuntimeOperationAction::Start, false)
                    .unwrap()
            }));
        }
        barrier.wait();
        let admissions = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        let accepted = admissions
            .iter()
            .find_map(|admission| match admission {
                RuntimeOperationAdmission::Accepted { operation } => Some(operation),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            admissions
                .iter()
                .filter(|admission| matches!(admission, RuntimeOperationAdmission::Accepted { .. }))
                .count(),
            1
        );
        assert!(admissions.iter().all(|admission| match admission {
            RuntimeOperationAdmission::Accepted { operation }
            | RuntimeOperationAdmission::Conflict {
                existing_operation: operation,
            } => operation.operation_id == accepted.operation_id,
            RuntimeOperationAdmission::Rejected { .. } => false,
        }));
        assert_eq!(
            manager.store.lock().unwrap().lifecycle_slots.get("ollama"),
            Some(&accepted.operation_id)
        );
    }

    #[test]
    fn concurrent_open_admission_never_exceeds_global_capacity() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let barrier = Arc::new(Barrier::new(33));
        let mut handles = Vec::new();
        for index in 0..32 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                let runtime = if index % 2 == 0 {
                    "ollama"
                } else {
                    "open-webui"
                };
                manager
                    .admit_operation(runtime, RuntimeOperationAction::Open, false)
                    .unwrap()
            }));
        }
        barrier.wait();
        let accepted_count = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|admission| matches!(admission, RuntimeOperationAdmission::Accepted { .. }))
            .count();
        assert_eq!(accepted_count, MAX_ACTIVE_OPERATIONS);
        assert_eq!(
            active_operation_count(&manager.store.lock().unwrap()),
            MAX_ACTIVE_OPERATIONS
        );
    }

    #[test]
    fn admission_serialization_is_canonical_and_contains_no_execution_details() {
        let manager = RuntimeOperationManager::default();
        let accepted = manager
            .admit_operation_at("ollama", RuntimeOperationAction::Start, false, time(0))
            .unwrap();
        let conflict = manager
            .admit_operation_at("ollama", RuntimeOperationAction::Stop, false, time(0))
            .unwrap();
        let accepted_json = serde_json::to_value(&accepted).unwrap();
        let conflict_json = serde_json::to_value(&conflict).unwrap();
        assert_eq!(accepted_json["status"], "accepted");
        assert_eq!(conflict_json["status"], "conflict");
        assert!(conflict_json.get("existingOperation").is_some());

        let capacity = RuntimeOperationAdmission::Rejected {
            error: safe_error(
                RuntimeErrorCode::OperationCapacityExceeded,
                "Runtime operation capacity has been reached.",
                true,
            ),
        };
        let capacity_json = serde_json::to_value(capacity).unwrap();
        assert_eq!(capacity_json["status"], "rejected");
        assert_eq!(
            capacity_json["error"]["code"],
            "operation-capacity-exceeded"
        );
        assert_eq!(capacity_json["error"]["retryable"], true);

        let serialized = format!("{accepted_json}{conflict_json}{capacity_json}");
        for forbidden in [
            "plan", "url", "token", "path", "command", "stdout", "stderr",
        ] {
            assert!(!serialized.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn json_contract_uses_typescript_compatible_names_and_safe_errors() {
        let manager = RuntimeOperationManager::default();
        let snapshot = manager
            .create_operation_at("ollama", RuntimeOperationAction::Restart, false, time(0))
            .unwrap();
        let event = super::super::models::RuntimeOperationEvent {
            version: 1,
            operation: snapshot,
        };
        let value = serde_json::to_value(event).unwrap();

        assert_eq!(value["version"], 1);
        assert_eq!(value["operation"]["action"], "restart");
        assert_eq!(value["operation"]["state"], "queued");
        assert_eq!(value["operation"]["revision"], 1);
        assert!(value["operation"].get("operationId").is_some());
        assert_eq!(
            serde_json::to_value(safe_error(
                RuntimeErrorCode::OperationTaskFailed,
                "Runtime operation task failed.",
                true,
            ))
            .unwrap(),
            json!({
                "code": "operation-task-failed",
                "message": "Runtime operation task failed.",
                "retryable": true,
            })
        );
    }

    #[test]
    fn cancellation_outcome_distinguishes_applied_from_idempotent_unchanged() {
        let manager = RuntimeOperationManager::default();
        let queued = manager
            .create_operation_at("ollama", RuntimeOperationAction::Open, true, time(0))
            .unwrap();

        let applied = manager
            .request_cancellation_update_at(&queued.operation_id, time(1))
            .unwrap();
        let cancelling = match applied {
            RuntimeOperationCancellationUpdate::Applied(snapshot) => snapshot,
            RuntimeOperationCancellationUpdate::Unchanged(_) => panic!("expected mutation"),
        };
        assert_eq!(cancelling.state, RuntimeOperationState::Cancelling);
        assert_eq!(cancelling.revision, queued.revision + 1);
        assert_ne!(cancelling.updated_at, queued.updated_at);

        let unchanged = manager
            .request_cancellation_update_at(&queued.operation_id, time(2))
            .unwrap();
        let repeated = match unchanged {
            RuntimeOperationCancellationUpdate::Unchanged(snapshot) => snapshot,
            RuntimeOperationCancellationUpdate::Applied(_) => panic!("expected no mutation"),
        };
        assert_eq!(repeated.revision, cancelling.revision);
        assert_eq!(repeated.updated_at, cancelling.updated_at);
    }
}
