use crate::planner::PlanId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub type TimestampMs = u64;
pub type TaskContext = BTreeMap<String, Value>;
pub type TaskResult = Value;

static TASK_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as TimestampMs)
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    pub fn new() -> Self {
        let timestamp = now_ms();
        let process_id = process::id();
        let counter = TASK_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        Self(format!("task_{timestamp}_{process_id}_{counter:06}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskType {
    Ask,
    Do,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Created,
    Understanding,
    Planning,
    Ready,
    Executing,
    Verifying,
    Completed,
    Recovery,
    Failed,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPriority {
    Critical,
    High,

    #[default]
    Normal,

    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub task_type: TaskType,
    pub intent: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub context: TaskContext,
    pub active_plan_id: Option<PlanId>,
    pub result: Option<TaskResult>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl Task {
    pub fn new(task_type: TaskType, intent: impl Into<String>) -> Result<Self, TaskError> {
        let intent = intent.into().trim().to_owned();

        if intent.is_empty() {
            return Err(TaskError::EmptyIntent);
        }

        let timestamp = now_ms();

        Ok(Self {
            id: TaskId::new(),
            task_type,
            intent,
            status: TaskStatus::Created,
            priority: TaskPriority::Normal,
            context: TaskContext::new(),
            active_plan_id: None,
            result: None,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn can_transition_to(&self, next: TaskStatus) -> bool {
        if self.status.is_terminal() {
            return false;
        }

        if next == TaskStatus::Failed {
            return true;
        }

        if self.task_type == TaskType::Do
            && self.status == TaskStatus::Planning
            && next == TaskStatus::Ready
            && self.active_plan_id.is_none()
        {
            return false;
        }

        match (self.task_type, self.status, next) {
            (_, TaskStatus::Created, TaskStatus::Understanding) => true,

            (TaskType::Ask, TaskStatus::Understanding, TaskStatus::Ready) => true,

            (TaskType::Do, TaskStatus::Understanding, TaskStatus::Planning) => true,

            (_, TaskStatus::Planning, TaskStatus::Ready) => true,

            (_, TaskStatus::Ready, TaskStatus::Executing) => true,

            (_, TaskStatus::Executing, TaskStatus::Verifying | TaskStatus::Recovery) => true,

            (_, TaskStatus::Verifying, TaskStatus::Completed | TaskStatus::Recovery) => true,

            (_, TaskStatus::Recovery, TaskStatus::Executing) => true,

            _ => false,
        }
    }

    pub fn activate_plan(&mut self, plan_id: PlanId) {
        self.active_plan_id = Some(plan_id);
        self.updated_at = now_ms();
    }

    pub fn transition_to(&mut self, next: TaskStatus) -> Result<(), TaskError> {
        if self.task_type == TaskType::Do
            && self.status == TaskStatus::Planning
            && next == TaskStatus::Ready
            && self.active_plan_id.is_none()
        {
            return Err(TaskError::ActivePlanRequired);
        }

        if !self.can_transition_to(next) {
            return Err(TaskError::InvalidTransition {
                task_type: self.task_type,
                from: self.status,
                to: next,
            });
        }

        self.status = next;
        self.updated_at = now_ms();

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    EmptyIntent,
    ActivePlanRequired,

    InvalidTransition {
        task_type: TaskType,
        from: TaskStatus,
        to: TaskStatus,
    },
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIntent => formatter.write_str("task intent must not be empty"),

            Self::ActivePlanRequired => {
                formatter.write_str("do task requires an active plan before becoming ready")
            }

            Self::InvalidTransition {
                task_type,
                from,
                to,
            } => write!(
                formatter,
                "invalid {task_type:?} task transition \
                 from {from:?} to {to:?}"
            ),
        }
    }
}

impl Error for TaskError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_task_with_stable_defaults() {
        let task = Task::new(TaskType::Do, "send email").expect("task should be created");

        let task_id = task.id.clone();

        assert!(task.id.as_str().starts_with("task_"));
        assert_eq!(task.id, task_id);
        assert_eq!(task.status, TaskStatus::Created);
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.context.is_empty());
        assert!(task.active_plan_id.is_none());
        assert!(task.result.is_none());
        assert!(task.updated_at >= task.created_at);
    }

    #[test]
    fn rejects_empty_intent() {
        assert_eq!(Task::new(TaskType::Ask, "   "), Err(TaskError::EmptyIntent));
    }

    #[test]
    fn ask_task_can_skip_planning() {
        let mut task = Task::new(TaskType::Ask, "summarize document").unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Ready).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Verifying).unwrap();
        task.transition_to(TaskStatus::Completed).unwrap();

        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn do_task_requires_planning() {
        let mut task = Task::new(TaskType::Do, "send email").unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();

        assert!(matches!(
            task.transition_to(TaskStatus::Ready),
            Err(TaskError::InvalidTransition { .. })
        ));

        task.transition_to(TaskStatus::Planning).unwrap();

        assert!(!task.can_transition_to(TaskStatus::Ready));
        assert_eq!(
            task.transition_to(TaskStatus::Ready),
            Err(TaskError::ActivePlanRequired)
        );

        let plan_id = PlanId::new();
        task.activate_plan(plan_id.clone());

        assert_eq!(task.active_plan_id, Some(plan_id));
        assert!(task.can_transition_to(TaskStatus::Ready));

        task.transition_to(TaskStatus::Ready).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Verifying).unwrap();
        task.transition_to(TaskStatus::Completed).unwrap();

        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn supports_recovery_and_retry() {
        let mut task = Task::new(TaskType::Do, "download file").unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();
        task.activate_plan(PlanId::new());
        task.transition_to(TaskStatus::Ready).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Recovery).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Verifying).unwrap();
        task.transition_to(TaskStatus::Completed).unwrap();

        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn terminal_states_reject_additional_transitions() {
        let mut completed = Task::new(TaskType::Ask, "answer question").unwrap();

        completed.transition_to(TaskStatus::Understanding).unwrap();

        completed.transition_to(TaskStatus::Ready).unwrap();
        completed.transition_to(TaskStatus::Executing).unwrap();
        completed.transition_to(TaskStatus::Verifying).unwrap();
        completed.transition_to(TaskStatus::Completed).unwrap();

        assert!(completed.transition_to(TaskStatus::Failed).is_err());

        let mut failed = Task::new(TaskType::Do, "send email").unwrap();

        failed.transition_to(TaskStatus::Failed).unwrap();

        assert!(failed.transition_to(TaskStatus::Understanding).is_err());
    }

    #[test]
    fn serializes_and_deserializes_without_data_loss() {
        let mut task = Task::new(TaskType::Do, "organize files").unwrap();

        task.context
            .insert("path".to_owned(), json!("/Users/example"));

        task.activate_plan(PlanId::new());

        task.result = Some(json!({
            "organized": 12
        }));

        let serialized = serde_json::to_string(&task).unwrap();

        let deserialized: Task = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized, task);
    }
}
