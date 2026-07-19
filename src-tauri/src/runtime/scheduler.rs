use std::{
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex},
};

type RuntimeTask = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct SchedulerState {
    pending: VecDeque<RuntimeTask>,
    running: bool,
}

#[derive(Clone, Default)]
pub(crate) struct RuntimeScheduler {
    state: Arc<Mutex<SchedulerState>>,
}

impl RuntimeScheduler {
    pub(crate) fn enqueue(&self, task: RuntimeTask) -> Result<(), ()> {
        let should_dispatch = {
            let mut state = self.state.lock().map_err(|_| ())?;
            state.pending.push_back(task);
            if state.running {
                false
            } else {
                state.running = true;
                true
            }
        };
        if should_dispatch {
            let scheduler = self.clone();
            tauri::async_runtime::spawn_blocking(move || scheduler.dispatch());
        }
        Ok(())
    }

    fn dispatch(&self) {
        while let Some(task) = self.next() {
            let _ = catch_unwind(AssertUnwindSafe(task));
        }
    }

    fn next(&self) -> Option<RuntimeTask> {
        let mut state = self.state.lock().ok()?;
        match state.pending.pop_front() {
            Some(task) => Some(task),
            None => {
                state.running = false;
                None
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.pending.len())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn running(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.running)
            .unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn shares_state_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc, Mutex},
        time::Duration,
    };

    #[test]
    fn tasks_dispatch_in_fifo_order_without_parallel_execution() {
        let scheduler = RuntimeScheduler::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let first_order = Arc::clone(&order);
        scheduler
            .enqueue(Box::new(move || {
                first_order.lock().unwrap().push(1);
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let second_order = Arc::clone(&order);
        scheduler
            .enqueue(Box::new(move || {
                second_order.lock().unwrap().push(2);
                done_tx.send(()).unwrap();
            }))
            .unwrap();
        assert!(scheduler.running());
        assert_eq!(scheduler.pending(), 1);
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn panicking_task_does_not_block_the_next_task() {
        let scheduler = RuntimeScheduler::default();
        let (done_tx, done_rx) = mpsc::channel();
        scheduler
            .enqueue(Box::new(|| panic!("task panic")))
            .unwrap();
        scheduler
            .enqueue(Box::new(move || done_tx.send(()).unwrap()))
            .unwrap();
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }
}
