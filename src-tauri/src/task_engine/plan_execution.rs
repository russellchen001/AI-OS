use super::{Task, TaskError, TaskId, TaskStatus};
use crate::planner::{Plan, PlanId, PlanStatus};
use std::{error::Error, fmt};

pub struct TaskPlanExecutionPolicy;

impl TaskPlanExecutionPolicy {
    pub fn validate_start(task: &Task, plan: &Plan) -> Result<(), TaskPlanExecutionError> {
        validate_identity(task, plan)?;

        if task.status != TaskStatus::Ready {
            return Err(TaskPlanExecutionError::TaskNotReady {
                task_id: task.id.clone(),
                status: task.status,
            });
        }

        if plan.status != PlanStatus::Ready {
            return Err(TaskPlanExecutionError::PlanNotReady {
                plan_id: plan.id.clone(),
                status: plan.status,
            });
        }

        Ok(())
    }

    pub fn mark_task_executing(
        task: &mut Task,
        executing_plan: &Plan,
    ) -> Result<(), TaskPlanExecutionError> {
        validate_identity(task, executing_plan)?;

        if task.status != TaskStatus::Ready {
            return Err(TaskPlanExecutionError::TaskNotReady {
                task_id: task.id.clone(),
                status: task.status,
            });
        }

        if executing_plan.status != PlanStatus::Executing {
            return Err(TaskPlanExecutionError::PlanNotExecuting {
                plan_id: executing_plan.id.clone(),
                status: executing_plan.status,
            });
        }

        task.transition_to(TaskStatus::Executing)?;
        Ok(())
    }

    pub fn synchronize_after_plan_change(
        task: &mut Task,
        plan: &Plan,
    ) -> Result<TaskPlanSynchronization, TaskPlanExecutionError> {
        validate_identity(task, plan)?;

        if task.status != TaskStatus::Executing {
            return Err(TaskPlanExecutionError::TaskNotExecuting {
                task_id: task.id.clone(),
                status: task.status,
            });
        }

        match plan.status {
            PlanStatus::Executing => Ok(TaskPlanSynchronization::Unchanged),
            PlanStatus::Completed => {
                task.transition_to(TaskStatus::Verifying)?;
                Ok(TaskPlanSynchronization::TaskBecameVerifying)
            }
            PlanStatus::Failed => {
                task.transition_to(TaskStatus::Failed)?;
                Ok(TaskPlanSynchronization::TaskFailed)
            }
            PlanStatus::Cancelled => Err(TaskPlanExecutionError::PlanCancelledUnsupported {
                plan_id: plan.id.clone(),
            }),
            status @ (PlanStatus::Draft | PlanStatus::Validated | PlanStatus::Ready) => {
                Err(TaskPlanExecutionError::PlanNotExecutionOutcome {
                    plan_id: plan.id.clone(),
                    status,
                })
            }
        }
    }
}

