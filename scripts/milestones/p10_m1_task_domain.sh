#!/usr/bin/env bash
set -euo pipefail

BRANCH="feature/p10-m1-task-domain"
CARGO_TOML="src-tauri/Cargo.toml"
SRC_ROOT="src-tauri/src"
TASK_DIR="$SRC_ROOT/task_engine"

fail() {
  printf '\nERROR: %s\n' "$1" >&2
  exit 1
}

[[ -f "$CARGO_TOML" ]] || fail "请在 AI-OS 仓库根目录运行。找不到 $CARGO_TOML"
command -v python3 >/dev/null 2>&1 || fail "找不到 python3。"
command -v cargo >/dev/null 2>&1 || fail "找不到 Rust/Cargo。"

if command -v git >/dev/null 2>&1 &&
  git rev-parse --is-inside-work-tree >/dev/null 2>&1; then

  if [[ -n "$(git status --porcelain)" ]]; then
    printf '\nWARNING: 当前仓库存在尚未提交的修改。\n'
    printf '脚本只会修改 P10-M1 相关文件，但建议先确认 git status。\n\n'
  fi

  if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
    git switch "$BRANCH"
  else
    git switch -c "$BRANCH"
  fi
fi

if [[ -e "$TASK_DIR/domain.rs" || -e "$TASK_DIR/mod.rs" ]]; then
  fail "$TASK_DIR 已经存在 P10-M1 文件。为避免覆盖，脚本已停止。"
fi

mkdir -p "$TASK_DIR"

cp "$CARGO_TOML" "$CARGO_TOML.p10-m1.bak"

python3 - "$CARGO_TOML" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")

if not re.search(r"(?m)^\[dependencies\]\s*$", text):
    raise SystemExit("Cargo.toml 中找不到 [dependencies]")

def insert_dependency(source: str, declaration: str) -> str:
    match = re.search(r"(?m)^\[dependencies\]\s*$", source)
    assert match is not None
    return source[:match.end()] + "\n" + declaration + source[match.end():]

serde_match = re.search(r"(?m)^(\s*serde\s*=\s*)(.+)$", text)

if serde_match is None:
    text = insert_dependency(
        text,
        'serde = { version = "1", features = ["derive"] }'
    )
else:
    prefix = serde_match.group(1)
    rhs = serde_match.group(2).strip()
    replacement = None

    if rhs.startswith('"') and rhs.endswith('"'):
        replacement = (
            f'{prefix}{{ version = {rhs}, features = ["derive"] }}'
        )
    elif rhs.startswith("{") and rhs.endswith("}") and "derive" not in rhs:
        if "features" in rhs:
            rhs = re.sub(
                r"features\s*=\s*\[",
                'features = ["derive", ',
                rhs,
                count=1,
            )
        else:
            rhs = rhs[:-1].rstrip()
            if not rhs.endswith("{"):
                rhs += ","
            rhs += ' features = ["derive"] }'

        replacement = prefix + rhs

    if replacement is not None:
        text = (
            text[:serde_match.start()]
            + replacement
            + text[serde_match.end():]
        )

if not re.search(r"(?m)^\s*serde_json\s*=", text):
    text = insert_dependency(text, 'serde_json = "1"')

path.write_text(text, encoding="utf-8")
PY

cat > "$TASK_DIR/mod.rs" <<'RUST'
pub mod domain;

pub use domain::{
    Task, TaskContext, TaskError, TaskId, TaskPlan, TaskPriority, TaskResult,
    TaskStatus, TaskType, TimestampMs,
};
RUST

