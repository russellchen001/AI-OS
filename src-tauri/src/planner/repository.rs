use super::domain::{Plan, PlanId};
use std::{collections::HashMap, error::Error, fmt, sync::RwLock};

pub trait PlanRepository: Send + Sync {
    fn create(&self, plan: Plan) -> Result<PlanId, PlanRepositoryError>;

    fn get(&self, plan_id: &PlanId) -> Result<Option<Plan>, PlanRepositoryError>;

    fn list(&self) -> Result<Vec<Plan>, PlanRepositoryError>;

    fn update(&self, plan: Plan) -> Result<(), PlanRepositoryError>;

    fn delete(&self, plan_id: &PlanId) -> Result<Plan, PlanRepositoryError>;
}

#[derive(Debug, Default)]
pub struct InMemoryPlanRepository {
    plans: RwLock<HashMap<PlanId, Plan>>,
}

impl InMemoryPlanRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> Result<usize, PlanRepositoryError> {
        let plans = self
            .plans
            .read()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        Ok(plans.len())
    }

    pub fn is_empty(&self) -> Result<bool, PlanRepositoryError> {
        self.len().map(|length| length == 0)
    }
}

impl PlanRepository for InMemoryPlanRepository {
    fn create(&self, plan: Plan) -> Result<PlanId, PlanRepositoryError> {
        let mut plans = self
            .plans
            .write()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        if plans.contains_key(&plan.id) {
            return Err(PlanRepositoryError::AlreadyExists(plan.id.clone()));
        }

        let plan_id = plan.id.clone();

        plans.insert(plan_id.clone(), plan);

        Ok(plan_id)
    }

    fn get(&self, plan_id: &PlanId) -> Result<Option<Plan>, PlanRepositoryError> {
        let plans = self
            .plans
            .read()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        Ok(plans.get(plan_id).cloned())
    }

    fn list(&self) -> Result<Vec<Plan>, PlanRepositoryError> {
        let plans = self
            .plans
            .read()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        let mut result: Vec<Plan> = plans.values().cloned().collect();

        result.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });

        Ok(result)
    }

    fn update(&self, plan: Plan) -> Result<(), PlanRepositoryError> {
        let mut plans = self
            .plans
            .write()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        if !plans.contains_key(&plan.id) {
            return Err(PlanRepositoryError::NotFound(plan.id.clone()));
        }

        plans.insert(plan.id.clone(), plan);

        Ok(())
    }

    fn delete(&self, plan_id: &PlanId) -> Result<Plan, PlanRepositoryError> {
        let mut plans = self
            .plans
            .write()
            .map_err(|_| PlanRepositoryError::LockPoisoned)?;

        plans
            .remove(plan_id)
            .ok_or_else(|| PlanRepositoryError::NotFound(plan_id.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRepositoryError {
    AlreadyExists(PlanId),
    NotFound(PlanId),
    LockPoisoned,
}

impl fmt::Display for PlanRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists(plan_id) => {
                write!(formatter, "plan {plan_id} already exists")
            }

            Self::NotFound(plan_id) => {
                write!(formatter, "plan {plan_id} was not found")
            }

            Self::LockPoisoned => formatter.write_str("plan repository lock was poisoned"),
        }
    }
}

impl Error for PlanRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{PlanStatus, PlanStep};

    fn plan(objective: &str) -> Plan {
        let mut plan = Plan::new(crate::task_engine::TaskId::new(), 1, objective).unwrap();

        plan.add_step(
            PlanStep::new(
                format!("{objective} step"),
                format!("test.{}", objective.replace(' ', "_")),
            )
            .unwrap(),
        )
        .unwrap();

        plan
    }

    #[test]
    fn creates_and_retrieves_plan() {
        let repository = InMemoryPlanRepository::new();
        let plan = plan("organize files");
        let plan_id = plan.id.clone();

        assert_eq!(repository.create(plan.clone()).unwrap(), plan_id);

        assert_eq!(repository.get(&plan_id).unwrap(), Some(plan));
        assert_eq!(repository.len().unwrap(), 1);
    }

    #[test]
    fn rejects_duplicate_plan_id() {
        let repository = InMemoryPlanRepository::new();
        let plan = plan("organize files");

        repository.create(plan.clone()).unwrap();

        assert_eq!(
            repository.create(plan.clone()),
            Err(PlanRepositoryError::AlreadyExists(plan.id))
        );
    }

    #[test]
    fn updates_existing_plan() {
        let repository = InMemoryPlanRepository::new();
        let mut plan = plan("organize files");
        let plan_id = plan.id.clone();

        repository.create(plan.clone()).unwrap();

        plan.status = PlanStatus::Validated;

        repository.update(plan.clone()).unwrap();

        assert_eq!(repository.get(&plan_id).unwrap(), Some(plan));
    }

    #[test]
    fn rejects_update_for_unknown_plan() {
        let repository = InMemoryPlanRepository::new();
        let plan = plan("missing plan");

        assert_eq!(
            repository.update(plan.clone()),
            Err(PlanRepositoryError::NotFound(plan.id))
        );
    }

    #[test]
    fn lists_plans_in_deterministic_order() {
        let repository = InMemoryPlanRepository::new();

        let first = plan("first");
        let second = plan("second");

        repository.create(second.clone()).unwrap();
        repository.create(first.clone()).unwrap();

        let plans = repository.list().unwrap();

        let mut expected = vec![first, second];

        expected.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });

        assert_eq!(plans, expected);
    }

    #[test]
    fn deletes_existing_plan() {
        let repository = InMemoryPlanRepository::new();
        let plan = plan("delete me");
        let plan_id = plan.id.clone();

        repository.create(plan.clone()).unwrap();

        assert_eq!(repository.delete(&plan_id).unwrap(), plan);
        assert_eq!(repository.get(&plan_id).unwrap(), None);
        assert!(repository.is_empty().unwrap());
    }

    #[test]
    fn rejects_delete_for_unknown_plan() {
        let repository = InMemoryPlanRepository::new();
        let plan_id = plan("missing plan").id;

        assert_eq!(
            repository.delete(&plan_id),
            Err(PlanRepositoryError::NotFound(plan_id))
        );
    }
}