fn validate_identity(task: &Task, plan: &Plan) -> Result<(), TaskPlanExecutionError> {
    let active_plan_id = task.active_plan_id.as_ref().ok_or_else(|| {
        TaskPlanExecutionError::TaskHasNoActivePlan {
            task_id: task.id.clone(),
        }
    })?;

    if active_plan_id != &plan.id {
        return Err(TaskPlanExecutionError::ActivePlanMismatch {
            task_id: task.id.clone(),
            active_plan_id: active_plan_id.clone(),
            supplied_plan_id: plan.id.clone(),
        });
    }

    if plan.task_id != task.id {
        return Err(TaskPlanExecutionError::PlanTaskMismatch {
            task_id: task.id.clone(),
            plan_id: plan.id.clone(),
            plan_task_id: plan.task_id.clone(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPlanSynchronization {
    Unchanged,
    TaskBecameVerifying,
    TaskFailed,
}

#[derive(Debug)]
pub enum TaskPlanExecutionError {
    TaskDomain(TaskError),
    TaskHasNoActivePlan {
        task_id: TaskId,
    },
    ActivePlanMismatch {
        task_id: TaskId,
        active_plan_id: PlanId,
        supplied_plan_id: PlanId,
    },
    PlanTaskMismatch {
        task_id: TaskId,
        plan_id: PlanId,
        plan_task_id: TaskId,
    },
    TaskNotReady {
        task_id: TaskId,
        status: TaskStatus,
    },
    TaskNotExecuting {
        task_id: TaskId,
        status: TaskStatus,
    },
    PlanNotReady {
        plan_id: PlanId,
        status: PlanStatus,
    },
    PlanNotExecuting {
        plan_id: PlanId,
        status: PlanStatus,
    },
    PlanNotExecutionOutcome {
        plan_id: PlanId,
        status: PlanStatus,
    },
    PlanCancelledUnsupported {
        plan_id: PlanId,
    },
}

impl fmt::Display for TaskPlanExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskDomain(error) => write!(formatter, "task domain error: {error}"),
            Self::TaskHasNoActivePlan { task_id } => {
                write!(formatter, "task {task_id} has no active plan")
            }
            Self::ActivePlanMismatch {
                task_id,
                active_plan_id,
                supplied_plan_id,
            } => write!(
                formatter,
                "task {task_id} has active plan {active_plan_id}, not supplied plan {supplied_plan_id}"
            ),
            Self::PlanTaskMismatch {
                task_id,
                plan_id,
                plan_task_id,
            } => write!(
                formatter,
                "plan {plan_id} belongs to task {plan_task_id}, not task {task_id}"
            ),
            Self::TaskNotReady { task_id, status } => {
                write!(formatter, "task {task_id} has status {status:?} and is not ready")
            }
            Self::TaskNotExecuting { task_id, status } => write!(
                formatter,
                "task {task_id} has status {status:?} and is not executing"
            ),
            Self::PlanNotReady { plan_id, status } => {
                write!(formatter, "plan {plan_id} has status {status:?} and is not ready")
            }
            Self::PlanNotExecuting { plan_id, status } => write!(
                formatter,
                "plan {plan_id} has status {status:?} and is not executing"
            ),
            Self::PlanNotExecutionOutcome { plan_id, status } => write!(
                formatter,
                "plan {plan_id} has status {status:?}, which is not an execution outcome"
            ),
            Self::PlanCancelledUnsupported { plan_id } => write!(
                formatter,
                "cancelled plan {plan_id} cannot be synchronized in this milestone"
            ),
        }
    }
}

impl Error for TaskPlanExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TaskDomain(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TaskError> for TaskPlanExecutionError {
    fn from(error: TaskError) -> Self {
        Self::TaskDomain(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planner::PlanStep, task_engine::TaskType};

    fn ready_pair() -> (Task, Plan) {
        let mut task = Task::new(TaskType::Do, "execute workflow").unwrap();
        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();

        let mut plan = Plan::new(task.id.clone(), 1, "execute workflow").unwrap();
        plan.add_step(PlanStep::new("execute", "test.execute").unwrap())
            .unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();

        task.activate_plan(plan.id.clone());
        task.transition_to(TaskStatus::Ready).unwrap();
        (task, plan)
    }

    fn executing_pair() -> (Task, Plan) {
        let (mut task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan).unwrap();
        (task, plan)
    }

    #[test]
    fn accepts_matching_task_and_active_plan() {
        let (task, plan) = ready_pair();
        assert!(validate_identity(&task, &plan).is_ok());
    }

    #[test]
    fn rejects_task_without_active_plan_without_mutation() {
        let task = Task::new(TaskType::Do, "execute workflow").unwrap();
        let plan = Plan::new(task.id.clone(), 1, "execute workflow").unwrap();
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::validate_start(&task, &plan),
            Err(TaskPlanExecutionError::TaskHasNoActivePlan { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn rejects_wrong_active_plan_without_mutation() {
        let (mut task, plan) = ready_pair();
        task.activate_plan(PlanId::new());
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::validate_start(&task, &plan),
            Err(TaskPlanExecutionError::ActivePlanMismatch { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn rejects_plan_owned_by_another_task_without_mutation() {
        let (mut task, _) = ready_pair();
        let plan = Plan::new(TaskId::new(), 1, "unrelated workflow").unwrap();
        task.activate_plan(plan.id.clone());
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::validate_start(&task, &plan),
            Err(TaskPlanExecutionError::PlanTaskMismatch { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn validate_start_accepts_ready_task_and_ready_plan() {
        let (task, plan) = ready_pair();
        assert!(TaskPlanExecutionPolicy::validate_start(&task, &plan).is_ok());
    }

    #[test]
    fn validate_start_rejects_non_ready_task_without_mutation() {
        let (mut task, plan) = ready_pair();
        task.transition_to(TaskStatus::Executing).unwrap();
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::validate_start(&task, &plan),
            Err(TaskPlanExecutionError::TaskNotReady { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn validate_start_rejects_non_ready_plan_without_mutation() {
        let (task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::validate_start(&task, &plan),
            Err(TaskPlanExecutionError::PlanNotReady { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn validate_start_does_not_mutate_task_or_plan() {
        let (task, plan) = ready_pair();
        let original_task = task.clone();
        let original_plan = plan.clone();
        TaskPlanExecutionPolicy::validate_start(&task, &plan).unwrap();
        assert_eq!(task, original_task);
        assert_eq!(plan, original_plan);
    }

    #[test]
    fn mark_task_executing_transitions_ready_task_to_executing() {
        let (mut task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan).unwrap();
        assert_eq!(task.status, TaskStatus::Executing);
    }

    #[test]
    fn mark_task_executing_requires_executing_plan() {
        let (mut task, plan) = ready_pair();
        assert!(matches!(
            TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan),
            Err(TaskPlanExecutionError::PlanNotExecuting { .. })
        ));
    }

    #[test]
    fn mark_task_executing_requires_ready_task() {
        let (mut task, mut plan) = ready_pair();
        task.transition_to(TaskStatus::Executing).unwrap();
        plan.transition_to(PlanStatus::Executing).unwrap();
        assert!(matches!(
            TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan),
            Err(TaskPlanExecutionError::TaskNotReady { .. })
        ));
    }

    #[test]
    fn rejected_mark_executing_does_not_mutate_task() {
        let (mut task, plan) = ready_pair();
        let original = task.clone();
        let _ = TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan);
        assert_eq!(task, original);
    }

    #[test]
    fn executing_plan_leaves_executing_task_unchanged() {
        let (mut task, plan) = executing_pair();
        let original = task.clone();
        assert_eq!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan).unwrap(),
            TaskPlanSynchronization::Unchanged
        );
        assert_eq!(task, original);
    }

    #[test]
    fn completed_plan_moves_executing_task_to_verifying() {
        let (mut task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Completed).unwrap();
        assert_eq!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan).unwrap(),
            TaskPlanSynchronization::TaskBecameVerifying
        );
        assert_eq!(task.status, TaskStatus::Verifying);
    }

    #[test]
    fn completed_plan_does_not_complete_task_directly() {
        let (mut task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Completed).unwrap();
        TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan).unwrap();
        assert_ne!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn failed_plan_moves_executing_task_to_failed() {
        let (mut task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Failed).unwrap();
        assert_eq!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan).unwrap(),
            TaskPlanSynchronization::TaskFailed
        );
        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[test]
    fn cancelled_plan_returns_unsupported_error_without_mutation() {
        let (mut task, mut plan) = executing_pair();
        plan.transition_to(PlanStatus::Cancelled).unwrap();
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::PlanCancelledUnsupported { .. })
        ));
        assert_eq!(task, original);
    }

    fn assert_not_execution_outcome(status: PlanStatus) {
        let (mut task, _) = executing_pair();
        let mut plan = Plan::new(task.id.clone(), 1, "outcome").unwrap();
        match status {
            PlanStatus::Draft => {}
            PlanStatus::Validated => {
                plan.transition_to(PlanStatus::Validated).unwrap();
            }
            PlanStatus::Ready => {
                plan.transition_to(PlanStatus::Validated).unwrap();
                plan.transition_to(PlanStatus::Ready).unwrap();
            }
            _ => unreachable!(),
        }
        task.activate_plan(plan.id.clone());
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::PlanNotExecutionOutcome { status: actual, .. })
                if actual == status
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn draft_plan_is_not_execution_outcome() {
        assert_not_execution_outcome(PlanStatus::Draft);
    }

    #[test]
    fn validated_plan_is_not_execution_outcome() {
        assert_not_execution_outcome(PlanStatus::Validated);
    }

    #[test]
    fn ready_plan_is_not_execution_outcome() {
        assert_not_execution_outcome(PlanStatus::Ready);
    }

    #[test]
    fn synchronization_requires_executing_task() {
        let (mut task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::TaskNotExecuting { .. })
        ));
    }

    #[test]
    fn synchronization_rejects_wrong_active_plan_without_mutation() {
        let (mut task, plan) = executing_pair();
        task.activate_plan(PlanId::new());
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::ActivePlanMismatch { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn synchronization_rejects_plan_task_mismatch_without_mutation() {
        let (mut task, _) = executing_pair();
        let mut plan = Plan::new(TaskId::new(), 1, "unrelated workflow").unwrap();
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        plan.transition_to(PlanStatus::Executing).unwrap();
        task.activate_plan(plan.id.clone());
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::PlanTaskMismatch { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn all_task_mutations_use_legal_task_transitions() {
        let (mut task, mut plan) = ready_pair();
        plan.transition_to(PlanStatus::Executing).unwrap();
        assert!(task.can_transition_to(TaskStatus::Executing));
        TaskPlanExecutionPolicy::mark_task_executing(&mut task, &plan).unwrap();
        plan.transition_to(PlanStatus::Completed).unwrap();
        assert!(task.can_transition_to(TaskStatus::Verifying));
        TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan).unwrap();
    }

    #[test]
    fn terminal_task_is_not_mutated_by_rejected_synchronization() {
        let (mut task, plan) = executing_pair();
        task.transition_to(TaskStatus::Failed).unwrap();
        let original = task.clone();
        assert!(matches!(
            TaskPlanExecutionPolicy::synchronize_after_plan_change(&mut task, &plan),
            Err(TaskPlanExecutionError::TaskNotExecuting { .. })
        ));
        assert_eq!(task, original);
    }

    #[test]
    fn error_display_messages_contain_relevant_ids_and_states() {
        let (task, plan) = ready_pair();
        let error = TaskPlanExecutionError::PlanNotExecuting {
            plan_id: plan.id.clone(),
            status: PlanStatus::Ready,
        };
        let message = error.to_string();
        assert!(message.contains(plan.id.as_str()));
        assert!(message.contains("Ready"));

        let error = TaskPlanExecutionError::TaskNotReady {
            task_id: task.id.clone(),
            status: TaskStatus::Planning,
        };
        let message = error.to_string();
        assert!(message.contains(task.id.as_str()));
        assert!(message.contains("Planning"));
    }

    #[test]
    fn task_plan_execution_error_source_exposes_task_error_only_for_task_domain() {
        let domain = TaskPlanExecutionError::from(TaskError::ActivePlanRequired);
        assert!(domain.source().is_some());

        let (task, _) = ready_pair();
        let policy = TaskPlanExecutionError::TaskHasNoActivePlan { task_id: task.id };
        assert!(policy.source().is_none());
    }

    #[test]
    fn task_plan_synchronization_equality_is_deterministic() {
        assert_eq!(
            TaskPlanSynchronization::Unchanged,
            TaskPlanSynchronization::Unchanged
        );
        assert_ne!(
            TaskPlanSynchronization::TaskBecameVerifying,
            TaskPlanSynchronization::TaskFailed
        );
    }
}