cat > "$TASK_DIR/domain.rs" <<'RUST'
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub type TimestampMs = u64;
pub type TaskContext = BTreeMap<String, Value>;
pub type TaskPlan = Vec<Value>;
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

        Self(format!(
            "task_{timestamp}_{process_id}_{counter:06}"
        ))
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
    pub plan: TaskPlan,
    pub result: Option<TaskResult>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl Task {
    pub fn new(
        task_type: TaskType,
        intent: impl Into<String>,
    ) -> Result<Self, TaskError> {
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
            plan: TaskPlan::new(),
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

        match (self.task_type, self.status, next) {
            (
                _,
                TaskStatus::Created,
                TaskStatus::Understanding,
            ) => true,

            (
                TaskType::Ask,
                TaskStatus::Understanding,
                TaskStatus::Ready,
            ) => true,

            (
                TaskType::Do,
                TaskStatus::Understanding,
                TaskStatus::Planning,
            ) => true,

            (
                _,
                TaskStatus::Planning,
                TaskStatus::Ready,
            ) => true,

            (
                _,
                TaskStatus::Ready,
                TaskStatus::Executing,
            ) => true,

            (
                _,
                TaskStatus::Executing,
                TaskStatus::Verifying | TaskStatus::Recovery,
            ) => true,

            (
                _,
                TaskStatus::Verifying,
                TaskStatus::Completed | TaskStatus::Recovery,
            ) => true,

            (
                _,
                TaskStatus::Recovery,
                TaskStatus::Executing,
            ) => true,

            _ => false,
        }
    }

    pub fn transition_to(
        &mut self,
        next: TaskStatus,
    ) -> Result<(), TaskError> {
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

    InvalidTransition {
        task_type: TaskType,
        from: TaskStatus,
        to: TaskStatus,
    },
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIntent => {
                formatter.write_str("task intent must not be empty")
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
        let task = Task::new(
            TaskType::Do,
            "send email",
        )
        .expect("task should be created");

        let task_id = task.id.clone();

        assert!(task.id.as_str().starts_with("task_"));
        assert_eq!(task.id, task_id);
        assert_eq!(task.status, TaskStatus::Created);
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.context.is_empty());
        assert!(task.plan.is_empty());
        assert!(task.result.is_none());
        assert!(task.updated_at >= task.created_at);
    }

    #[test]
    fn rejects_empty_intent() {
        assert_eq!(
            Task::new(TaskType::Ask, "   "),
            Err(TaskError::EmptyIntent)
        );
    }

    #[test]
    fn ask_task_can_skip_planning() {
        let mut task = Task::new(
            TaskType::Ask,
            "summarize document",
        )
        .unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Ready).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Verifying).unwrap();
        task.transition_to(TaskStatus::Completed).unwrap();

        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn do_task_requires_planning() {
        let mut task = Task::new(
            TaskType::Do,
            "send email",
        )
        .unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();

        assert!(matches!(
            task.transition_to(TaskStatus::Ready),
            Err(TaskError::InvalidTransition { .. })
        ));

        task.transition_to(TaskStatus::Planning).unwrap();
        task.transition_to(TaskStatus::Ready).unwrap();
        task.transition_to(TaskStatus::Executing).unwrap();
        task.transition_to(TaskStatus::Verifying).unwrap();
        task.transition_to(TaskStatus::Completed).unwrap();

        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn supports_recovery_and_retry() {
        let mut task = Task::new(
            TaskType::Do,
            "download file",
        )
        .unwrap();

        task.transition_to(TaskStatus::Understanding).unwrap();
        task.transition_to(TaskStatus::Planning).unwrap();
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
        let mut completed = Task::new(
            TaskType::Ask,
            "answer question",
        )
        .unwrap();

        completed
            .transition_to(TaskStatus::Understanding)
            .unwrap();

        completed.transition_to(TaskStatus::Ready).unwrap();
        completed.transition_to(TaskStatus::Executing).unwrap();
        completed.transition_to(TaskStatus::Verifying).unwrap();
        completed.transition_to(TaskStatus::Completed).unwrap();

        assert!(
            completed
                .transition_to(TaskStatus::Failed)
                .is_err()
        );

        let mut failed = Task::new(
            TaskType::Do,
            "send email",
        )
        .unwrap();

        failed.transition_to(TaskStatus::Failed).unwrap();

        assert!(
            failed
                .transition_to(TaskStatus::Understanding)
                .is_err()
        );
    }

    #[test]
    fn serializes_and_deserializes_without_data_loss() {
        let mut task = Task::new(
            TaskType::Do,
            "organize files",
        )
        .unwrap();

        task.context.insert(
            "path".to_owned(),
            json!("/Users/example"),
        );

        task.plan.push(json!({
            "step": "scan"
        }));

        task.result = Some(json!({
            "organized": 12
        }));

        let serialized = serde_json::to_string(&task).unwrap();

        let deserialized: Task =
            serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized, task);
    }
}
RUST

MODULE_FILE=""

if [[ -f "$SRC_ROOT/lib.rs" ]]; then
  MODULE_FILE="$SRC_ROOT/lib.rs"
elif [[ -f "$SRC_ROOT/main.rs" ]]; then
  MODULE_FILE="$SRC_ROOT/main.rs"
else
  fail "找不到 src-tauri/src/lib.rs 或 src-tauri/src/main.rs"
fi

cp "$MODULE_FILE" "$MODULE_FILE.p10-m1.bak"

python3 - "$MODULE_FILE" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")

pattern = r"(?m)^\s*(?:pub\s+)?mod\s+task_engine\s*;"

if not re.search(pattern, text):
    if text and not text.endswith("\n"):
        text += "\n"

    text += "\npub mod task_engine;\n"

path.write_text(text, encoding="utf-8")
PY

printf '\n正在运行 cargo fmt...\n'
cargo fmt --manifest-path "$CARGO_TOML" --all

printf '\n正在运行 cargo check...\n'
cargo check --manifest-path "$CARGO_TOML"

printf '\n正在运行 Task Engine 测试...\n'
cargo test --manifest-path "$CARGO_TOML" task_engine

printf '\n正在运行完整 Rust 测试...\n'
cargo test --manifest-path "$CARGO_TOML"

printf '\n========================================\n'
printf 'P10-M1 已完成并通过自动验证。\n'
printf '========================================\n\n'

printf '查看修改：\n'
printf '  git status\n'
printf '  git diff -- src-tauri/Cargo.toml src-tauri/src\n\n'

printf '确认无误后提交：\n'
printf '  git add src-tauri/Cargo.toml src-tauri/src/task_engine "%s"\n' "$MODULE_FILE"
printf '  git commit -m "feat(task-engine): add P10-M1 task domain model"\n'
