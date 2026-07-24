use super::domain::{TaskId, TaskStatus};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, sync::Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskEvent {
    Created {
        task_id: TaskId,
    },

    StatusChanged {
        task_id: TaskId,
        from: TaskStatus,
        to: TaskStatus,
    },

    Completed {
        task_id: TaskId,
    },

    Failed {
        task_id: TaskId,
    },
}

pub trait TaskEventSink: Send + Sync {
    fn publish(&self, event: TaskEvent) -> Result<(), TaskEventError>;
}

#[derive(Debug, Default)]
pub struct InMemoryTaskEventBus {
    events: Mutex<Vec<TaskEvent>>,
}

impl InMemoryTaskEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Result<Vec<TaskEvent>, TaskEventError> {
        let events = self
            .events
            .lock()
            .map_err(|_| TaskEventError::LockPoisoned)?;

        Ok(events.clone())
    }

    pub fn drain(&self) -> Result<Vec<TaskEvent>, TaskEventError> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| TaskEventError::LockPoisoned)?;

        Ok(events.drain(..).collect())
    }
}

impl TaskEventSink for InMemoryTaskEventBus {
    fn publish(&self, event: TaskEvent) -> Result<(), TaskEventError> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| TaskEventError::LockPoisoned)?;

        events.push(event);

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEventError {
    LockPoisoned,
}

impl fmt::Display for TaskEventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockPoisoned => formatter.write_str("task event bus lock was poisoned"),
        }
    }
}

impl Error for TaskEventError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_engine::{Task, TaskType};

    #[test]
    fn publishes_and_snapshots_events() {
        let event_bus = InMemoryTaskEventBus::new();
        let task = Task::new(TaskType::Ask, "answer question").unwrap();

        let event = TaskEvent::Created { task_id: task.id };

        event_bus.publish(event.clone()).unwrap();

        assert_eq!(event_bus.snapshot().unwrap(), vec![event]);
    }

    #[test]
    fn drains_events_without_leaving_duplicates() {
        let event_bus = InMemoryTaskEventBus::new();
        let task = Task::new(TaskType::Do, "send email").unwrap();

        event_bus
            .publish(TaskEvent::Created { task_id: task.id })
            .unwrap();

        assert_eq!(event_bus.drain().unwrap().len(), 1);
        assert!(event_bus.snapshot().unwrap().is_empty());
    }

    #[test]
    fn serializes_event_with_stable_contract() {
        let task = Task::new(TaskType::Ask, "answer question").unwrap();

        let event = TaskEvent::StatusChanged {
            task_id: task.id,
            from: TaskStatus::Created,
            to: TaskStatus::Understanding,
        };

        let value = serde_json::to_value(event).unwrap();

        assert_eq!(value["type"], "STATUS_CHANGED");
        assert_eq!(value["from"], "CREATED");
        assert_eq!(value["to"], "UNDERSTANDING");
    }
}
