use std::sync::Arc;

use super::operations::RuntimeOperationManager;

#[derive(Clone)]
pub struct RuntimeExecutionState {
    manager: Arc<RuntimeOperationManager>,
}

impl Default for RuntimeExecutionState {
    fn default() -> Self {
        Self {
            manager: Arc::new(RuntimeOperationManager::default()),
        }
    }
}

impl RuntimeExecutionState {
    #[allow(dead_code)]
    pub(crate) fn manager(&self) -> Arc<RuntimeOperationManager> {
        Arc::clone(&self.manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_only_manager_arc() {
        let state = RuntimeExecutionState::default();
        let clone = state.clone();

        assert!(Arc::ptr_eq(&state.manager(), &clone.manager()));
        assert_eq!(
            std::mem::size_of::<RuntimeExecutionState>(),
            std::mem::size_of::<Arc<RuntimeOperationManager>>()
        );
    }
}
