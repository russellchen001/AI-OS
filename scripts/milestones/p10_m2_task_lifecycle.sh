#!/usr/bin/env bash
set -euo pipefail

BRANCH="feature/p10-m2-task-lifecycle"
CARGO_TOML="src-tauri/Cargo.toml"
TASK_DIR="src-tauri/src/task_engine"

fail() {
  printf '\nERROR: %s\n' "$1" >&2
  exit 1
}

[[ -f "$CARGO_TOML" ]] || fail "找不到 $CARGO_TOML，请在 AI-OS/dashboard 仓库根目录运行。"
[[ -f "$TASK_DIR/domain.rs" ]] || fail "找不到 P10-M1 domain.rs，请先完成 P10-M1。"
[[ -f "$TASK_DIR/mod.rs" ]] || fail "找不到 task_engine/mod.rs。"

command -v cargo >/dev/null 2>&1 || fail "找不到 cargo。"
command -v git >/dev/null 2>&1 || fail "找不到 git。"

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  CURRENT_BRANCH="$(git branch --show-current)"

  printf '当前分支：%s\n' "$CURRENT_BRANCH"

  if [[ "$CURRENT_BRANCH" != "feature/p10-m1-task-domain" &&
        "$CURRENT_BRANCH" != "$BRANCH" ]]; then
    fail "请从 feature/p10-m1-task-domain 开始 P10-M2。当前分支是 $CURRENT_BRANCH"
  fi

  DIRTY_FILES="$(
    git status --porcelain |
      grep -v '^?? p10_m2_task_lifecycle\.sh$' || true
  )"

  if [[ -n "$DIRTY_FILES" ]]; then
    printf '\n当前存在其他未提交修改：\n%s\n' "$DIRTY_FILES"
    fail "请先清理工作区，再运行 P10-M2 脚本。"
  fi

  if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
    git switch "$BRANCH"
  else
    git switch -c "$BRANCH"
  fi
fi

for file in repository.rs events.rs lifecycle.rs; do
  if [[ -e "$TASK_DIR/$file" ]]; then
    fail "$TASK_DIR/$file 已存在。为防止覆盖，脚本已停止。"
  fi
done

cat > "$TASK_DIR/repository.rs" <<'RUST'
use super::domain::{Task, TaskId};
use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::RwLock,
};

pub trait TaskRepository: Send + Sync {
    fn create(&self, task: Task) -> Result<TaskId, TaskRepositoryError>;

    fn get(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<Task>, TaskRepositoryError>;

    fn list(&self) -> Result<Vec<Task>, TaskRepositoryError>;

    fn update(&self, task: Task) -> Result<(), TaskRepositoryError>;

    fn delete(
        &self,
        task_id: &TaskId,
    ) -> Result<Task, TaskRepositoryError>;
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
            return Err(TaskRepositoryError::AlreadyExists(
                task.id.clone(),
            ));
        }

        let task_id = task.id.clone();
        tasks.insert(task_id.clone(), task);

