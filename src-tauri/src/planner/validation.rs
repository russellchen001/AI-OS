use super::domain::{Plan, PlanStepId};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

pub fn validate_plan(plan: &Plan) -> Result<(), PlanValidationError> {
    if plan.steps.is_empty() {
        return Err(PlanValidationError::EmptyPlan);
    }

    let step_ids = plan
        .steps
        .iter()
        .map(|step| step.id.clone())
        .collect::<HashSet<_>>();

    if step_ids.len() != plan.steps.len() {
        return Err(PlanValidationError::DuplicateStepId);
    }

    for step in &plan.steps {
        let mut dependencies = HashSet::new();

        for dependency in &step.dependencies {
            if dependency == &step.id {
                return Err(PlanValidationError::SelfDependency {
                    step_id: step.id.clone(),
                });
            }

            if !step_ids.contains(dependency) {
                return Err(PlanValidationError::UnknownDependency {
                    step_id: step.id.clone(),
                    dependency_id: dependency.clone(),
                });
            }

            if !dependencies.insert(dependency.clone()) {
                return Err(PlanValidationError::DuplicateDependency {
                    step_id: step.id.clone(),
                    dependency_id: dependency.clone(),
                });
            }
        }
    }

    detect_cycle(plan)?;

    Ok(())
}

fn detect_cycle(plan: &Plan) -> Result<(), PlanValidationError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    fn visit(
        step_id: &PlanStepId,
        graph: &HashMap<PlanStepId, Vec<PlanStepId>>,
        states: &mut HashMap<PlanStepId, VisitState>,
    ) -> Result<(), PlanValidationError> {
        match states.get(step_id) {
            Some(VisitState::Visiting) => {
                return Err(PlanValidationError::CyclicDependency {
                    step_id: step_id.clone(),
                });
            }

            Some(VisitState::Visited) => {
                return Ok(());
            }

            None => {}
        }

        states.insert(step_id.clone(), VisitState::Visiting);

        if let Some(dependencies) = graph.get(step_id) {
            for dependency in dependencies {
                visit(dependency, graph, states)?;
            }
        }

        states.insert(step_id.clone(), VisitState::Visited);

        Ok(())
    }

    let graph = plan
        .steps
        .iter()
        .map(|step| (step.id.clone(), step.dependencies.clone()))
        .collect::<HashMap<_, _>>();

    let mut states = HashMap::new();

    for step in &plan.steps {
        visit(&step.id, &graph, &mut states)?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanValidationError {
    EmptyPlan,
    DuplicateStepId,

    SelfDependency {
        step_id: PlanStepId,
    },

    UnknownDependency {
        step_id: PlanStepId,
        dependency_id: PlanStepId,
    },

    DuplicateDependency {
        step_id: PlanStepId,
        dependency_id: PlanStepId,
    },

    CyclicDependency {
        step_id: PlanStepId,
    },
}

impl fmt::Display for PlanValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPlan => formatter.write_str("plan must contain at least one step"),

            Self::DuplicateStepId => formatter.write_str("plan contains duplicate step ids"),

            Self::SelfDependency { step_id } => {
                write!(formatter, "step {step_id} cannot depend on itself")
            }

            Self::UnknownDependency {
                step_id,
                dependency_id,
            } => {
                write!(
                    formatter,
                    "step {step_id} depends on unknown step \
                     {dependency_id}"
                )
            }

            Self::DuplicateDependency {
                step_id,
                dependency_id,
            } => {
                write!(
                    formatter,
                    "step {step_id} repeats dependency \
                     {dependency_id}"
                )
            }

            Self::CyclicDependency { step_id } => {
                write!(
                    formatter,
                    "plan contains a dependency cycle near \
                     step {step_id}"
                )
            }
        }
    }
}

