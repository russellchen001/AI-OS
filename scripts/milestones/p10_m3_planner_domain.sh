#!/usr/bin/env bash
set -euo pipefail

EXPECTED_BRANCH="feature/p10-m3-planner-domain"
CARGO_TOML="src-tauri/Cargo.toml"
SRC_DIR="src-tauri/src"
PLANNER_DIR="$SRC_DIR/planner"
LIB_FILE="$SRC_DIR/lib.rs"

fail() {
  printf '\nERROR: %s\n' "$1" >&2
  exit 1
}

[[ -f "$CARGO_TOML" ]] ||
  fail "找不到 $CARGO_TOML，请在 AI-OS/dashboard 仓库根目录运行。"

[[ -f "$LIB_FILE" ]] ||
  fail "找不到 $LIB_FILE。"

command -v git >/dev/null 2>&1 ||
  fail "找不到 Git。"

command -v cargo >/dev/null 2>&1 ||
  fail "找不到 Cargo。"

command -v python3 >/dev/null 2>&1 ||
  fail "找不到 Python 3。"

CURRENT_BRANCH="$(git branch --show-current)"

[[ "$CURRENT_BRANCH" == "$EXPECTED_BRANCH" ]] ||
  fail "当前分支是 $CURRENT_BRANCH，应为 $EXPECTED_BRANCH。"

DIRTY_FILES="$(
  git status --porcelain |
    grep -v '^?? p10_m3_planner_domain\.sh$' || true
)"

if [[ -n "$DIRTY_FILES" ]]; then
  printf '\n当前存在其他未提交修改：\n%s\n' "$DIRTY_FILES"
  fail "请先清理工作区。"
fi

if [[ -e "$PLANNER_DIR" ]]; then
  fail "$PLANNER_DIR 已经存在，为避免覆盖脚本已停止。"
fi

mkdir -p "$PLANNER_DIR"