        Ok(task_id)
    }

    fn get(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<Task>, TaskRepositoryError> {
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

    fn delete(
        &self,
        task_id: &TaskId,
    ) -> Result<Task, TaskRepositoryError> {
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
            Self::LockPoisoned => {
                formatter.write_str("task repository lock was poisoned")
            }
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

        assert_eq!(
            repository.get(&task_id).unwrap(),
            Some(task)
        );
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

        assert_eq!(
            repository.get(&task_id).unwrap(),
            Some(task)
        );
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
RUST

cat > "$TASK_DIR/events.rs" <<'RUST'
use super::domain::{TaskId, TaskStatus};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    sync::Mutex,
};

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
            Self::LockPoisoned => {
                formatter.write_str("task event bus lock was poisoned")
            }
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

        let event = TaskEvent::Created {
            task_id: task.id,
        };

        event_bus.publish(event.clone()).unwrap();

        assert_eq!(event_bus.snapshot().unwrap(), vec![event]);
    }

    #[test]
    fn drains_events_without_leaving_duplicates() {
        let event_bus = InMemoryTaskEventBus::new();
        let task = Task::new(TaskType::Do, "send email").unwrap();

        event_bus
            .publish(TaskEvent::Created {
                task_id: task.id,
            })
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
RUST

cat > "$TASK_DIR/lifecycle.rs" <<'RUST'
use super::{
    domain::{Task, TaskError, TaskId, TaskStatus},
    events::{TaskEvent, TaskEventError, TaskEventSink},
    repository::{TaskRepository, TaskRepositoryError},
};
use std::{
    error::Error,
    fmt,
};

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
        Self {
            repository,
            events,
        }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn events(&self) -> &E {
        &self.events
    }

    pub fn create(
        &self,
        task: Task,
    ) -> Result<TaskId, TaskLifecycleError> {
        let task_id = self.repository.create(task)?;

        self.events.publish(TaskEvent::Created {
            task_id: task_id.clone(),
        })?;

        Ok(task_id)
    }

    pub fn get(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<Task>, TaskLifecycleError> {
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
            .ok_or_else(|| {
                TaskRepositoryError::NotFound(task_id.clone())
            })?;

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
    use crate::task_engine::{
        InMemoryTaskEventBus,
        InMemoryTaskRepository,
        TaskEvent,
        TaskType,
    };

    fn manager() -> TaskLifecycleManager<
        InMemoryTaskRepository,
        InMemoryTaskEventBus,
    > {
        TaskLifecycleManager::new(
            InMemoryTaskRepository::new(),
            InMemoryTaskEventBus::new(),
        )
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

        manager
            .transition(&task_id, TaskStatus::Ready)
            .unwrap();

        manager
            .transition(&task_id, TaskStatus::Executing)
            .unwrap();

        manager
            .transition(&task_id, TaskStatus::Verifying)
            .unwrap();

        manager.events().drain().unwrap();

        manager
            .transition(&task_id, TaskStatus::Completed)
            .unwrap();

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![
                TaskEvent::StatusChanged {
                    task_id: task_id.clone(),
                    from: TaskStatus::Verifying,
                    to: TaskStatus::Completed,
                },
                TaskEvent::Completed {
                    task_id,
                },
            ]
        );
    }

    #[test]
    fn failed_transition_emits_status_and_failed_events() {
        let manager = manager();
        let task = Task::new(TaskType::Do, "send email").unwrap();
        let task_id = manager.create(task).unwrap();

        manager.events().drain().unwrap();

        manager
            .transition(&task_id, TaskStatus::Failed)
            .unwrap();

        assert_eq!(
            manager.events().snapshot().unwrap(),
            vec![
                TaskEvent::StatusChanged {
                    task_id: task_id.clone(),
                    from: TaskStatus::Created,
                    to: TaskStatus::Failed,
                },
                TaskEvent::Failed {
                    task_id,
                },
            ]
        );
    }

    #[test]
    fn invalid_transition_does_not_update_or_emit_event() {
        let manager = manager();
        let task = Task::new(TaskType::Do, "send email").unwrap();
        let task_id = manager.create(task).unwrap();

        manager.events().drain().unwrap();

        let result = manager.transition(
            &task_id,
            TaskStatus::Completed,
        );

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
        let missing_id = Task::new(
            TaskType::Ask,
            "missing task",
        )
        .unwrap()
        .id;

        assert!(matches!(
            manager.transition(
                &missing_id,
                TaskStatus::Understanding,
            ),
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
RUST

cat > "$TASK_DIR/mod.rs" <<'RUST'
pub mod domain;
pub mod events;
pub mod lifecycle;
pub mod repository;

pub use domain::{
    Task, TaskContext, TaskError, TaskId, TaskPlan, TaskPriority,
    TaskResult, TaskStatus, TaskType, TimestampMs,
};

pub use events::{
    InMemoryTaskEventBus, TaskEvent, TaskEventError, TaskEventSink,
};

pub use lifecycle::{
    TaskLifecycleError, TaskLifecycleManager,
};

pub use repository::{
    InMemoryTaskRepository, TaskRepository, TaskRepositoryError,
};
RUST

printf '\n正在运行 cargo fmt...\n'
cargo fmt --manifest-path "$CARGO_TOML" --all

printf '\n正在运行 cargo check...\n'
cargo check --manifest-path "$CARGO_TOML"

printf '\n正在运行 P10-M2 Task Engine 测试...\n'
cargo test --manifest-path "$CARGO_TOML" task_engine

printf '\n正在运行完整 Rust 测试...\n'
cargo test --manifest-path "$CARGO_TOML"

printf '\n========================================\n'
printf 'P10-M2 已完成并通过自动验证。\n'
printf '========================================\n\n'

printf '新增文件：\n'
printf '  %s/repository.rs\n' "$TASK_DIR"
printf '  %s/events.rs\n' "$TASK_DIR"
printf '  %s/lifecycle.rs\n' "$TASK_DIR"
printf '  %s/mod.rs\n\n' "$TASK_DIR"

printf '检查修改：\n'
printf '  git status\n'
printf '  git diff -- src-tauri/src/task_engine\n\n'

printf '确认无误后提交：\n'
printf '  git add src-tauri/src/task_engine\n'
printf '  git commit -m "feat(task-engine): add P10-M2 repository and lifecycle"\n'
