use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
    time::Instant,
};

use tauri::{AppHandle, Emitter};

use super::{
    lifecycle::{
        execute_plan, prepare_execution_plan, validate_runtime_lifecycle_request,
        RuntimeExecutionPlan, RuntimeLifecycleRequest, ValidatedRuntimeLifecycleRequest,
        PREPARATION_TIMEOUT,
    },
    models::{
        NormalizedRuntimeError, RuntimeErrorCode, RuntimeOperationAdmission, RuntimeOperationEvent,
        RuntimeOperationProgress, RuntimeOperationResult, RuntimeOperationSnapshot,
        RuntimeOperationState,
    },
    operations::{
        RuntimeOperationCancellationUpdate, RuntimeOperationManager, RuntimeOperationProgressUpdate,
    },
    scheduler::RuntimeScheduler,
};

pub(crate) const RUNTIME_OPERATION_EVENT: &str = "runtime://operation";

#[derive(Clone)]
pub struct RuntimeExecutionState {
    manager: Arc<RuntimeOperationManager>,
    scheduler: RuntimeScheduler,
}

impl Default for RuntimeExecutionState {
    fn default() -> Self {
        Self {
            manager: Arc::new(RuntimeOperationManager::default()),
            scheduler: RuntimeScheduler::default(),
        }
    }
}

impl RuntimeExecutionState {
    pub(crate) fn manager(&self) -> Arc<RuntimeOperationManager> {
        Arc::clone(&self.manager)
    }

    pub(crate) fn scheduler(&self) -> RuntimeScheduler {
        self.scheduler.clone()
    }
}

pub(crate) trait OperationEventEmitter: Send + Sync {
    fn emit(&self, snapshot: RuntimeOperationSnapshot) -> Result<(), ()>;
}

#[cfg(test)]
pub(crate) trait SupervisorSpawner {
    fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()>;
}

trait PreparedOperation: Send {
    fn execute(
        self: Box<Self>,
        report: &mut dyn FnMut(RuntimeOperationProgress),
    ) -> Result<(), NormalizedRuntimeError>;
}