impl Error for PlanValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{Plan, PlanStep, PlanStepId};

    fn step(id: &str) -> PlanStep {
        PlanStep::new(id, format!("test.{id}"))
            .unwrap()
            .with_id(PlanStepId::from_static(id))
    }

    #[test]
    fn accepts_valid_linear_plan() {
        let scan = step("scan");
        let classify = step("classify").depends_on(scan.id.clone());
        let move_files = step("move").depends_on(classify.id.clone());

        let mut plan = Plan::new("organize files").unwrap();

        plan.add_step(scan).unwrap();
        plan.add_step(classify).unwrap();
        plan.add_step(move_files).unwrap();

        assert_eq!(validate_plan(&plan), Ok(()));
    }

    #[test]
    fn accepts_valid_branching_plan() {
        let source = step("source");

        let left = step("left").depends_on(source.id.clone());

        let right = step("right").depends_on(source.id.clone());

        let finish = step("finish")
            .depends_on(left.id.clone())
            .depends_on(right.id.clone());

        let mut plan = Plan::new("branching workflow").unwrap();

        plan.add_step(source).unwrap();
        plan.add_step(left).unwrap();
        plan.add_step(right).unwrap();
        plan.add_step(finish).unwrap();

        assert_eq!(validate_plan(&plan), Ok(()));
    }

    #[test]
    fn rejects_empty_plan() {
        let plan = Plan::new("empty workflow").unwrap();

        assert_eq!(validate_plan(&plan), Err(PlanValidationError::EmptyPlan));
    }

    #[test]
    fn rejects_unknown_dependency() {
        let missing = PlanStepId::from_static("missing");

        let dependent = step("dependent").depends_on(missing.clone());

        let mut plan = Plan::new("invalid workflow").unwrap();

        plan.add_step(dependent).unwrap();

        assert_eq!(
            validate_plan(&plan),
            Err(PlanValidationError::UnknownDependency {
                step_id: PlanStepId::from_static("dependent"),
                dependency_id: missing,
            })
        );
    }

    #[test]
    fn rejects_self_dependency() {
        let id = PlanStepId::from_static("self");

        let dependent = step("self").depends_on(id.clone());

        let mut plan = Plan::new("invalid workflow").unwrap();

        plan.add_step(dependent).unwrap();

        assert_eq!(
            validate_plan(&plan),
            Err(PlanValidationError::SelfDependency { step_id: id })
        );
    }

    #[test]
    fn rejects_duplicate_dependency() {
        let source = step("source");
        let source_id = source.id.clone();

        let dependent = PlanStep {
            dependencies: vec![source_id.clone(), source_id.clone()],
            ..step("dependent")
        };

        let mut plan = Plan::new("invalid workflow").unwrap();

        plan.add_step(source).unwrap();
        plan.add_step(dependent).unwrap();

        assert_eq!(
            validate_plan(&plan),
            Err(PlanValidationError::DuplicateDependency {
                step_id: PlanStepId::from_static("dependent"),
                dependency_id: source_id,
            })
        );
    }

    #[test]
    fn rejects_direct_cycle() {
        let first_id = PlanStepId::from_static("first");
        let second_id = PlanStepId::from_static("second");

        let first = step("first").depends_on(second_id.clone());

        let second = step("second").depends_on(first_id);

        let mut plan = Plan::new("cyclic workflow").unwrap();

        plan.add_step(first).unwrap();
        plan.add_step(second).unwrap();

        assert!(matches!(
            validate_plan(&plan),
            Err(PlanValidationError::CyclicDependency { .. })
        ));
    }

    #[test]
    fn rejects_indirect_cycle() {
        let first = step("first").depends_on(PlanStepId::from_static("third"));

        let second = step("second").depends_on(PlanStepId::from_static("first"));

        let third = step("third").depends_on(PlanStepId::from_static("second"));

        let mut plan = Plan::new("cyclic workflow").unwrap();

        plan.add_step(first).unwrap();
        plan.add_step(second).unwrap();
        plan.add_step(third).unwrap();

        assert!(matches!(
            validate_plan(&plan),
            Err(PlanValidationError::CyclicDependency { .. })
        ));
    }
}
