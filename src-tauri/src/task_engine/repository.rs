use super::domain::{Task, TaskId};
use std::{collections::HashMap, error::Error, fmt, sync::RwLock};

pub trait TaskRepository: Send + Sync {
    fn create(&self, task: Task) -> Result<TaskId, TaskRepositoryError>;

    fn get(&self, task_id: &TaskId) -> Result<Option<Task>, TaskRepositoryError>;

    fn list(&self) -> Result<Vec<Task>, TaskRepositoryError>;

    fn update(&self, task: Task) -> Result<(), TaskRepositoryError>;

    fn delete(&self, task_id: &TaskId) -> Result<Task, TaskRepositoryError>;
}

#[derive(Debug, Default)]
pub struct InMemoryTaskRepository {
    tasks: RwLock<HashMap<TaskId, Task>>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> Result<usize, TaskRepositoryError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        Ok(tasks.len())
    }

    pub fn is_empty(&self) -> Result<bool, TaskRepositoryError> {
        self.len().map(|length| length == 0)
    }
}

impl TaskRepository for InMemoryTaskRepository {
    fn create(&self, task: Task) -> Result<TaskId, TaskRepositoryError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        if tasks.contains_key(&task.id) {
            return Err(TaskRepositoryError::AlreadyExists(task.id.clone()));
        }

        let task_id = task.id.clone();
        tasks.insert(task_id.clone(), task);

        Ok(task_id)
    }

    fn get(&self, task_id: &TaskId) -> Result<Option<Task>, TaskRepositoryError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        Ok(tasks.get(task_id).cloned())
    }

    fn list(&self) -> Result<Vec<Task>, TaskRepositoryError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        let mut result: Vec<Task> = tasks.values().cloned().collect();

        result.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });

        Ok(result)
    }

    fn update(&self, task: Task) -> Result<(), TaskRepositoryError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        if !tasks.contains_key(&task.id) {
            return Err(TaskRepositoryError::NotFound(task.id.clone()));
        }

        tasks.insert(task.id.clone(), task);

        Ok(())
    }

    fn delete(&self, task_id: &TaskId) -> Result<Task, TaskRepositoryError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| TaskRepositoryError::LockPoisoned)?;

        tasks
            .remove(task_id)
            .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRepositoryError {
    AlreadyExists(TaskId),
    NotFound(TaskId),
    LockPoisoned,
}

impl fmt::Display for TaskRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists(task_id) => {
                write!(formatter, "task {task_id} already exists")
            }
            Self::NotFound(task_id) => {
                write!(formatter, "task {task_id} was not found")
            }
            Self::LockPoisoned => formatter.write_str("task repository lock was poisoned"),
        }
    }
}

impl Error for TaskRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_engine::{TaskStatus, TaskType};

    fn task(intent: &str) -> Task {
        Task::new(TaskType::Do, intent).unwrap()
    }

    #[test]
    fn creates_and_retrieves_task() {
        let repository = InMemoryTaskRepository::new();
        let task = task("send email");
        let task_id = task.id.clone();

        repository.create(task.clone()).unwrap();

        assert_eq!(repository.get(&task_id).unwrap(), Some(task));
        assert_eq!(repository.len().unwrap(), 1);
    }

    #[test]
    fn rejects_duplicate_task_id() {
        let repository = InMemoryTaskRepository::new();
        let task = task("send email");

        repository.create(task.clone()).unwrap();

        assert_eq!(
            repository.create(task.clone()),
            Err(TaskRepositoryError::AlreadyExists(task.id))
        );
    }

    #[test]
    fn updates_existing_task() {
        let repository = InMemoryTaskRepository::new();
        let mut task = task("organize files");
        let task_id = task.id.clone();

        repository.create(task.clone()).unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();
        repository.update(task.clone()).unwrap();

        assert_eq!(repository.get(&task_id).unwrap(), Some(task));
    }

    #[test]
    fn rejects_update_for_unknown_task() {
        let repository = InMemoryTaskRepository::new();
        let task = task("unknown task");

        assert_eq!(
            repository.update(task.clone()),
            Err(TaskRepositoryError::NotFound(task.id))
        );
    }

    #[test]
    fn lists_tasks_in_deterministic_order() {
        let repository = InMemoryTaskRepository::new();

        let first = task("first");
        let second = task("second");

        repository.create(second.clone()).unwrap();
        repository.create(first.clone()).unwrap();

        let tasks = repository.list().unwrap();

        assert_eq!(tasks.len(), 2);

        let mut expected = vec![first, second];

        expected.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });

        assert_eq!(tasks, expected);
    }

    #[test]
    fn deletes_existing_task() {
        let repository = InMemoryTaskRepository::new();
        let task = task("delete me");
        let task_id = task.id.clone();

        repository.create(task.clone()).unwrap();

        assert_eq!(repository.delete(&task_id).unwrap(), task);
        assert_eq!(repository.get(&task_id).unwrap(), None);
        assert!(repository.is_empty().unwrap());
    }

    #[test]
    fn rejects_delete_for_unknown_task() {
        let repository = InMemoryTaskRepository::new();
        let task_id = task("missing").id;

        assert_eq!(
            repository.delete(&task_id),
            Err(TaskRepositoryError::NotFound(task_id))
        );
    }
}
