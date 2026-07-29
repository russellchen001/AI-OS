use super::{
    Plan, PlanDomainError, PlanId, PlanRepository, PlanRepositoryError, PlanStatus, PlanStepId,
    PlanStepStatus, StepOutput,
};
use std::{collections::HashSet, error::Error, fmt};

pub struct PlanExecutionCoordinator<P> {
    plans: P,
}

impl<P> PlanExecutionCoordinator<P>
where
    P: PlanRepository,
{
    pub fn new(plans: P) -> Self {
        Self { plans }
    }

    pub fn plan_repository(&self) -> &P {
        &self.plans
    }

    pub fn start_plan(&self, plan_id: &PlanId) -> Result<Plan, PlanExecutionError> {
        let mut plan = self.load_plan(plan_id)?;

        prepare_plan_start(&mut plan)?;
        self.persist(plan)
    }

    pub fn start_step(
        &self,
        plan_id: &PlanId,
        step_id: &PlanStepId,
    ) -> Result<Plan, PlanExecutionError> {
        let mut plan = self.load_executing_plan(plan_id)?;
        let step = plan
            .step_mut(step_id)
            .ok_or_else(|| PlanExecutionError::StepNotFound {
                plan_id: plan_id.clone(),
                step_id: step_id.clone(),
            })?;

        if step.status != PlanStepStatus::Ready {
            return Err(PlanExecutionError::StepNotReady {
                step_id: step.id.clone(),
                status: step.status,
            });
        }

        step.transition_to(PlanStepStatus::Running)?;
        self.persist(plan)
    }

    pub fn complete_step(
        &self,
        plan_id: &PlanId,
        step_id: &PlanStepId,
        output: StepOutput,
    ) -> Result<Plan, PlanExecutionError> {
        self.complete_step_with_output(plan_id, step_id, Some(output))
    }

    pub fn complete_step_without_output(
        &self,
        plan_id: &PlanId,
        step_id: &PlanStepId,
    ) -> Result<Plan, PlanExecutionError> {
        self.complete_step_with_output(plan_id, step_id, None)
    }

    fn complete_step_with_output(
        &self,
        plan_id: &PlanId,
        step_id: &PlanStepId,
        output: Option<StepOutput>,
    ) -> Result<Plan, PlanExecutionError> {
        let mut plan = self.load_executing_plan(plan_id)?;
        let step = plan
            .step_mut(step_id)
            .ok_or_else(|| PlanExecutionError::StepNotFound {
                plan_id: plan_id.clone(),
                step_id: step_id.clone(),
            })?;

        if step.status != PlanStepStatus::Running {
            return Err(PlanExecutionError::StepNotRunning {
                step_id: step.id.clone(),
                status: step.status,
            });
        }

        step.output = output;
        step.transition_to(PlanStepStatus::Completed)?;
        promote_ready_steps(&mut plan)?;

        if plan
            .steps
            .iter()
            .all(|step| step.status == PlanStepStatus::Completed)
        {
            plan.transition_to(PlanStatus::Completed)?;
        }

        self.persist(plan)
    }

    pub fn fail_step(
        &self,
        plan_id: &PlanId,
        step_id: &PlanStepId,
    ) -> Result<Plan, PlanExecutionError> {
        let mut plan = self.load_executing_plan(plan_id)?;
        let step = plan
            .step_mut(step_id)
            .ok_or_else(|| PlanExecutionError::StepNotFound {
                plan_id: plan_id.clone(),
                step_id: step_id.clone(),
            })?;

        if step.status != PlanStepStatus::Running {
            return Err(PlanExecutionError::StepNotRunning {
                step_id: step.id.clone(),
                status: step.status,
            });
        }

        step.transition_to(PlanStepStatus::Failed)?;
        cancel_non_terminal_steps(&mut plan)?;
        plan.transition_to(PlanStatus::Failed)?;
        self.persist(plan)
    }

    pub fn cancel_plan(&self, plan_id: &PlanId) -> Result<Plan, PlanExecutionError> {
        let mut plan = self.load_plan(plan_id)?;

        if !matches!(plan.status, PlanStatus::Ready | PlanStatus::Executing) {
            return Err(PlanExecutionError::PlanCannotBeCancelled {
                plan_id: plan.id,
                status: plan.status,
            });
        }

        cancel_non_terminal_steps(&mut plan)?;
        plan.transition_to(PlanStatus::Cancelled)?;
        self.persist(plan)
    }

    fn load_plan(&self, plan_id: &PlanId) -> Result<Plan, PlanExecutionError> {
        self.plans
            .get(plan_id)?
            .ok_or_else(|| PlanRepositoryError::NotFound(plan_id.clone()).into())
    }

    fn load_executing_plan(&self, plan_id: &PlanId) -> Result<Plan, PlanExecutionError> {
        let plan = self.load_plan(plan_id)?;

        if plan.status != PlanStatus::Executing {
            return Err(PlanExecutionError::PlanNotExecuting {
                plan_id: plan.id,
                status: plan.status,
            });
        }

        Ok(plan)
    }

    fn persist(&self, plan: Plan) -> Result<Plan, PlanExecutionError> {
        self.plans.update(plan.clone())?;
        Ok(plan)
    }
}

