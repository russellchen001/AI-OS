use super::{
    validate_plan as validate_plan_structure, Plan, PlanDomainError, PlanId, PlanRepository,
    PlanRepositoryError, PlanStatus, PlanValidationError,
};
use crate::task_engine::{Task, TaskId, TaskRepository, TaskRepositoryError, TaskStatus};
use std::{error::Error, fmt};

pub struct PlannerService<T, P> {
    tasks: T,
    plans: P,
}

impl<T, P> PlannerService<T, P>
where
    T: TaskRepository,
    P: PlanRepository,
{
    pub fn new(tasks: T, plans: P) -> Self {
        Self { tasks, plans }
    }

    pub fn task_repository(&self) -> &T {
        &self.tasks
    }

    pub fn plan_repository(&self) -> &P {
        &self.plans
    }

    pub fn create_plan(
        &self,
        task_id: &TaskId,
        objective: impl Into<String>,
    ) -> Result<Plan, PlannerServiceError> {
        self.tasks
            .get(task_id)?
            .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))?;

        let revision = self
            .plans
            .list_by_task(task_id)?
            .iter()
            .map(|plan| plan.revision)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| PlannerServiceError::RevisionOverflow(task_id.clone()))?;

        let plan = Plan::new(task_id.clone(), revision, objective)?;

        self.plans.create(plan.clone())?;

        Ok(plan)
    }

    pub fn validate_plan(&self, plan_id: &PlanId) -> Result<Plan, PlannerServiceError> {
        let mut plan = self
            .plans
            .get(plan_id)?
            .ok_or_else(|| PlanRepositoryError::NotFound(plan_id.clone()))?;

        if plan.status != PlanStatus::Draft {
            return Err(PlannerServiceError::PlanNotDraft {
                plan_id: plan.id,
                status: plan.status,
            });
        }

        validate_plan_structure(&plan)?;

        plan.transition_to(PlanStatus::Validated)?;

        self.plans.update(plan.clone())?;

        Ok(plan)
    }

    pub fn activate_plan(
        &self,
        task_id: &TaskId,
        plan_id: &PlanId,
    ) -> Result<Task, PlannerServiceError> {
        let mut task = self
            .tasks
            .get(task_id)?
            .ok_or_else(|| TaskRepositoryError::NotFound(task_id.clone()))?;

        let mut plan = self
            .plans
            .get(plan_id)?
            .ok_or_else(|| PlanRepositoryError::NotFound(plan_id.clone()))?;

        if plan.task_id != task.id {
            return Err(PlannerServiceError::PlanTaskMismatch {
                task_id: task.id,
                plan_id: plan.id,
                plan_task_id: plan.task_id,
            });
        }

        if task.status != TaskStatus::Planning {
            return Err(PlannerServiceError::TaskNotPlanning {
                task_id: task.id,
                status: task.status,
            });
        }

        if let Some(active_plan_id) = task.active_plan_id.clone() {
            return Err(PlannerServiceError::TaskAlreadyHasActivePlan {
                task_id: task.id,
                active_plan_id,
            });
        }

        if plan.status != PlanStatus::Validated {
            return Err(PlannerServiceError::PlanNotValidated {
                plan_id: plan.id,
                status: plan.status,
            });
        }

        plan.transition_to(PlanStatus::Ready)?;
        self.plans.update(plan.clone())?;

        task.activate_plan(plan.id);
        self.tasks.update(task.clone())?;

        Ok(task)
    }
}

#[derive(Debug)]
pub enum PlannerServiceError {
    TaskRepository(TaskRepositoryError),
    PlanRepository(PlanRepositoryError),
    Domain(PlanDomainError),
    Validation(PlanValidationError),

    PlanTaskMismatch {
        task_id: TaskId,
        plan_id: PlanId,
        plan_task_id: TaskId,
    },

    PlanNotDraft {
        plan_id: PlanId,
        status: PlanStatus,
    },

    PlanNotValidated {
        plan_id: PlanId,
        status: PlanStatus,
    },

    TaskNotPlanning {
        task_id: TaskId,
        status: TaskStatus,
    },