trait OperationPipeline: Send + Sync {
    fn prepare(
        &self,
        request: &ValidatedRuntimeLifecycleRequest,
        deadline: Instant,
    ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError>;
}

struct NativePipeline;

struct NativePreparedOperation(RuntimeExecutionPlan);

impl OperationPipeline for NativePipeline {
    fn prepare(
        &self,
        request: &ValidatedRuntimeLifecycleRequest,
        deadline: Instant,
    ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError> {
        prepare_execution_plan(request, deadline)
            .map(|plan| Box::new(NativePreparedOperation(plan)) as Box<dyn PreparedOperation>)
    }
}

impl PreparedOperation for NativePreparedOperation {
    fn execute(
        self: Box<Self>,
        report: &mut dyn FnMut(RuntimeOperationProgress),
    ) -> Result<(), NormalizedRuntimeError> {
        execute_plan(&self.0, report)
    }
}

pub(crate) struct TauriEventEmitter {
    app: AppHandle,
}

impl TauriEventEmitter {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl OperationEventEmitter for TauriEventEmitter {
    fn emit(&self, snapshot: RuntimeOperationSnapshot) -> Result<(), ()> {
        self.app
            .emit(
                RUNTIME_OPERATION_EVENT,
                RuntimeOperationEvent {
                    version: 1,
                    operation: snapshot,
                },
            )
            .map_err(|_| ())
    }
}

pub(crate) fn start_accepted_operation(
    manager: Arc<RuntimeOperationManager>,
    scheduler: RuntimeScheduler,
    request: RuntimeLifecycleRequest,
    app: AppHandle,
) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
    let admission = manager.admit_operation(&request.runtime_id, request.action, false)?;
    let queued = match admission {
        RuntimeOperationAdmission::Accepted { operation } => operation,
        other => return Ok(other),
    };
    let emitter: Arc<dyn OperationEventEmitter> = Arc::new(TauriEventEmitter::new(app));
    emit_best_effort(emitter.as_ref(), queued.clone());
    let operation_id = queued.operation_id.clone();
    let task_manager = Arc::clone(&manager);
    let task_emitter = Arc::clone(&emitter);
    let task = Box::new(move || match validate_runtime_lifecycle_request(request) {
        Ok(validated) => run_operation_supervisor(
            task_manager,
            operation_id,
            validated,
            task_emitter,
            Arc::new(NativePipeline),
        ),
        Err(error) => {
            let _ = fail_operation(&task_manager, &operation_id, error, task_emitter.as_ref());
        }
    });
    let scheduled = catch_unwind(AssertUnwindSafe(|| scheduler.enqueue(task)))
        .ok()
        .and_then(Result::ok)
        .is_some();
    if !scheduled {
        let terminal = fail_operation(
            &manager,
            &queued.operation_id,
            operation_task_failed(),
            emitter.as_ref(),
        )?;
        let operation = match terminal {
            RuntimeOperationTerminalUpdate::Applied(operation)
            | RuntimeOperationTerminalUpdate::AlreadyTerminal(operation) => operation,
        };
        return Ok(RuntimeOperationAdmission::Accepted { operation });
    }
    Ok(RuntimeOperationAdmission::Accepted { operation: queued })
}

#[cfg(test)]
fn start_with_dependencies(
    manager: Arc<RuntimeOperationManager>,
    request: ValidatedRuntimeLifecycleRequest,
    emitter: Arc<dyn OperationEventEmitter>,
    spawner: &dyn SupervisorSpawner,
    pipeline: Arc<dyn OperationPipeline>,
) -> Result<RuntimeOperationAdmission, NormalizedRuntimeError> {
    let admission = manager.admit_operation(request.runtime_id(), request.action(), false)?;
    let queued = match admission {
        RuntimeOperationAdmission::Accepted { operation } => operation,
        other => return Ok(other),
    };

    emit_best_effort(emitter.as_ref(), queued.clone());
    let operation_id = queued.operation_id.clone();
    let task_manager = Arc::clone(&manager);
    let task_emitter = Arc::clone(&emitter);
    let task = Box::new(move || {
        run_operation_supervisor(task_manager, operation_id, request, task_emitter, pipeline);
    });

    let scheduled = catch_unwind(AssertUnwindSafe(|| spawner.spawn(task)))
        .ok()
        .and_then(Result::ok)
        .is_some();
    if !scheduled {
        let terminal = fail_operation(
            &manager,
            &queued.operation_id,
            operation_task_failed(),
            emitter.as_ref(),
        )?;
        let operation = match terminal {
            RuntimeOperationTerminalUpdate::Applied(operation)
            | RuntimeOperationTerminalUpdate::AlreadyTerminal(operation) => operation,
        };
        return Ok(RuntimeOperationAdmission::Accepted { operation });
    }

    Ok(RuntimeOperationAdmission::Accepted { operation: queued })
}

fn run_operation_supervisor(
    manager: Arc<RuntimeOperationManager>,
    operation_id: String,
    request: ValidatedRuntimeLifecycleRequest,
    emitter: Arc<dyn OperationEventEmitter>,
    pipeline: Arc<dyn OperationPipeline>,
) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        run_operation_supervisor_inner(
            &manager,
            &operation_id,
            &request,
            emitter.as_ref(),
            pipeline.as_ref(),
        )
    }));
    if outcome.is_err() {
        let _ = fail_operation(
            &manager,
            &operation_id,
            operation_task_failed(),
            emitter.as_ref(),
        );
    }
}