pub(crate) fn prepare_plan_start(plan: &mut Plan) -> Result<(), PlanExecutionError> {
    if plan.status != PlanStatus::Ready {
        return Err(PlanExecutionError::PlanNotReady {
            plan_id: plan.id.clone(),
            status: plan.status,
        });
    }

    plan.transition_to(PlanStatus::Executing)?;

    for step in &mut plan.steps {
        if step.status == PlanStepStatus::Pending && step.dependencies.is_empty() {
            step.transition_to(PlanStepStatus::Ready)?;
        }
    }

    Ok(())
}

fn promote_ready_steps(plan: &mut Plan) -> Result<(), PlanDomainError> {
    let completed = plan
        .steps
        .iter()
        .filter(|step| step.status == PlanStepStatus::Completed)
        .map(|step| step.id.clone())
        .collect::<HashSet<_>>();

    for step in &mut plan.steps {
        if step.status == PlanStepStatus::Pending
            && step
                .dependencies
                .iter()
                .all(|dependency| completed.contains(dependency))
        {
            step.transition_to(PlanStepStatus::Ready)?;
        }
    }

    Ok(())
}

fn cancel_non_terminal_steps(plan: &mut Plan) -> Result<(), PlanDomainError> {
    for step in &mut plan.steps {
        if !step.status.is_terminal() {
            step.transition_to(PlanStepStatus::Cancelled)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum PlanExecutionError {
    Repository(PlanRepositoryError),
    Domain(PlanDomainError),
    PlanNotReady {
        plan_id: PlanId,
        status: PlanStatus,
    },
    PlanNotExecuting {
        plan_id: PlanId,
        status: PlanStatus,
    },
    PlanCannotBeCancelled {
        plan_id: PlanId,
        status: PlanStatus,
    },
    StepNotFound {
        plan_id: PlanId,
        step_id: PlanStepId,
    },
    StepNotReady {
        step_id: PlanStepId,
        status: PlanStepStatus,
    },
    StepNotRunning {
        step_id: PlanStepId,
        status: PlanStepStatus,
    },
}

impl fmt::Display for PlanExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => write!(formatter, "plan repository error: {error}"),
            Self::Domain(error) => write!(formatter, "plan domain error: {error}"),
            Self::PlanNotReady { plan_id, status } => {
                write!(
                    formatter,
                    "plan {plan_id} has status {status:?} and is not ready"
                )
            }
            Self::PlanNotExecuting { plan_id, status } => write!(
                formatter,
                "plan {plan_id} has status {status:?} and is not executing"
            ),
            Self::PlanCannotBeCancelled { plan_id, status } => write!(
                formatter,
                "plan {plan_id} has status {status:?} and cannot be cancelled"
            ),
            Self::StepNotFound { plan_id, step_id } => {
                write!(formatter, "step {step_id} was not found in plan {plan_id}")
            }
            Self::StepNotReady { step_id, status } => {
                write!(
                    formatter,
                    "step {step_id} has status {status:?} and is not ready"
                )
            }
            Self::StepNotRunning { step_id, status } => write!(
                formatter,
                "step {step_id} has status {status:?} and is not running"
            ),
        }
    }
}

