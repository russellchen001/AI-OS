use crate::task_engine::TaskId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt, process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub type TimestampMs = u64;
pub type StepInput = BTreeMap<String, Value>;
pub type StepOutput = Value;

static PLAN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static STEP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as TimestampMs)
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanId(String);

impl PlanId {
    pub fn new() -> Self {
        let timestamp = now_ms();
        let process_id = process::id();
        let counter = PLAN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        Self(format!("plan_{timestamp}_{process_id}_{counter:06}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[cfg(test)]
    pub(crate) fn from_static(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl Default for PlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanStepId(String);

impl PlanStepId {
    pub fn new() -> Self {
        let timestamp = now_ms();
        let process_id = process::id();
        let counter = STEP_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        Self(format!("step_{timestamp}_{process_id}_{counter:06}"))
    }

    pub fn from_static(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for PlanStepId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlanStepId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStatus {
    #[default]
    Draft,

    Validated,
    Ready,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

impl PlanStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStepStatus {
    #[default]
    Pending,

    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

impl PlanStepStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, next),
            (Self::Pending, Self::Ready | Self::Cancelled)
                | (Self::Ready, Self::Running | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Completed | Self::Failed | Self::Cancelled
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: PlanStepId,
    pub name: String,
    pub description: String,
    pub capability: String,
    pub dependencies: Vec<PlanStepId>,
    pub input: StepInput,
    #[serde(default)]
    pub user_confirmed: bool,
    pub output: Option<StepOutput>,
    pub status: PlanStepStatus,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl PlanStep {
    pub fn new(
        name: impl Into<String>,
        capability: impl Into<String>,
    ) -> Result<Self, PlanDomainError> {
        let name = name.into().trim().to_owned();
        let capability = capability.into().trim().to_owned();

        if name.is_empty() {
            return Err(PlanDomainError::EmptyStepName);
        }

        if capability.is_empty() {
            return Err(PlanDomainError::EmptyCapability);
        }

        let timestamp = now_ms();

        Ok(Self {
            id: PlanStepId::new(),
            name,
            description: String::new(),
            capability,
            dependencies: Vec::new(),
            input: StepInput::new(),
            user_confirmed: false,
            output: None,
            status: PlanStepStatus::Pending,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn with_id(mut self, id: PlanStepId) -> Self {
        self.id = id;
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_user_confirmation(mut self, user_confirmed: bool) -> Self {
        self.user_confirmed = user_confirmed;
        self
    }

    pub fn depends_on(mut self, dependency: PlanStepId) -> Self {
        if !self.dependencies.contains(&dependency) {
            self.dependencies.push(dependency);
        }

        self
    }

    pub fn transition_to(&mut self, next: PlanStepStatus) -> Result<(), PlanDomainError> {
        if !self.status.can_transition_to(next) {
            return Err(PlanDomainError::InvalidStepStatusTransition {
                from: self.status,
                to: next,
            });
        }

        self.status = next;
        self.updated_at = now_ms();

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub task_id: TaskId,
    pub revision: u32,
    pub objective: String,
    pub status: PlanStatus,
    pub steps: Vec<PlanStep>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl Plan {
    pub fn new(
        task_id: TaskId,
        revision: u32,
        objective: impl Into<String>,
    ) -> Result<Self, PlanDomainError> {
        let objective = objective.into().trim().to_owned();

        if revision == 0 {
            return Err(PlanDomainError::InvalidRevision);
        }

        if objective.is_empty() {
            return Err(PlanDomainError::EmptyObjective);
        }

        let timestamp = now_ms();

        Ok(Self {
            id: PlanId::new(),
            task_id,
            revision,
            objective,
            status: PlanStatus::Draft,
            steps: Vec::new(),
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn can_transition_to(&self, next: PlanStatus) -> bool {
        if self.status.is_terminal() {
            return false;
        }

        matches!(
            (self.status, next),
            (PlanStatus::Draft, PlanStatus::Validated)
                | (PlanStatus::Validated, PlanStatus::Ready)
                | (
                    PlanStatus::Ready,
                    PlanStatus::Executing | PlanStatus::Cancelled
                )
                | (
                    PlanStatus::Executing,
                    PlanStatus::Completed | PlanStatus::Failed | PlanStatus::Cancelled
                )
        )
    }

    pub fn transition_to(&mut self, next: PlanStatus) -> Result<(), PlanDomainError> {
        if !self.can_transition_to(next) {
            return Err(PlanDomainError::InvalidStatusTransition {
                from: self.status,
                to: next,
            });
        }

        self.status = next;
        self.updated_at = now_ms();

        Ok(())
    }

    pub fn add_step(&mut self, step: PlanStep) -> Result<(), PlanDomainError> {
        if self.steps.iter().any(|existing| existing.id == step.id) {
            return Err(PlanDomainError::DuplicateStepId(step.id));
        }

        self.steps.push(step);
        self.updated_at = now_ms();

        Ok(())
    }

    pub fn step(&self, id: &PlanStepId) -> Option<&PlanStep> {
        self.steps.iter().find(|step| &step.id == id)
    }

    pub fn step_mut(&mut self, id: &PlanStepId) -> Option<&mut PlanStep> {
        self.steps.iter_mut().find(|step| &step.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanDomainError {
    InvalidStepStatusTransition {
        from: PlanStepStatus,
        to: PlanStepStatus,
    },

    EmptyObjective,
    InvalidRevision,
    EmptyStepName,
    EmptyCapability,
    DuplicateStepId(PlanStepId),

    InvalidStatusTransition {
        from: PlanStatus,
        to: PlanStatus,
    },
}

impl fmt::Display for PlanDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStepStatusTransition { from, to } => {
                write!(
                    formatter,
                    "invalid step status transition {:?} -> {:?}",
                    from, to
                )
            }

            Self::EmptyObjective => formatter.write_str("plan objective must not be empty"),

            Self::InvalidRevision => formatter.write_str("plan revision must be greater than zero"),

            Self::EmptyStepName => formatter.write_str("plan step name must not be empty"),

            Self::EmptyCapability => formatter.write_str("plan step capability must not be empty"),

            Self::DuplicateStepId(step_id) => {
                write!(formatter, "plan contains duplicate step id {step_id}")
            }

            Self::InvalidStatusTransition { from, to } => {
                write!(formatter, "plan cannot transition from {from:?} to {to:?}")
            }
        }
    }
}

impl std::error::Error for PlanDomainError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_plan_with_stable_defaults() {
        let task_id = TaskId::new();
        let plan = Plan::new(task_id.clone(), 1, "organize files").unwrap();

        assert!(plan.id.as_str().starts_with("plan_"));
        assert_eq!(plan.task_id, task_id);
        assert_eq!(plan.revision, 1);
        assert_eq!(plan.objective, "organize files");
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(plan.steps.is_empty());
        assert!(plan.updated_at >= plan.created_at);
    }

    #[test]
    fn rejects_empty_plan_objective() {
        assert_eq!(
            Plan::new(TaskId::new(), 1, "   "),
            Err(PlanDomainError::EmptyObjective)
        );
    }

    #[test]
    fn rejects_zero_revision() {
        assert_eq!(
            Plan::new(TaskId::new(), 0, "organize files"),
            Err(PlanDomainError::InvalidRevision)
        );
    }

    #[test]
    fn creates_step_with_stable_defaults() {
        let step = PlanStep::new("scan directory", "filesystem.scan").unwrap();

        assert!(step.id.as_str().starts_with("step_"));
        assert_eq!(step.status, PlanStepStatus::Pending);
        assert!(step.dependencies.is_empty());
        assert!(step.input.is_empty());
        assert!(step.output.is_none());
    }

    #[test]
    fn rejects_invalid_step_fields() {
        assert_eq!(
            PlanStep::new("", "filesystem.scan"),
            Err(PlanDomainError::EmptyStepName)
        );

        assert_eq!(
            PlanStep::new("scan directory", " "),
            Err(PlanDomainError::EmptyCapability)
        );
    }

    #[test]
    fn rejects_duplicate_step_ids() {
        let step_id = PlanStepId::from_static("scan");

        let first = PlanStep::new("scan directory", "filesystem.scan")
            .unwrap()
            .with_id(step_id.clone());

        let second = PlanStep::new("scan again", "filesystem.scan")
            .unwrap()
            .with_id(step_id.clone());

        let mut plan = Plan::new(TaskId::new(), 1, "organize files").unwrap();

        plan.add_step(first).unwrap();

        assert_eq!(
            plan.add_step(second),
            Err(PlanDomainError::DuplicateStepId(step_id))
        );
    }

    #[test]
    fn supports_valid_plan_lifecycle_transitions() {
        let mut plan = Plan::new(TaskId::new(), 1, "execute workflow").unwrap();

        assert!(plan.can_transition_to(PlanStatus::Validated));
        plan.transition_to(PlanStatus::Validated).unwrap();

        assert!(plan.can_transition_to(PlanStatus::Ready));
        plan.transition_to(PlanStatus::Ready).unwrap();

        assert!(plan.can_transition_to(PlanStatus::Executing));
        plan.transition_to(PlanStatus::Executing).unwrap();

        assert!(plan.can_transition_to(PlanStatus::Completed));
        plan.transition_to(PlanStatus::Completed).unwrap();

        assert_eq!(plan.status, PlanStatus::Completed);
        assert!(plan.status.is_terminal());
    }

    #[test]
    fn supports_executing_to_failed_transition() {
        let mut plan = Plan::new(TaskId::new(), 1, "execute workflow").unwrap();

        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        plan.transition_to(PlanStatus::Executing).unwrap();
        plan.transition_to(PlanStatus::Failed).unwrap();

        assert_eq!(plan.status, PlanStatus::Failed);
        assert!(plan.status.is_terminal());
    }

    #[test]
    fn supports_cancellation_from_ready_and_executing() {
        let mut ready = Plan::new(TaskId::new(), 1, "ready workflow").unwrap();

        ready.transition_to(PlanStatus::Validated).unwrap();
        ready.transition_to(PlanStatus::Ready).unwrap();
        ready.transition_to(PlanStatus::Cancelled).unwrap();

        assert_eq!(ready.status, PlanStatus::Cancelled);

        let mut executing = Plan::new(TaskId::new(), 1, "executing workflow").unwrap();

        executing.transition_to(PlanStatus::Validated).unwrap();
        executing.transition_to(PlanStatus::Ready).unwrap();
        executing.transition_to(PlanStatus::Executing).unwrap();
        executing.transition_to(PlanStatus::Cancelled).unwrap();

        assert_eq!(executing.status, PlanStatus::Cancelled);
    }

    #[test]
    fn rejects_invalid_plan_status_transitions_without_mutation() {
        let mut plan = Plan::new(TaskId::new(), 1, "execute workflow").unwrap();
        plan.updated_at = 0;

        assert_eq!(
            plan.transition_to(PlanStatus::Ready),
            Err(PlanDomainError::InvalidStatusTransition {
                from: PlanStatus::Draft,
                to: PlanStatus::Ready,
            })
        );

        assert_eq!(plan.status, PlanStatus::Draft);
        assert_eq!(plan.updated_at, 0);

        plan.transition_to(PlanStatus::Validated).unwrap();

        let validated_updated_at = plan.updated_at;

        assert_eq!(
            plan.transition_to(PlanStatus::Executing),
            Err(PlanDomainError::InvalidStatusTransition {
                from: PlanStatus::Validated,
                to: PlanStatus::Executing,
            })
        );

        assert_eq!(plan.status, PlanStatus::Validated);
        assert_eq!(plan.updated_at, validated_updated_at);
    }

    #[test]
    fn rejects_ready_to_completed_transition() {
        let mut plan = Plan::new(TaskId::new(), 1, "execute workflow").unwrap();

        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();

        assert_eq!(
            plan.transition_to(PlanStatus::Completed),
            Err(PlanDomainError::InvalidStatusTransition {
                from: PlanStatus::Ready,
                to: PlanStatus::Completed,
            })
        );

        assert_eq!(plan.status, PlanStatus::Ready);
    }

    #[test]
    fn terminal_plan_states_reject_additional_transitions() {
        for terminal_status in [
            PlanStatus::Completed,
            PlanStatus::Failed,
            PlanStatus::Cancelled,
        ] {
            let mut plan = Plan::new(TaskId::new(), 1, "terminal workflow").unwrap();

            plan.status = terminal_status;
            plan.updated_at = 0;

            assert_eq!(
                plan.transition_to(PlanStatus::Executing),
                Err(PlanDomainError::InvalidStatusTransition {
                    from: terminal_status,
                    to: PlanStatus::Executing,
                })
            );

            assert_eq!(plan.status, terminal_status);
            assert_eq!(plan.updated_at, 0);
        }
    }

    #[test]
    fn successful_transition_updates_timestamp() {
        let mut plan = Plan::new(TaskId::new(), 1, "timestamp workflow").unwrap();
        plan.updated_at = 0;

        plan.transition_to(PlanStatus::Validated).unwrap();

        assert_eq!(plan.status, PlanStatus::Validated);
        assert!(plan.updated_at > 0);
    }

    #[test]
    fn supports_valid_step_execution_lifecycle() {
        let mut step = PlanStep::new("execute task", "test.execute").unwrap();

        assert_eq!(step.status, PlanStepStatus::Pending);

        step.transition_to(PlanStepStatus::Ready).unwrap();

        step.transition_to(PlanStepStatus::Running).unwrap();

        step.transition_to(PlanStepStatus::Completed).unwrap();

        assert_eq!(step.status, PlanStepStatus::Completed);
    }

    #[test]
    fn supports_failed_step_execution() {
        let mut step = PlanStep::new("execute task", "test.execute").unwrap();

        step.transition_to(PlanStepStatus::Ready).unwrap();

        step.transition_to(PlanStepStatus::Running).unwrap();

        step.transition_to(PlanStepStatus::Failed).unwrap();

        assert_eq!(step.status, PlanStepStatus::Failed);
    }

    #[test]
    fn rejects_invalid_step_status_transition_without_mutation() {
        let mut step = PlanStep::new("execute task", "test.execute").unwrap();

        let result = step.transition_to(PlanStepStatus::Completed);

        assert!(matches!(
            result,
            Err(PlanDomainError::InvalidStepStatusTransition {
                from: PlanStepStatus::Pending,
                to: PlanStepStatus::Completed
            })
        ));

        assert_eq!(step.status, PlanStepStatus::Pending);
    }

    #[test]
    fn serializes_without_data_loss() {
        let scan_id = PlanStepId::from_static("scan");

        let mut scan = PlanStep::new("scan directory", "filesystem.scan")
            .unwrap()
            .with_id(scan_id.clone());

        scan.input
            .insert("path".to_owned(), json!("/Users/example"));

        let move_step = PlanStep::new("move files", "filesystem.move")
            .unwrap()
            .depends_on(scan_id);

        let mut plan = Plan::new(TaskId::new(), 1, "organize files").unwrap();

        plan.add_step(scan).unwrap();
        plan.add_step(move_step).unwrap();

        let serialized = serde_json::to_string(&plan).unwrap();
        let restored: Plan = serde_json::from_str(&serialized).unwrap();

        assert_eq!(restored, plan);
    }
}