fn run_operation_supervisor_inner(
    manager: &RuntimeOperationManager,
    operation_id: &str,
    request: &ValidatedRuntimeLifecycleRequest,
    emitter: &dyn OperationEventEmitter,
    pipeline: &dyn OperationPipeline,
) {
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    let prepared = match pipeline.prepare(request, deadline) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = fail_operation(manager, operation_id, error, emitter);
            return;
        }
    };

    let running = match manager.transition(operation_id, RuntimeOperationState::Running, None, None)
    {
        Ok(snapshot) => snapshot,
        Err(_) => return,
    };
    emit_best_effort(emitter, running);

    let mut report = |progress| match manager.update_progress(operation_id, progress) {
        Ok(update @ RuntimeOperationProgressUpdate::Applied(_)) => {
            emit_best_effort(emitter, update.operation().clone());
        }
        Ok(RuntimeOperationProgressUpdate::Unchanged(_)) | Err(_) => {}
    };
    let execution = prepared.execute(&mut report);
    match execution {
        Ok(()) => {
            if let Ok(snapshot) = manager.transition(
                operation_id,
                RuntimeOperationState::Succeeded,
                Some(RuntimeOperationResult {
                    message: "Runtime operation completed.".to_string(),
                    bulk: None,
                }),
                None,
            ) {
                emit_best_effort(emitter, snapshot);
            }
        }
        Err(error) => {
            let _ = fail_operation(manager, operation_id, error, emitter);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeOperationTerminalUpdate {
    Applied(RuntimeOperationSnapshot),
    AlreadyTerminal(RuntimeOperationSnapshot),
}

fn fail_operation(
    manager: &RuntimeOperationManager,
    operation_id: &str,
    error: NormalizedRuntimeError,
    emitter: &dyn OperationEventEmitter,
) -> Result<RuntimeOperationTerminalUpdate, NormalizedRuntimeError> {
    match manager.transition(
        operation_id,
        RuntimeOperationState::Failed,
        None,
        Some(error),
    ) {
        Ok(snapshot) => {
            emit_best_effort(emitter, snapshot.clone());
            Ok(RuntimeOperationTerminalUpdate::Applied(snapshot))
        }
        Err(transition_error) => match manager.get_operation(operation_id) {
            Ok(snapshot) if snapshot.state.is_terminal() => {
                Ok(RuntimeOperationTerminalUpdate::AlreadyTerminal(snapshot))
            }
            Ok(_) => Err(transition_error),
            Err(manager_error) => Err(manager_error),
        },
    }
}

pub(crate) fn cancel_operation_with_emitter(
    manager: &RuntimeOperationManager,
    operation_id: &str,
    emitter: &dyn OperationEventEmitter,
) -> Result<RuntimeOperationSnapshot, NormalizedRuntimeError> {
    match manager.request_cancellation_update(operation_id)? {
        RuntimeOperationCancellationUpdate::Applied(snapshot) => {
            emit_best_effort(emitter, snapshot.clone());
            Ok(snapshot)
        }
        RuntimeOperationCancellationUpdate::Unchanged(snapshot) => Ok(snapshot),
    }
}

pub(crate) fn execute_validated_request(
    request: &ValidatedRuntimeLifecycleRequest,
    report: &mut dyn FnMut(RuntimeOperationProgress),
) -> Result<(), NormalizedRuntimeError> {
    NativePipeline
        .prepare(request, Instant::now() + PREPARATION_TIMEOUT)?
        .execute(report)
}

pub(crate) fn emit_best_effort(
    emitter: &dyn OperationEventEmitter,
    snapshot: RuntimeOperationSnapshot,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| emitter.emit(snapshot)));
}