    TaskAlreadyHasActivePlan {
        task_id: TaskId,
        active_plan_id: PlanId,
    },

    RevisionOverflow(TaskId),
}

impl fmt::Display for PlannerServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskRepository(error) => {
                write!(formatter, "task repository error: {error}")
            }

            Self::PlanRepository(error) => {
                write!(formatter, "plan repository error: {error}")
            }

            Self::Domain(error) => {
                write!(formatter, "plan domain error: {error}")
            }

            Self::Validation(error) => {
                write!(formatter, "plan validation error: {error}")
            }

            Self::PlanTaskMismatch {
                task_id,
                plan_id,
                plan_task_id,
            } => {
                write!(
                    formatter,
                    "plan {plan_id} belongs to task {plan_task_id}, not task {task_id}"
                )
            }

            Self::PlanNotDraft { plan_id, status } => {
                write!(
                    formatter,
                    "plan {plan_id} has status {status:?} and cannot be validated"
                )
            }

            Self::PlanNotValidated { plan_id, status } => {
                write!(
                    formatter,
                    "plan {plan_id} has status {status:?} and cannot be activated"
                )
            }

            Self::TaskNotPlanning { task_id, status } => {
                write!(
                    formatter,
                    "task {task_id} has status {status:?} and cannot activate a plan"
                )
            }

            Self::TaskAlreadyHasActivePlan {
                task_id,
                active_plan_id,
            } => {
                write!(
                    formatter,
                    "task {task_id} already has active plan {active_plan_id}"
                )
            }

            Self::RevisionOverflow(task_id) => {
                write!(formatter, "plan revision overflow for task {task_id}")
            }
        }
    }
}

impl Error for PlannerServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TaskRepository(error) => Some(error),
            Self::PlanRepository(error) => Some(error),
            Self::Domain(error) => Some(error),
            Self::Validation(error) => Some(error),

            Self::PlanTaskMismatch { .. }
            | Self::PlanNotDraft { .. }
            | Self::PlanNotValidated { .. }
            | Self::TaskNotPlanning { .. }
            | Self::TaskAlreadyHasActivePlan { .. }
            | Self::RevisionOverflow(_) => None,
        }
    }
}

impl From<TaskRepositoryError> for PlannerServiceError {
    fn from(error: TaskRepositoryError) -> Self {
        Self::TaskRepository(error)
    }
}

impl From<PlanRepositoryError> for PlannerServiceError {
    fn from(error: PlanRepositoryError) -> Self {
        Self::PlanRepository(error)
    }
}

