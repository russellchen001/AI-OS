use super::{
    domain::{Task, TaskError, TaskId, TaskStatus},
    events::{TaskEvent, TaskEventError, TaskEventSink},
    repository::{TaskRepository, TaskRepositoryError},
};
use std::{error::Error, fmt};

pub struct TaskLifecycleManager<R, E> {
    repository: R,
    events: E,
}

impl<R, E> TaskLifecycleManager<R, E>
where
    R: TaskRepository,
    E: TaskEventSink,
{
    pub fn new(repository: R, events: E) -> Self {
        Self { repository, events }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn events(&self) -> &E {
        &self.events
    }

    pub fn create(&self, task: Task) -> Result<TaskId, TaskLifecycleError> {
        let task_id = self.repository.create(task)?;

        self.events.publish(TaskEvent::Created {
            task_id: task_id.clone(),
        })?;

        Ok(task_id)
    }

    pub fn get(&self, task_id: &TaskId) -> Result<Option<Task>, TaskLifecycleError> {
        self.repository.get(task_id).map_err(Into::into)
    }

    pub fn list(&self) -> Result<Vec<Task>, TaskLifecycleError> {
        self.repository.list().map_err(Into::into)
    }

    pub fn transition(
        &self,
        task_id: &TaskId,
        next: TaskStatus,
    ) -> Result<Task, TaskLifecycleError> {
        let mut task = self
            .repository
            .get(task_id)?
            .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))?;

        let previous = task.status;

        task.transition_to(next)?;

        self.repository.update(task.clone())?;

        self.events.publish(TaskEvent::StatusChanged {
            task_id: task_id.clone(),
            from: previous,
            to: next,
        })?;

        match next {
            TaskStatus::Completed => {
                self.events.publish(TaskEvent::Completed {
                    task_id: task_id.clone(),
                })?;
            }

            TaskStatus::Failed => {
                self.events.publish(TaskEvent::Failed {
                    task_id: task_id.clone(),
                })?;
            }

            _ => {}
        }

        Ok(task)
    }
}

#[derive(Debug)]
pub enum TaskLifecycleError {
    Domain(TaskError),
    Repository(TaskRepositoryError),
    Event(TaskEventError),
}

impl fmt::Display for TaskLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => {
                write!(formatter, "task domain error: {error}")
            }
            Self::Repository(error) => {
                write!(formatter, "task repository error: {error}")
            }
            Self::Event(error) => {
                write!(formatter, "task event error: {error}")
            }
        }
    }
}

impl Error for TaskLifecycleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain(error) => Some(error),
            Self::Repository(error) => Some(error),
            Self::Event(error) => Some(error),
        }
    }
}

impl From<TaskError> for TaskLifecycleError {
    fn from(error: TaskError) -> Self {
        Self::Domain(error)
    }
}

impl From<TaskRepositoryError> for TaskLifecycleError {
    fn from(error: TaskRepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<TaskEventError> for TaskLifecycleError {
    fn from(error: TaskEventError) -> Self {
        Self::Event(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_engine::{InMemoryTaskEventBus, InMemoryTaskRepository, TaskEvent, TaskType};

    fn manager() -> TaskLifecycleManager<InMemoryTaskRepository, InMemoryTaskEventBus> {
        TaskLifecycleManager::new(InMemoryTaskRepository::new(), InMemoryTaskEventBus::new())
    }

    #[test]
    fn creates_task_and_emits_created_event() {
        let manager = manager();
        let task = Task::new(TaskType::Do, "send email").unwrap();
        let task_id = task.id.clone();

        assert_eq!(manager.create(task).unwrap(), task_id);

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![TaskEvent::Created {
                task_id: task_id.clone(),
            }]
        );

        assert!(manager.get(&task_id).unwrap().is_some());
    }

    #[test]
    fn transition_updates_repository_and_emits_event() {
        let manager = manager();
        let task = Task::new(TaskType::Ask, "answer question").unwrap();
        let task_id = manager.create(task).unwrap();

        manager.events().drain().unwrap();

        let updated = manager
            .transition(&task_id, TaskStatus::Understanding)
            .unwrap();

        assert_eq!(updated.status, TaskStatus::Understanding);

        assert_eq!(
            manager.get(&task_id).unwrap().unwrap().status,
            TaskStatus::Understanding
        );

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![TaskEvent::StatusChanged {
                task_id,
                from: TaskStatus::Created,
                to: TaskStatus::Understanding,
            }]
        );
    }

    #[test]
    fn completed_transition_emits_status_and_completed_events() {
        let manager = manager();
        let task = Task::new(TaskType::Ask, "answer question").unwrap();
        let task_id = manager.create(task).unwrap();

        manager
            .transition(&task_id, TaskStatus::Understanding)
            .unwrap();

        manager.transition(&task_id, TaskStatus::Ready).unwrap();

        manager.transition(&task_id, TaskStatus::Executing).unwrap();

        manager.transition(&task_id, TaskStatus::Verifying).unwrap();

        manager.events().drain().unwrap();

        manager.transition(&task_id, TaskStatus::Completed).unwrap();

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![
                TaskEvent::StatusChanged {
                    task_id: task_id.clone(),
                    from: TaskStatus::Verifying,
                    to: TaskStatus::Completed,
                },
                TaskEvent::Completed { task_id },
            ]
        );
    }

    #[test]
    fn failed_transition_emits_status_and_failed_events() {
        let manager = manager();
        let task = Task::new(TaskType::Do, "send email").unwrap();
        let task_id = manager.create(task).unwrap();

        manager.events().drain().unwrap();

        manager.transition(&task_id, TaskStatus::Failed).unwrap();

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![
                TaskEvent::StatusChanged {
                    task_id: task_id.clone(),
                    from: TaskStatus::Created,
                    to: TaskStatus::Failed,
                },
                TaskEvent::Failed { task_id },
            ]
        );
    }

    #[test]
    fn invalid_transition_does_not_update_or_emit_event() {
        let manager = manager();
        let task = Task::new(TaskType::Do, "send email").unwrap();
        let task_id = manager.create(task).unwrap();

        manager.events().drain().unwrap();

        let result = manager.transition(&task_id, TaskStatus::Completed);

        assert!(matches!(
            result,
            Err(TaskLifecycleError::Domain(
                TaskError::InvalidTransition { .. }
            ))
        ));

        assert_eq!(
            manager.get(&task_id).unwrap().unwrap().status,
            TaskStatus::Created
        );

        assert!(manager.events().snapshot().unwrap().is_empty());
    }

    #[test]
    fn unknown_task_returns_repository_error() {
        let manager = manager();
        let missing_id = Task::new(TaskType::Ask, "missing task").unwrap().id;

        assert!(matches!(
            manager.transition(&missing_id, TaskStatus::Understanding,),
            Err(TaskLifecycleError::Repository(
                TaskRepositoryError::NotFound(_)
            ))
        ));
    }

    #[test]
    fn lists_managed_tasks() {
        let manager = manager();

        manager
            .create(Task::new(TaskType::Ask, "first").unwrap())
            .unwrap();

        manager
            .create(Task::new(TaskType::Do, "second").unwrap())
            .unwrap();

        assert_eq!(manager.list().unwrap().len(), 2);
    }
}