impl Error for PlanExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Domain(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PlanRepositoryError> for PlanExecutionError {
    fn from(error: PlanRepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<PlanDomainError> for PlanExecutionError {
    fn from(error: PlanDomainError) -> Self {
        Self::Domain(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        planner::{InMemoryPlanRepository, PlanStep},
        task_engine::TaskId,
    };
    use serde_json::json;

    fn step(id: &str) -> PlanStep {
        PlanStep::new(id, format!("test.{id}"))
            .unwrap()
            .with_id(PlanStepId::from_static(id))
    }

    fn ready_plan(steps: Vec<PlanStep>) -> Plan {
        let mut plan = Plan::new(TaskId::new(), 1, "execute plan").unwrap();
        for step in steps {
            plan.add_step(step).unwrap();
        }
        plan.transition_to(PlanStatus::Validated).unwrap();
        plan.transition_to(PlanStatus::Ready).unwrap();
        plan
    }

    fn coordinator_with(plan: Plan) -> PlanExecutionCoordinator<InMemoryPlanRepository> {
        let repository = InMemoryPlanRepository::new();
        repository.create(plan).unwrap();
        PlanExecutionCoordinator::new(repository)
    }

    fn executing_plan(
        steps: Vec<PlanStep>,
    ) -> (PlanExecutionCoordinator<InMemoryPlanRepository>, Plan) {
        let plan = ready_plan(steps);
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let plan = coordinator.start_plan(&plan_id).unwrap();
        (coordinator, plan)
    }

    fn persisted(
        coordinator: &PlanExecutionCoordinator<InMemoryPlanRepository>,
        plan_id: &PlanId,
    ) -> Plan {
        coordinator.plan_repository().get(plan_id).unwrap().unwrap()
    }

    fn start(
        coordinator: &PlanExecutionCoordinator<InMemoryPlanRepository>,
        plan: &Plan,
        step_id: &str,
    ) -> Plan {
        coordinator
            .start_step(&plan.id, &PlanStepId::from_static(step_id))
            .unwrap()
    }

    #[test]
    fn start_plan_transitions_ready_plan_to_executing() {
        let plan = ready_plan(vec![step("a")]);
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let updated = coordinator.start_plan(&plan_id).unwrap();
        assert_eq!(updated.status, PlanStatus::Executing);
        assert_eq!(persisted(&coordinator, &plan_id), updated);
    }

    #[test]
    fn start_plan_marks_root_steps_ready() {
        let plan = ready_plan(vec![step("a"), step("b")]);
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let updated = coordinator.start_plan(&plan_id).unwrap();
        assert!(updated
            .steps
            .iter()
            .all(|step| step.status == PlanStepStatus::Ready));
    }

    #[test]
    fn start_plan_keeps_dependent_steps_pending() {
        let plan = ready_plan(vec![
            step("a"),
            step("b").depends_on(PlanStepId::from_static("a")),
        ]);
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let updated = coordinator.start_plan(&plan_id).unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("b")).unwrap().status,
            PlanStepStatus::Pending
        );
    }

    #[test]
    fn rejects_start_for_non_ready_plan_without_mutation() {
        let mut plan = ready_plan(vec![step("a")]);
        plan.transition_to(PlanStatus::Cancelled).unwrap();
        let original = plan.clone();
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        assert!(matches!(
            coordinator.start_plan(&plan_id),
            Err(PlanExecutionError::PlanNotReady { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan_id), original);
    }

    #[test]
    fn rejects_start_for_unknown_plan() {
        let coordinator = PlanExecutionCoordinator::new(InMemoryPlanRepository::new());
        assert!(matches!(
            coordinator.start_plan(&PlanId::new()),
            Err(PlanExecutionError::Repository(
                PlanRepositoryError::NotFound(_)
            ))
        ));
    }

    #[test]
    fn start_step_transitions_ready_step_to_running() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        let updated = start(&coordinator, &plan, "a");
        assert_eq!(
            updated.step(&PlanStepId::from_static("a")).unwrap().status,
            PlanStepStatus::Running
        );
        assert_eq!(persisted(&coordinator, &plan.id), updated);
    }

    #[test]
    fn rejects_start_step_for_non_executing_plan_without_mutation() {
        let plan = ready_plan(vec![step("a")]);
        let original = plan.clone();
        let coordinator = coordinator_with(plan.clone());
        assert!(matches!(
            coordinator.start_step(&plan.id, &PlanStepId::from_static("a")),
            Err(PlanExecutionError::PlanNotExecuting { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan.id), original);
    }

    #[test]
    fn rejects_start_step_when_step_is_not_ready_without_mutation() {
        let (coordinator, plan) = executing_plan(vec![
            step("a"),
            step("b").depends_on(PlanStepId::from_static("a")),
        ]);
        assert!(matches!(
            coordinator.start_step(&plan.id, &PlanStepId::from_static("b")),
            Err(PlanExecutionError::StepNotReady { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan.id), plan);
    }

    #[test]
    fn rejects_start_for_unknown_step_without_mutation() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        assert!(matches!(
            coordinator.start_step(&plan.id, &PlanStepId::from_static("missing")),
            Err(PlanExecutionError::StepNotFound { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan.id), plan);
    }

    #[test]
    fn complete_step_stores_output() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        let plan = start(&coordinator, &plan, "a");
        let output = json!({"ok": true});
        let updated = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), output.clone())
            .unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("a")).unwrap().output,
            Some(output)
        );
        assert_eq!(persisted(&coordinator, &plan.id), updated);
    }

    #[test]
    fn complete_step_unlocks_linear_dependency() {
        let (coordinator, plan) = executing_plan(vec![
            step("a"),
            step("b").depends_on(PlanStepId::from_static("a")),
        ]);
        let plan = start(&coordinator, &plan, "a");
        let updated = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("b")).unwrap().status,
            PlanStepStatus::Ready
        );
    }

    #[test]
    fn complete_step_does_not_unlock_partially_satisfied_dependency() {
        let c = step("c")
            .depends_on(PlanStepId::from_static("a"))
            .depends_on(PlanStepId::from_static("b"));
        let (coordinator, plan) = executing_plan(vec![step("a"), step("b"), c]);
        let plan = start(&coordinator, &plan, "a");
        let updated = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("c")).unwrap().status,
            PlanStepStatus::Pending
        );
    }

    #[test]
    fn complete_step_unlocks_branching_dependency_after_all_prerequisites_complete() {
        let c = step("c")
            .depends_on(PlanStepId::from_static("a"))
            .depends_on(PlanStepId::from_static("b"));
        let (coordinator, plan) = executing_plan(vec![step("a"), step("b"), c]);
        let plan = start(&coordinator, &plan, "a");
        let plan = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        let plan = start(&coordinator, &plan, "b");
        let updated = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("b"), json!(2))
            .unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("c")).unwrap().status,
            PlanStepStatus::Ready
        );
    }

    #[test]
    fn completing_final_step_completes_plan() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        let plan = start(&coordinator, &plan, "a");
        let updated = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        assert_eq!(updated.status, PlanStatus::Completed);
    }

    #[test]
    fn rejects_completion_when_step_is_not_running_without_mutation() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        assert!(matches!(
            coordinator.complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1)),
            Err(PlanExecutionError::StepNotRunning { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan.id), plan);
    }

    #[test]
    fn rejected_completion_does_not_store_output() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        let _ =
            coordinator.complete_step(&plan.id, &PlanStepId::from_static("a"), json!("rejected"));
        assert!(persisted(&coordinator, &plan.id)
            .step(&PlanStepId::from_static("a"))
            .unwrap()
            .output
            .is_none());
    }

    fn failure_fixture() -> (PlanExecutionCoordinator<InMemoryPlanRepository>, Plan) {
        let dependent = step("pending").depends_on(PlanStepId::from_static("failed"));
        let (coordinator, plan) = executing_plan(vec![
            step("failed"),
            step("ready"),
            step("running"),
            dependent,
            step("completed"),
        ]);
        let plan = start(&coordinator, &plan, "completed");
        let plan = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("completed"), json!(1))
            .unwrap();
        let plan = start(&coordinator, &plan, "failed");
        let plan = start(&coordinator, &plan, "running");
        (coordinator, plan)
    }

    #[test]
    fn fail_step_marks_selected_step_and_plan_failed_and_cancels_remaining_steps() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(updated.status, PlanStatus::Failed);
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("failed"))
                .unwrap()
                .status,
            PlanStepStatus::Failed
        );
        for id in ["ready", "running", "pending"] {
            assert_eq!(
                updated.step(&PlanStepId::from_static(id)).unwrap().status,
                PlanStepStatus::Cancelled
            );
        }
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("completed"))
                .unwrap()
                .status,
            PlanStepStatus::Completed
        );
        assert_eq!(persisted(&coordinator, &plan.id), updated);
    }

    #[test]
    fn fail_step_marks_selected_step_failed() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("failed"))
                .unwrap()
                .status,
            PlanStepStatus::Failed
        );
    }

    #[test]
    fn fail_step_marks_plan_failed() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(updated.status, PlanStatus::Failed);
    }

    #[test]
    fn fail_step_cancels_remaining_pending_steps() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("pending"))
                .unwrap()
                .status,
            PlanStepStatus::Cancelled
        );
    }

    #[test]
    fn fail_step_cancels_remaining_ready_steps() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("ready"))
                .unwrap()
                .status,
            PlanStepStatus::Cancelled
        );
    }

    #[test]
    fn fail_step_cancels_other_running_steps() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("running"))
                .unwrap()
                .status,
            PlanStepStatus::Cancelled
        );
    }

    #[test]
    fn fail_step_preserves_completed_steps() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_eq!(
            updated
                .step(&PlanStepId::from_static("completed"))
                .unwrap()
                .status,
            PlanStepStatus::Completed
        );
    }

    #[test]
    fn fail_step_does_not_cancel_failed_step() {
        let (coordinator, plan) = failure_fixture();
        let updated = coordinator
            .fail_step(&plan.id, &PlanStepId::from_static("failed"))
            .unwrap();
        assert_ne!(
            updated
                .step(&PlanStepId::from_static("failed"))
                .unwrap()
                .status,
            PlanStepStatus::Cancelled
        );
    }

    #[test]
    fn rejects_failure_when_step_is_not_running_without_mutation() {
        let (coordinator, plan) = executing_plan(vec![step("a")]);
        assert!(matches!(
            coordinator.fail_step(&plan.id, &PlanStepId::from_static("a")),
            Err(PlanExecutionError::StepNotRunning { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan.id), plan);
    }

    #[test]
    fn cancel_ready_plan_cancels_pending_steps() {
        let plan = ready_plan(vec![step("a")]);
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let updated = coordinator.cancel_plan(&plan_id).unwrap();
        assert_eq!(updated.status, PlanStatus::Cancelled);
        assert_eq!(updated.steps[0].status, PlanStepStatus::Cancelled);
    }

    #[test]
    fn cancel_executing_plan_cancels_non_terminal_steps_and_preserves_completed() {
        let (coordinator, plan) = executing_plan(vec![step("a"), step("b")]);
        let plan = start(&coordinator, &plan, "a");
        let plan = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        let updated = coordinator.cancel_plan(&plan.id).unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("a")).unwrap().status,
            PlanStepStatus::Completed
        );
        assert_eq!(
            updated.step(&PlanStepId::from_static("b")).unwrap().status,
            PlanStepStatus::Cancelled
        );
        assert_eq!(persisted(&coordinator, &plan.id), updated);
    }

    #[test]
    fn cancel_executing_plan_cancels_non_terminal_steps() {
        let (coordinator, plan) = executing_plan(vec![step("a"), step("b")]);
        let plan = start(&coordinator, &plan, "a");
        let updated = coordinator.cancel_plan(&plan.id).unwrap();
        assert!(updated
            .steps
            .iter()
            .all(|step| step.status == PlanStepStatus::Cancelled));
    }

    #[test]
    fn cancel_plan_preserves_completed_steps() {
        let (coordinator, plan) = executing_plan(vec![step("a"), step("b")]);
        let plan = start(&coordinator, &plan, "a");
        let plan = coordinator
            .complete_step(&plan.id, &PlanStepId::from_static("a"), json!(1))
            .unwrap();
        let updated = coordinator.cancel_plan(&plan.id).unwrap();
        assert_eq!(
            updated.step(&PlanStepId::from_static("a")).unwrap().status,
            PlanStepStatus::Completed
        );
    }

    #[test]
    fn cancel_plan_preserves_other_terminal_steps() {
        let mut plan = ready_plan(vec![step("failed"), step("skipped"), step("cancelled")]);
        plan.steps[0].transition_to(PlanStepStatus::Ready).unwrap();
        plan.steps[0]
            .transition_to(PlanStepStatus::Running)
            .unwrap();
        plan.steps[0].transition_to(PlanStepStatus::Failed).unwrap();
        plan.steps[1].status = PlanStepStatus::Skipped;
        plan.steps[2]
            .transition_to(PlanStepStatus::Cancelled)
            .unwrap();
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        let updated = coordinator.cancel_plan(&plan_id).unwrap();
        assert_eq!(
            updated
                .steps
                .iter()
                .map(|step| step.status)
                .collect::<Vec<_>>(),
            vec![
                PlanStepStatus::Failed,
                PlanStepStatus::Skipped,
                PlanStepStatus::Cancelled
            ]
        );
    }

    #[test]
    fn rejects_cancellation_for_disallowed_plan_states_without_mutation() {
        for status in [
            PlanStatus::Draft,
            PlanStatus::Validated,
            PlanStatus::Completed,
            PlanStatus::Failed,
            PlanStatus::Cancelled,
        ] {
            let mut plan = ready_plan(vec![step("a")]);
            match status {
                PlanStatus::Draft => plan = Plan::new(TaskId::new(), 1, "draft").unwrap(),
                PlanStatus::Validated => {
                    plan = Plan::new(TaskId::new(), 1, "validated").unwrap();
                    plan.add_step(step("a")).unwrap();
                    plan.transition_to(PlanStatus::Validated).unwrap();
                }
                PlanStatus::Completed => {
                    plan.transition_to(PlanStatus::Executing).unwrap();
                    plan.steps[0].transition_to(PlanStepStatus::Ready).unwrap();
                    plan.steps[0]
                        .transition_to(PlanStepStatus::Running)
                        .unwrap();
                    plan.steps[0]
                        .transition_to(PlanStepStatus::Completed)
                        .unwrap();
                    plan.transition_to(PlanStatus::Completed).unwrap();
                }
                PlanStatus::Failed => {
                    plan.transition_to(PlanStatus::Executing).unwrap();
                    plan.transition_to(PlanStatus::Failed).unwrap();
                }
                PlanStatus::Cancelled => plan.transition_to(PlanStatus::Cancelled).unwrap(),
                _ => unreachable!(),
            }
            let original = plan.clone();
            let plan_id = plan.id.clone();
            let coordinator = coordinator_with(plan);
            assert!(matches!(
                coordinator.cancel_plan(&plan_id),
                Err(PlanExecutionError::PlanCannotBeCancelled { .. })
            ));
            assert_eq!(persisted(&coordinator, &plan_id), original);
        }
    }

    fn assert_rejected_cancellation(status: PlanStatus) {
        let mut plan = ready_plan(vec![step("a")]);
        match status {
            PlanStatus::Draft => plan = Plan::new(TaskId::new(), 1, "draft").unwrap(),
            PlanStatus::Validated => {
                plan = Plan::new(TaskId::new(), 1, "validated").unwrap();
                plan.add_step(step("a")).unwrap();
                plan.transition_to(PlanStatus::Validated).unwrap();
            }
            PlanStatus::Completed => {
                plan.transition_to(PlanStatus::Executing).unwrap();
                plan.steps[0].transition_to(PlanStepStatus::Ready).unwrap();
                plan.steps[0]
                    .transition_to(PlanStepStatus::Running)
                    .unwrap();
                plan.steps[0]
                    .transition_to(PlanStepStatus::Completed)
                    .unwrap();
                plan.transition_to(PlanStatus::Completed).unwrap();
            }
            PlanStatus::Failed => {
                plan.transition_to(PlanStatus::Executing).unwrap();
                plan.transition_to(PlanStatus::Failed).unwrap();
            }
            PlanStatus::Cancelled => plan.transition_to(PlanStatus::Cancelled).unwrap(),
            _ => unreachable!(),
        }
        let original = plan.clone();
        let plan_id = plan.id.clone();
        let coordinator = coordinator_with(plan);
        assert!(matches!(
            coordinator.cancel_plan(&plan_id),
            Err(PlanExecutionError::PlanCannotBeCancelled { .. })
        ));
        assert_eq!(persisted(&coordinator, &plan_id), original);
    }

    #[test]
    fn rejects_cancellation_for_draft_plan_without_mutation() {
        assert_rejected_cancellation(PlanStatus::Draft);
    }

    #[test]
    fn rejects_cancellation_for_validated_plan_without_mutation() {
        assert_rejected_cancellation(PlanStatus::Validated);
    }

    #[test]
    fn rejects_cancellation_for_completed_plan_without_mutation() {
        assert_rejected_cancellation(PlanStatus::Completed);
    }

    #[test]
    fn rejects_cancellation_for_failed_plan_without_mutation() {
        assert_rejected_cancellation(PlanStatus::Failed);
    }

    #[test]
    fn rejects_cancellation_for_cancelled_plan_without_mutation() {
        assert_rejected_cancellation(PlanStatus::Cancelled);
    }
}