impl From<PlanDomainError> for PlannerServiceError {
    fn from(error: PlanDomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<PlanValidationError> for PlannerServiceError {
    fn from(error: PlanValidationError) -> Self {
        Self::Validation(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        planner::{InMemoryPlanRepository, PlanStep},
        task_engine::{InMemoryTaskRepository, Task, TaskStatus, TaskType},
    };

    fn service() -> PlannerService<InMemoryTaskRepository, InMemoryPlanRepository> {
        PlannerService::new(InMemoryTaskRepository::new(), InMemoryPlanRepository::new())
    }

    fn create_task(
        service: &PlannerService<InMemoryTaskRepository, InMemoryPlanRepository>,
        intent: &str,
    ) -> Task {
        let task = Task::new(TaskType::Do, intent).unwrap();

        service.task_repository().create(task.clone()).unwrap();

        task
    }

    fn add_valid_step(
        service: &PlannerService<InMemoryTaskRepository, InMemoryPlanRepository>,
        mut plan: Plan,
    ) -> Plan {
        let step = PlanStep::new("execute task", "test.execute").unwrap();

        plan.add_step(step).unwrap();

        service.plan_repository().update(plan.clone()).unwrap();

        plan
    }

    fn move_task_to_planning(
        service: &PlannerService<InMemoryTaskRepository, InMemoryPlanRepository>,
        task: &Task,
    ) -> Task {
        let mut task = task.clone();

        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();

        service.task_repository().update(task.clone()).unwrap();

        task
    }

    #[test]
    fn creates_first_plan_revision_for_existing_task() {
        let service = service();
        let task = create_task(&service, "organize files");

        let plan = service
            .create_plan(&task.id, "scan and organize files")
            .unwrap();

        assert_eq!(plan.task_id, task.id);
        assert_eq!(plan.revision, 1);

        assert_eq!(service.plan_repository().get(&plan.id).unwrap(), Some(plan));
    }

    #[test]
    fn increments_revision_for_each_task_independently() {
        let service = service();
        let first_task = create_task(&service, "organize files");
        let second_task = create_task(&service, "send email");

        let first_revision = service.create_plan(&first_task.id, "first plan").unwrap();

        let second_revision = service.create_plan(&first_task.id, "revised plan").unwrap();

        let unrelated = service.create_plan(&second_task.id, "email plan").unwrap();

        assert_eq!(first_revision.revision, 1);
        assert_eq!(second_revision.revision, 2);
        assert_eq!(unrelated.revision, 1);
    }

    #[test]
    fn rejects_plan_creation_for_unknown_task() {
        let service = service();
        let missing_task_id = TaskId::new();

        assert!(matches!(
            service.create_plan(&missing_task_id, "missing task plan"),
            Err(PlannerServiceError::TaskRepository(
                TaskRepositoryError::NotFound(task_id)
            )) if task_id == missing_task_id
        ));

        assert!(service.plan_repository().is_empty().unwrap());
    }

    #[test]
    fn validates_plan_and_persists_validated_status() {
        let service = service();
        let task = create_task(&service, "organize files");

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        let plan = add_valid_step(&service, plan);

        let validated = service.validate_plan(&plan.id).unwrap();

        assert_eq!(validated.status, PlanStatus::Validated);

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Validated
        );

        assert!(service
            .task_repository()
            .get(&task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());
    }

    #[test]
    fn rejects_validation_for_unknown_plan() {
        let service = service();
        let missing_plan_id = PlanId::new();

        assert!(matches!(
            service.validate_plan(&missing_plan_id),
            Err(PlannerServiceError::PlanRepository(
                PlanRepositoryError::NotFound(plan_id)
            )) if plan_id == missing_plan_id
        ));
    }

    #[test]
    fn rejects_validation_for_non_draft_plan() {
        let service = service();
        let task = create_task(&service, "organize files");

        let mut plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        plan.status = PlanStatus::Ready;

        service.plan_repository().update(plan.clone()).unwrap();

        assert!(matches!(
            service.validate_plan(&plan.id),
            Err(PlannerServiceError::PlanNotDraft {
                plan_id,
                status: PlanStatus::Ready,
            }) if plan_id == plan.id
        ));

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Ready
        );
    }

    #[test]
    fn failed_validation_preserves_draft_status() {
        let service = service();
        let task = create_task(&service, "organize files");

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        assert!(matches!(
            service.validate_plan(&plan.id),
            Err(PlannerServiceError::Validation(
                PlanValidationError::EmptyPlan
            ))
        ));

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Draft
        );
    }

    #[test]
    fn rejects_activation_for_draft_plan() {
        let service = service();
        let task = create_task(&service, "organize files");
        let task = move_task_to_planning(&service, &task);

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        assert!(matches!(
            service.activate_plan(&task.id, &plan.id),
            Err(PlannerServiceError::PlanNotValidated {
                plan_id,
                status: PlanStatus::Draft,
            }) if plan_id == plan.id
        ));

        assert!(service
            .task_repository()
            .get(&task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());
    }

    #[test]
    fn activates_validated_plan_and_marks_it_ready() {
        let service = service();
        let task = create_task(&service, "organize files");
        let task = move_task_to_planning(&service, &task);

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        let plan = add_valid_step(&service, plan);

        service.validate_plan(&plan.id).unwrap();

        let updated = service.activate_plan(&task.id, &plan.id).unwrap();

        assert_eq!(updated.active_plan_id, Some(plan.id.clone()));

        assert_eq!(
            service
                .task_repository()
                .get(&task.id)
                .unwrap()
                .unwrap()
                .active_plan_id,
            Some(plan.id.clone())
        );

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Ready
        );
    }

    #[test]
    fn rejects_activation_when_task_is_not_planning() {
        let service = service();
        let task = create_task(&service, "organize files");

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        let plan = add_valid_step(&service, plan);

        service.validate_plan(&plan.id).unwrap();

        assert!(matches!(
            service.activate_plan(&task.id, &plan.id),
            Err(PlannerServiceError::TaskNotPlanning {
                task_id,
                status: TaskStatus::Created,
            }) if task_id == task.id
        ));

        assert!(service
            .task_repository()
            .get(&task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Validated
        );
    }

    #[test]
    fn rejects_activation_when_task_already_has_active_plan() {
        let service = service();
        let task = create_task(&service, "organize files");
        let mut task = move_task_to_planning(&service, &task);
        let existing_plan_id = PlanId::new();

        task.activate_plan(existing_plan_id.clone());
        service.task_repository().update(task.clone()).unwrap();

        let plan = service.create_plan(&task.id, "replacement plan").unwrap();

        let plan = add_valid_step(&service, plan);

        service.validate_plan(&plan.id).unwrap();

        assert!(matches!(
            service.activate_plan(&task.id, &plan.id),
            Err(PlannerServiceError::TaskAlreadyHasActivePlan {
                task_id,
                active_plan_id,
            }) if task_id == task.id && active_plan_id == existing_plan_id
        ));

        assert_eq!(
            service
                .task_repository()
                .get(&task.id)
                .unwrap()
                .unwrap()
                .active_plan_id,
            Some(existing_plan_id)
        );

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Validated
        );
    }

    #[test]
    fn rejects_reactivation_of_ready_plan() {
        let service = service();
        let task = create_task(&service, "organize files");
        let task = move_task_to_planning(&service, &task);

        let plan = service
            .create_plan(&task.id, "organize files safely")
            .unwrap();

        let plan = add_valid_step(&service, plan);

        service.validate_plan(&plan.id).unwrap();
        service.activate_plan(&task.id, &plan.id).unwrap();

        let mut persisted_task = service.task_repository().get(&task.id).unwrap().unwrap();

        persisted_task.active_plan_id = None;
        service.task_repository().update(persisted_task).unwrap();

        assert!(matches!(
            service.activate_plan(&task.id, &plan.id),
            Err(PlannerServiceError::PlanNotValidated {
                plan_id,
                status: PlanStatus::Ready,
            }) if plan_id == plan.id
        ));

        assert!(service
            .task_repository()
            .get(&task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());

        assert_eq!(
            service
                .plan_repository()
                .get(&plan.id)
                .unwrap()
                .unwrap()
                .status,
            PlanStatus::Ready
        );
    }

    #[test]
    fn rejects_activation_for_another_tasks_plan() {
        let service = service();
        let first_task = create_task(&service, "first task");
        let second_task = create_task(&service, "second task");

        let plan = service
            .create_plan(&first_task.id, "first task plan")
            .unwrap();

        assert!(matches!(
            service.activate_plan(&second_task.id, &plan.id),
            Err(PlannerServiceError::PlanTaskMismatch {
                task_id,
                plan_id,
                plan_task_id,
            }) if task_id == second_task.id
                && plan_id == plan.id
                && plan_task_id == first_task.id
        ));

        assert!(service
            .task_repository()
            .get(&second_task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());
    }

    #[test]
    fn rejects_activation_for_unknown_plan() {
        let service = service();
        let task = create_task(&service, "organize files");
        let missing_plan_id = PlanId::new();

        assert!(matches!(
            service.activate_plan(&task.id, &missing_plan_id),
            Err(PlannerServiceError::PlanRepository(
                PlanRepositoryError::NotFound(plan_id)
            )) if plan_id == missing_plan_id
        ));

        assert!(service
            .task_repository()
            .get(&task.id)
            .unwrap()
            .unwrap()
            .active_plan_id
            .is_none());
    }
}