fn operation_task_failed() -> NormalizedRuntimeError {
    NormalizedRuntimeError {
        code: RuntimeErrorCode::OperationTaskFailed,
        message: "The runtime operation task failed.".to_string(),
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    };

    use super::*;
    use crate::runtime::{
        lifecycle::{validate_runtime_lifecycle_request, RuntimeLifecycleRequest},
        models::RuntimeOperationAction,
    };

    #[derive(Default)]
    struct RecordingEmitter {
        events: Mutex<Vec<RuntimeOperationSnapshot>>,
        fail: AtomicBool,
    }

    impl OperationEventEmitter for RecordingEmitter {
        fn emit(&self, snapshot: RuntimeOperationSnapshot) -> Result<(), ()> {
            self.events.lock().unwrap().push(snapshot);
            if self.fail.load(Ordering::SeqCst) {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    struct InlineSpawner;

    impl SupervisorSpawner for InlineSpawner {
        fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()> {
            task();
            Ok(())
        }
    }

    struct RejectingSpawner;

    impl SupervisorSpawner for RejectingSpawner {
        fn spawn(&self, _task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()> {
            Err(())
        }
    }

    struct PanickingSpawner;

    impl SupervisorSpawner for PanickingSpawner {
        fn spawn(&self, _task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()> {
            panic!("scheduler internals")
        }
    }

    struct RunThenRejectSpawner;

    impl SupervisorSpawner for RunThenRejectSpawner {
        fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()> {
            task();
            Err(())
        }
    }

    struct ThreadSpawner;

    impl SupervisorSpawner for ThreadSpawner {
        fn spawn(&self, task: Box<dyn FnOnce() + Send + 'static>) -> Result<(), ()> {
            std::thread::Builder::new()
                .spawn(task)
                .map(|_| ())
                .map_err(|_| ())
        }
    }

    struct FakePipeline {
        preparation: Mutex<Option<Result<FakePrepared, NormalizedRuntimeError>>>,
    }

    struct FakePrepared {
        progress: Vec<RuntimeOperationProgress>,
        result: Result<(), NormalizedRuntimeError>,
        executed: Option<Arc<AtomicBool>>,
        block: Option<mpsc::Receiver<()>>,
    }

    impl OperationPipeline for FakePipeline {
        fn prepare(
            &self,
            _request: &ValidatedRuntimeLifecycleRequest,
            _deadline: Instant,
        ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError> {
            self.preparation
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .map(|prepared| Box::new(prepared) as Box<dyn PreparedOperation>)
        }
    }

    impl PreparedOperation for FakePrepared {
        fn execute(
            self: Box<Self>,
            report: &mut dyn FnMut(RuntimeOperationProgress),
        ) -> Result<(), NormalizedRuntimeError> {
            if let Some(executed) = &self.executed {
                executed.store(true, Ordering::SeqCst);
            }
            if let Some(receiver) = self.block {
                let _ = receiver.recv();
            }
            for progress in self.progress {
                report(progress);
            }
            self.result
        }
    }

    fn request_for(action: RuntimeOperationAction) -> ValidatedRuntimeLifecycleRequest {
        validate_runtime_lifecycle_request(RuntimeLifecycleRequest {
            runtime_id: "ollama".to_string(),
            action,
            endpoint_url: Some("http://localhost:11434".to_string()),
        })
        .unwrap()
    }

    fn request() -> ValidatedRuntimeLifecycleRequest {
        request_for(RuntimeOperationAction::Open)
    }

    fn pipeline(result: Result<(), NormalizedRuntimeError>) -> Arc<dyn OperationPipeline> {
        Arc::new(FakePipeline {
            preparation: Mutex::new(Some(Ok(FakePrepared {
                progress: vec![RuntimeOperationProgress {
                    phase: "opening".to_string(),
                    completed_units: None,
                    total_units: None,
                    message: "Opening runtime.".to_string(),
                }],
                result,
                executed: None,
                block: None,
            }))),
        })
    }

    #[test]
    fn clones_share_runtime_coordination_state() {
        let state = RuntimeExecutionState::default();
        let clone = state.clone();
        assert!(Arc::ptr_eq(&state.manager(), &clone.manager()));
        assert!(state.scheduler().shares_state_with(&clone.scheduler()));
    }

    #[test]
    fn sequential_supervisor_emits_queued_running_progress_and_success() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter.clone(),
            &InlineSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        let operation_id = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
            _ => panic!("expected acceptance"),
        };
        let latest = manager.get_operation(&operation_id).unwrap();
        assert_eq!(latest.state, RuntimeOperationState::Succeeded);
        let events = emitter.events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].state, RuntimeOperationState::Queued);
        assert_eq!(events[1].state, RuntimeOperationState::Running);
        assert_eq!(events[2].revision, 3);
        assert_eq!(events[3].state, RuntimeOperationState::Succeeded);
        assert!(events
            .windows(2)
            .all(|pair| pair[0].revision < pair[1].revision));
    }

    #[test]
    fn duplicate_progress_emits_only_the_applied_full_snapshot() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let update = RuntimeOperationProgress {
            phase: "opening".to_string(),
            completed_units: None,
            total_units: None,
            message: "Opening runtime.".to_string(),
        };
        let fake = Arc::new(FakePipeline {
            preparation: Mutex::new(Some(Ok(FakePrepared {
                progress: vec![update.clone(), update],
                result: Ok(()),
                executed: None,
                block: None,
            }))),
        });
        start_with_dependencies(manager, request(), emitter.clone(), &InlineSpawner, fake).unwrap();
        let events = emitter.events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[2].progress.as_ref().unwrap().phase, "opening");
        assert_eq!(events[3].revision, 4);
    }

    #[test]
    fn event_contract_is_version_one_full_snapshot_without_native_details() {
        let manager = RuntimeOperationManager::default();
        let operation = match manager
            .admit_operation("ollama", RuntimeOperationAction::Open, false)
            .unwrap()
        {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        let value = serde_json::to_value(RuntimeOperationEvent {
            version: 1,
            operation,
        })
        .unwrap();
        assert_eq!(RUNTIME_OPERATION_EVENT, "runtime://operation");
        assert_eq!(value["version"], 1);
        assert_eq!(value["operation"]["revision"], 1);
        let serialized = value.to_string().to_ascii_lowercase();
        for forbidden in ["plan", "command", "stdout", "stderr", "endpoint", "token"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn scheduling_failure_returns_accepted_failed_and_releases_slot() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request_for(RuntimeOperationAction::Start),
            emitter,
            &RejectingSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        let operation = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(operation.state, RuntimeOperationState::Failed);
        assert_eq!(
            operation.error.unwrap().code,
            RuntimeErrorCode::OperationTaskFailed
        );
        assert!(matches!(
            manager
                .admit_operation("ollama", RuntimeOperationAction::Stop, false)
                .unwrap(),
            RuntimeOperationAdmission::Accepted { .. }
        ));
    }

    #[test]
    fn conflict_and_capacity_rejections_emit_nothing() {
        let manager = Arc::new(RuntimeOperationManager::default());
        manager
            .admit_operation("ollama", RuntimeOperationAction::Start, false)
            .unwrap();
        let emitter = Arc::new(RecordingEmitter::default());
        let conflict = start_with_dependencies(
            Arc::clone(&manager),
            request_for(RuntimeOperationAction::Stop),
            emitter.clone(),
            &RejectingSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        assert!(matches!(
            conflict,
            RuntimeOperationAdmission::Conflict { .. }
        ));
        assert!(emitter.events.lock().unwrap().is_empty());

        let capacity_manager = Arc::new(RuntimeOperationManager::default());
        for _ in 0..super::super::operations::MAX_ACTIVE_OPERATIONS {
            capacity_manager
                .admit_operation("ollama", RuntimeOperationAction::Open, false)
                .unwrap();
        }
        let capacity_emitter = Arc::new(RecordingEmitter::default());
        let rejected = start_with_dependencies(
            capacity_manager,
            request(),
            capacity_emitter.clone(),
            &RejectingSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        assert!(matches!(
            rejected,
            RuntimeOperationAdmission::Rejected { .. }
        ));
        assert!(capacity_emitter.events.lock().unwrap().is_empty());
    }

    #[test]
    fn preparation_failure_never_executes_and_terminalizes_once() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let pipeline = Arc::new(FakePipeline {
            preparation: Mutex::new(Some(Err(NormalizedRuntimeError {
                code: RuntimeErrorCode::ReadinessTimeout,
                message: "Runtime preparation timed out.".to_string(),
                retryable: true,
            }))),
        });
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter.clone(),
            &InlineSpawner,
            pipeline,
        )
        .unwrap();
        let id = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(
            manager.get_operation(&id).unwrap().state,
            RuntimeOperationState::Failed
        );
        assert_eq!(emitter.events.lock().unwrap().len(), 2);
    }

    #[test]
    fn execution_failure_is_sanitized_and_event_failure_does_not_change_truth() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        emitter.fail.store(true, Ordering::SeqCst);
        let error = NormalizedRuntimeError {
            code: RuntimeErrorCode::OperationFailed,
            message: "Runtime operation failed.".to_string(),
            retryable: true,
        };
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter,
            &InlineSpawner,
            pipeline(Err(error)),
        )
        .unwrap();
        let id = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(
            manager.get_operation(&id).unwrap().state,
            RuntimeOperationState::Failed
        );
    }

    #[test]
    fn start_returns_queued_while_independent_execution_is_blocked() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let executed = Arc::new(AtomicBool::new(false));
        let (release_tx, release_rx) = mpsc::channel();
        let fake = Arc::new(FakePipeline {
            preparation: Mutex::new(Some(Ok(FakePrepared {
                progress: vec![],
                result: Ok(()),
                executed: Some(Arc::clone(&executed)),
                block: Some(release_rx),
            }))),
        });
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter,
            &ThreadSpawner,
            fake,
        )
        .unwrap();
        let operation = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(operation.state, RuntimeOperationState::Queued);
        for _ in 0..100 {
            if executed.load(Ordering::SeqCst) {
                break;
            }
            std::thread::yield_now();
        }
        assert!(executed.load(Ordering::SeqCst));
        assert!(manager
            .admit_operation("open-webui", RuntimeOperationAction::Open, false)
            .is_ok());
        release_tx.send(()).unwrap();
    }

    #[test]
    fn operation_stays_queued_and_manager_is_responsive_during_blocked_preparation() {
        struct BlockingPipeline(Mutex<Option<mpsc::Receiver<()>>>);
        impl OperationPipeline for BlockingPipeline {
            fn prepare(
                &self,
                _request: &ValidatedRuntimeLifecycleRequest,
                _deadline: Instant,
            ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError> {
                let receiver = self.0.lock().unwrap().take().unwrap();
                let _ = receiver.recv();
                Ok(Box::new(FakePrepared {
                    progress: vec![],
                    result: Ok(()),
                    executed: None,
                    block: None,
                }))
            }
        }

        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let (release_tx, release_rx) = mpsc::channel();
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter,
            &ThreadSpawner,
            Arc::new(BlockingPipeline(Mutex::new(Some(release_rx)))),
        )
        .unwrap();
        let operation = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(
            manager
                .get_operation(&operation.operation_id)
                .unwrap()
                .state,
            RuntimeOperationState::Queued
        );
        assert!(manager
            .admit_operation("open-webui", RuntimeOperationAction::Open, false)
            .is_ok());
        release_tx.send(()).unwrap();
    }

    #[test]
    fn supervisor_panic_becomes_sanitized_failure_without_panic_text() {
        struct PanicPipeline;
        impl OperationPipeline for PanicPipeline {
            fn prepare(
                &self,
                _request: &ValidatedRuntimeLifecycleRequest,
                _deadline: Instant,
            ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError> {
                panic!("secret panic payload")
            }
        }
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter,
            &InlineSpawner,
            Arc::new(PanicPipeline),
        )
        .unwrap();
        let id = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
            _ => panic!("expected acceptance"),
        };
        let failed = manager.get_operation(&id).unwrap();
        assert_eq!(failed.state, RuntimeOperationState::Failed);
        assert_eq!(failed.error.unwrap(), operation_task_failed());
    }

    #[test]
    fn execution_panic_becomes_sanitized_failure() {
        struct PanicPrepared;
        impl PreparedOperation for PanicPrepared {
            fn execute(
                self: Box<Self>,
                _report: &mut dyn FnMut(RuntimeOperationProgress),
            ) -> Result<(), NormalizedRuntimeError> {
                panic!("native panic details")
            }
        }
        struct Pipeline;
        impl OperationPipeline for Pipeline {
            fn prepare(
                &self,
                _request: &ValidatedRuntimeLifecycleRequest,
                _deadline: Instant,
            ) -> Result<Box<dyn PreparedOperation>, NormalizedRuntimeError> {
                Ok(Box::new(PanicPrepared))
            }
        }

        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request(),
            emitter,
            &InlineSpawner,
            Arc::new(Pipeline),
        )
        .unwrap();
        let id = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
            _ => panic!("expected acceptance"),
        };
        let failed = manager.get_operation(&id).unwrap();
        assert_eq!(failed.error.unwrap(), operation_task_failed());
    }

    #[test]
    fn rejected_running_transition_prevents_execution() {
        let manager = RuntimeOperationManager::default();
        let queued = match manager
            .admit_operation("ollama", RuntimeOperationAction::Open, false)
            .unwrap()
        {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        manager
            .transition(
                &queued.operation_id,
                RuntimeOperationState::Failed,
                None,
                Some(operation_task_failed()),
            )
            .unwrap();
        let executed = Arc::new(AtomicBool::new(false));
        let fake = FakePipeline {
            preparation: Mutex::new(Some(Ok(FakePrepared {
                progress: vec![],
                result: Ok(()),
                executed: Some(Arc::clone(&executed)),
                block: None,
            }))),
        };
        let emitter = RecordingEmitter::default();
        run_operation_supervisor_inner(&manager, &queued.operation_id, &request(), &emitter, &fake);
        assert!(!executed.load(Ordering::SeqCst));
        assert!(emitter.events.lock().unwrap().is_empty());
    }

    #[test]
    fn cancellation_applied_emits_once_and_unchanged_emits_nothing() {
        let manager = RuntimeOperationManager::default();
        let queued = match manager
            .admit_operation("ollama", RuntimeOperationAction::Open, true)
            .unwrap()
        {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        let emitter = RecordingEmitter::default();
        let cancelling =
            cancel_operation_with_emitter(&manager, &queued.operation_id, &emitter).unwrap();
        assert_eq!(cancelling.revision, queued.revision + 1);
        assert_eq!(emitter.events.lock().unwrap().len(), 1);
        let event = RuntimeOperationEvent {
            version: 1,
            operation: emitter.events.lock().unwrap()[0].clone(),
        };
        assert_eq!(serde_json::to_value(event).unwrap()["version"], 1);

        let repeated =
            cancel_operation_with_emitter(&manager, &queued.operation_id, &emitter).unwrap();
        assert_eq!(repeated.revision, cancelling.revision);
        assert_eq!(emitter.events.lock().unwrap().len(), 1);
    }

    #[test]
    fn rejected_cancellation_paths_emit_nothing_and_preserve_state() {
        let manager = RuntimeOperationManager::default();
        let queued = match manager
            .admit_operation("ollama", RuntimeOperationAction::Open, false)
            .unwrap()
        {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        let emitter = RecordingEmitter::default();
        assert_eq!(
            cancel_operation_with_emitter(&manager, &queued.operation_id, &emitter)
                .unwrap_err()
                .code,
            RuntimeErrorCode::CancellationUnsupported
        );
        assert_eq!(manager.get_operation(&queued.operation_id).unwrap(), queued);

        manager
            .transition(
                &queued.operation_id,
                RuntimeOperationState::Running,
                None,
                None,
            )
            .unwrap();
        manager
            .transition(
                &queued.operation_id,
                RuntimeOperationState::Succeeded,
                Some(RuntimeOperationResult {
                    message: "done".to_string(),
                    bulk: None,
                }),
                None,
            )
            .unwrap();
        assert_eq!(
            cancel_operation_with_emitter(&manager, &queued.operation_id, &emitter)
                .unwrap_err()
                .code,
            RuntimeErrorCode::CancellationTooLate
        );
        assert_eq!(
            cancel_operation_with_emitter(&manager, "missing", &emitter)
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationNotFound
        );
        assert!(emitter.events.lock().unwrap().is_empty());
    }

    #[test]
    fn spawner_panic_is_a_sanitized_scheduling_failure() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            Arc::clone(&manager),
            request_for(RuntimeOperationAction::Start),
            emitter,
            &PanickingSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        let operation = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(operation.state, RuntimeOperationState::Failed);
        assert_eq!(operation.error.unwrap(), operation_task_failed());
        assert!(matches!(
            manager
                .admit_operation("ollama", RuntimeOperationAction::Stop, false)
                .unwrap(),
            RuntimeOperationAdmission::Accepted { .. }
        ));
    }

    #[test]
    fn terminal_winner_is_returned_when_spawner_runs_then_reports_failure() {
        let manager = Arc::new(RuntimeOperationManager::default());
        let emitter = Arc::new(RecordingEmitter::default());
        let admission = start_with_dependencies(
            manager,
            request(),
            emitter.clone(),
            &RunThenRejectSpawner,
            pipeline(Ok(())),
        )
        .unwrap();
        let operation = match admission {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert_eq!(operation.state, RuntimeOperationState::Succeeded);
        assert_eq!(
            emitter
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|snapshot| snapshot.state.is_terminal())
                .count(),
            1
        );
    }

    struct SelectivePanicEmitter {
        state: RuntimeOperationState,
        progress: bool,
        events: Mutex<Vec<RuntimeOperationSnapshot>>,
    }

    impl OperationEventEmitter for SelectivePanicEmitter {
        fn emit(&self, snapshot: RuntimeOperationSnapshot) -> Result<(), ()> {
            if snapshot.state == self.state && (!self.progress || snapshot.progress.is_some()) {
                panic!("private emitter panic")
            }
            self.events.lock().unwrap().push(snapshot);
            Ok(())
        }
    }

    #[test]
    fn emitter_panics_never_change_successful_operation_truth() {
        for (state, progress) in [
            (RuntimeOperationState::Running, false),
            (RuntimeOperationState::Running, true),
            (RuntimeOperationState::Succeeded, false),
        ] {
            let manager = Arc::new(RuntimeOperationManager::default());
            let emitter = Arc::new(SelectivePanicEmitter {
                state,
                progress,
                events: Mutex::new(Vec::new()),
            });
            let selected_pipeline: Arc<dyn OperationPipeline> = if progress {
                Arc::new(FakePipeline {
                    preparation: Mutex::new(Some(Ok(FakePrepared {
                        progress: vec![
                            RuntimeOperationProgress {
                                phase: "opening".to_string(),
                                completed_units: None,
                                total_units: None,
                                message: "Opening runtime.".to_string(),
                            },
                            RuntimeOperationProgress {
                                phase: "verifying".to_string(),
                                completed_units: None,
                                total_units: None,
                                message: "Verifying runtime.".to_string(),
                            },
                        ],
                        result: Ok(()),
                        executed: None,
                        block: None,
                    }))),
                })
            } else {
                pipeline(Ok(()))
            };
            let admission = start_with_dependencies(
                Arc::clone(&manager),
                request(),
                emitter,
                &InlineSpawner,
                selected_pipeline,
            )
            .unwrap();
            let id = match admission {
                RuntimeOperationAdmission::Accepted { operation } => operation.operation_id,
                _ => panic!("expected acceptance"),
            };
            let terminal = manager.get_operation(&id).unwrap();
            assert_eq!(terminal.state, RuntimeOperationState::Succeeded);
            if progress {
                assert_eq!(terminal.revision, 5);
                assert_eq!(terminal.progress.as_ref().unwrap().phase, "verifying");
            }
            assert!(!serde_json::to_string(&terminal)
                .unwrap()
                .contains("private emitter panic"));
        }
    }

    #[test]
    fn terminalization_outcome_emits_only_applied_and_propagates_unknown() {
        let manager = RuntimeOperationManager::default();
        let emitter = RecordingEmitter::default();
        let queued = match manager
            .admit_operation("ollama", RuntimeOperationAction::Open, false)
            .unwrap()
        {
            RuntimeOperationAdmission::Accepted { operation } => operation,
            _ => panic!("expected acceptance"),
        };
        assert!(matches!(
            fail_operation(
                &manager,
                &queued.operation_id,
                operation_task_failed(),
                &emitter
            )
            .unwrap(),
            RuntimeOperationTerminalUpdate::Applied(_)
        ));
        assert_eq!(emitter.events.lock().unwrap().len(), 1);
        assert!(matches!(
            fail_operation(
                &manager,
                &queued.operation_id,
                operation_task_failed(),
                &emitter
            )
            .unwrap(),
            RuntimeOperationTerminalUpdate::AlreadyTerminal(_)
        ));
        assert_eq!(emitter.events.lock().unwrap().len(), 1);
        assert_eq!(
            fail_operation(&manager, "missing", operation_task_failed(), &emitter)
                .unwrap_err()
                .code,
            RuntimeErrorCode::OperationNotFound
        );
    }
}