cat > "$PLANNER_DIR/domain.rs" <<'RUST'
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    process,
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

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Default,
)]
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
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled
        )
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Default,
)]
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
            Self::Completed
                | Self::Failed
                | Self::Skipped
                | Self::Cancelled
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

    pub fn with_description(
        mut self,
        description: impl Into<String>,
    ) -> Self {
        self.description = description.into();
        self
    }

    pub fn depends_on(mut self, dependency: PlanStepId) -> Self {
        if !self.dependencies.contains(&dependency) {
            self.dependencies.push(dependency);
        }

        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub objective: String,
    pub status: PlanStatus,
    pub steps: Vec<PlanStep>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl Plan {
    pub fn new(
        objective: impl Into<String>,
    ) -> Result<Self, PlanDomainError> {
        let objective = objective.into().trim().to_owned();

        if objective.is_empty() {
            return Err(PlanDomainError::EmptyObjective);
        }

        let timestamp = now_ms();

        Ok(Self {
            id: PlanId::new(),
            objective,
            status: PlanStatus::Draft,
            steps: Vec::new(),
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn add_step(
        &mut self,
        step: PlanStep,
    ) -> Result<(), PlanDomainError> {
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

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanDomainError {
    EmptyObjective,
    EmptyStepName,
    EmptyCapability,
    DuplicateStepId(PlanStepId),
}

impl fmt::Display for PlanDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyObjective => {
                formatter.write_str("plan objective must not be empty")
            }

            Self::EmptyStepName => {
                formatter.write_str("plan step name must not be empty")
            }

            Self::EmptyCapability => {
                formatter.write_str(
                    "plan step capability must not be empty",
                )
            }

            Self::DuplicateStepId(step_id) => {
                write!(
                    formatter,
                    "plan contains duplicate step id {step_id}"
                )
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
        let plan = Plan::new("organize files").unwrap();

        assert!(plan.id.as_str().starts_with("plan_"));
        assert_eq!(plan.objective, "organize files");
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(plan.steps.is_empty());
        assert!(plan.updated_at >= plan.created_at);
    }

    #[test]
    fn rejects_empty_plan_objective() {
        assert_eq!(
            Plan::new("   "),
            Err(PlanDomainError::EmptyObjective)
        );
    }

    #[test]
    fn creates_step_with_stable_defaults() {
        let step = PlanStep::new(
            "scan directory",
            "filesystem.scan",
        )
        .unwrap();

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

        let first = PlanStep::new(
            "scan directory",
            "filesystem.scan",
        )
        .unwrap()
        .with_id(step_id.clone());

        let second = PlanStep::new(
            "scan again",
            "filesystem.scan",
        )
        .unwrap()
        .with_id(step_id.clone());

        let mut plan = Plan::new("organize files").unwrap();

        plan.add_step(first).unwrap();

        assert_eq!(
            plan.add_step(second),
            Err(PlanDomainError::DuplicateStepId(step_id))
        );
    }

    #[test]
    fn serializes_without_data_loss() {
        let scan_id = PlanStepId::from_static("scan");

        let mut scan = PlanStep::new(
            "scan directory",
            "filesystem.scan",
        )
        .unwrap()
        .with_id(scan_id.clone());

        scan.input.insert(
            "path".to_owned(),
            json!("/Users/example"),
        );

        let move_step = PlanStep::new(
            "move files",
            "filesystem.move",
        )
        .unwrap()
        .depends_on(scan_id);

        let mut plan = Plan::new("organize files").unwrap();

        plan.add_step(scan).unwrap();
        plan.add_step(move_step).unwrap();

        let serialized = serde_json::to_string(&plan).unwrap();
        let restored: Plan =
            serde_json::from_str(&serialized).unwrap();

        assert_eq!(restored, plan);
    }
}
RUST

cat > "$PLANNER_DIR/validation.rs" <<'RUST'
use super::domain::{Plan, PlanStepId};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

pub fn validate_plan(
    plan: &Plan,
) -> Result<(), PlanValidationError> {
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
                return Err(
                    PlanValidationError::UnknownDependency {
                        step_id: step.id.clone(),
                        dependency_id: dependency.clone(),
                    },
                );
            }

            if !dependencies.insert(dependency.clone()) {
                return Err(
                    PlanValidationError::DuplicateDependency {
                        step_id: step.id.clone(),
                        dependency_id: dependency.clone(),
                    },
                );
            }
        }
    }

    detect_cycle(plan)?;

    Ok(())
}

fn detect_cycle(
    plan: &Plan,
) -> Result<(), PlanValidationError> {
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
                return Err(
                    PlanValidationError::CyclicDependency {
                        step_id: step_id.clone(),
                    },
                );
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
        .map(|step| {
            (step.id.clone(), step.dependencies.clone())
        })
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
            Self::EmptyPlan => {
                formatter.write_str(
                    "plan must contain at least one step",
                )
            }

            Self::DuplicateStepId => {
                formatter.write_str(
                    "plan contains duplicate step ids",
                )
            }

            Self::SelfDependency { step_id } => {
                write!(
                    formatter,
                    "step {step_id} cannot depend on itself"
                )
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
        let classify = step("classify")
            .depends_on(scan.id.clone());
        let move_files = step("move")
            .depends_on(classify.id.clone());

        let mut plan = Plan::new("organize files").unwrap();

        plan.add_step(scan).unwrap();
        plan.add_step(classify).unwrap();
        plan.add_step(move_files).unwrap();

        assert_eq!(validate_plan(&plan), Ok(()));
    }

    #[test]
    fn accepts_valid_branching_plan() {
        let source = step("source");

        let left = step("left")
            .depends_on(source.id.clone());

        let right = step("right")
            .depends_on(source.id.clone());

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

        assert_eq!(
            validate_plan(&plan),
            Err(PlanValidationError::EmptyPlan)
        );
    }

    #[test]
    fn rejects_unknown_dependency() {
        let missing = PlanStepId::from_static("missing");

        let dependent = step("dependent")
            .depends_on(missing.clone());

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
            Err(PlanValidationError::SelfDependency {
                step_id: id,
            })
        );
    }

    #[test]
    fn rejects_duplicate_dependency() {
        let source = step("source");
        let source_id = source.id.clone();

        let dependent = PlanStep {
            dependencies: vec![
                source_id.clone(),
                source_id.clone(),
            ],
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

        let first = step("first")
            .depends_on(second_id.clone());

        let second = step("second")
            .depends_on(first_id);

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
        let first = step("first")
            .depends_on(PlanStepId::from_static("third"));

        let second = step("second")
            .depends_on(PlanStepId::from_static("first"));

        let third = step("third")
            .depends_on(PlanStepId::from_static("second"));

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
RUST

cat > "$PLANNER_DIR/mod.rs" <<'RUST'
pub mod domain;
pub mod validation;

pub use domain::{
    Plan,
    PlanDomainError,
    PlanId,
    PlanStatus,
    PlanStep,
    PlanStepId,
    PlanStepStatus,
    StepInput,
    StepOutput,
    TimestampMs,
};

pub use validation::{
    validate_plan,
    PlanValidationError,
};
RUST

python3 - "$LIB_FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")

module_line = "mod planner;\n"

if module_line in text or "pub mod planner;\n" in text:
    raise SystemExit("planner module declaration already exists")

anchor = "mod openclaw;\n"

if anchor not in text:
    raise SystemExit("cannot find mod openclaw; insertion anchor")

text = text.replace(
    anchor,
    anchor + "pub mod planner;\n",
    1,
)

path.write_text(text, encoding="utf-8")
PY

printf '\n正在运行 cargo fmt...\n'
cargo fmt --manifest-path "$CARGO_TOML" --all

printf '\n正在运行 cargo check...\n'
cargo check --manifest-path "$CARGO_TOML"

printf '\n正在运行 Planner 测试...\n'
cargo test --manifest-path "$CARGO_TOML" planner

printf '\n正在运行完整 Rust 测试...\n'
cargo test --manifest-path "$CARGO_TOML"

printf '\n========================================\n'
printf 'P10-M3 Planner Domain 已通过自动验证。\n'
printf '========================================\n\n'

printf '检查修改：\n'
printf '  git status\n'
printf '  git diff -- src-tauri/src/planner src-tauri/src/lib.rs\n\n'

printf '验收后提交：\n'
printf '  mkdir -p scripts/milestones\n'
printf '  mv p10_m3_planner_domain.sh scripts/milestones/\n'
printf '  git add src-tauri/src/planner src-tauri/src/lib.rs scripts/milestones/p10_m3_planner_domain.sh\n'
printf '  git commit -m "feat(planner): add P10-M3 planner domain model"\n'
printf '  git push --set-upstream origin feature/p10-m3-planner-domain\n'
