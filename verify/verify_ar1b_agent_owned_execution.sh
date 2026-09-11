#!/bin/bash
set -u

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0

check() {
  NAME="$1"
  shift

  if "$@"; then
    echo "PASS $NAME"
    PASS=$((PASS + 1))
  else
    echo "FAIL $NAME"
    FAIL=$((FAIL + 1))
  fi
}

contains() {
  FILE="$1"
  PATTERN="$2"
  rg -q "$PATTERN" "$FILE"
}

not_contains() {
  FILE="$1"
  PATTERN="$2"
  ! rg -q "$PATTERN" "$FILE"
}

echo "============================================================"
echo "VERIFY AR-1B — AGENT-OWNED EXECUTION"
echo "============================================================"

check \
  "Plan stores selected Agent as control-plane metadata" \
  contains \
  src-tauri/src/planner/domain.rs \
  'pub agent_id: Option<String>'

check \
  "PlanRuntimeExecutionRequest carries Agent identity" \
  contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'pub agent_id: Option<String>'

check \
  "PlanRuntimeExecutionRequest carries Task identity" \
  contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'pub task_id: TaskId'

check \
  "Task layer no longer hard-codes only openclaw" \
  not_contains \
  src-tauri/src/task_execution.rs \
  'agent_id != "openclaw"'

check \
  "Task Plan stores selected Agent" \
  contains \
  src-tauri/src/task_execution.rs \
  'plan\.agent_id = Some\(agent_id\.to_owned\(\)\)'

check \
  "No Agent id is hidden in generic Do-task Skill input" \
  not_contains \
  src-tauri/src/task_execution.rs \
  '"agentId"\.to_owned\(\)'

check \
  "Generic no-Skill Do-task uses agent.execute control-plane step" \
  contains \
  src-tauri/src/task_execution.rs \
  '"agent\.execute"'

check \
  "Runtime no longer routes user Do-task by skill.executor.kind" \
  not_contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'skill\.executor'

check \
  "Runtime no longer direct-dispatches MCP" \
  not_contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'execute_mcp_runtime_task'

check \
  "Runtime no longer direct-dispatches local model backend" \
  not_contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'execute_local_model_runtime_task'

check \
  "Runtime no longer direct-dispatches media backend" \
  not_contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  'execute_generative_media_runtime_task'

check \
  "Agent capability contract exists" \
  contains \
  src-tauri/src/runtime/agent_execution.rs \
  'enum AgentCapability'

check \
  "Agent capability probe result exists" \
  contains \
  src-tauri/src/runtime/agent_execution.rs \
  'struct AgentProbeResult'

check \
  "Capability negotiation exists" \
  contains \
  src-tauri/src/runtime/agent_execution.rs \
  'fn negotiate_agent_compatibility'

check \
  "Skill transport is separate capability" \
  contains \
  src-tauri/src/runtime/agent_execution.rs \
  'enum AgentSkillTransport'

check \
  "No exact OpenClaw version is embedded in Runtime bridge" \
  not_contains \
  src-tauri/src/runtime/plan_runtime_bridge.rs \
  '2026\.8\.2'

echo
echo "=== Rust tests ==="

if (
  cd src-tauri &&
  cargo test runtime::agent_execution --lib &&
  cargo test runtime::plan_runtime_bridge --lib &&
  cargo test task_execution --lib &&
  cargo test task_plan_orchestration --lib
); then
  echo "PASS targeted Rust tests"
  PASS=$((PASS + 1))
else
  echo "FAIL targeted Rust tests"
  FAIL=$((FAIL + 1))
fi

echo
echo "=== formatting ==="

if rustfmt --check \
  src-tauri/src/planner/domain.rs \
  src-tauri/src/task_execution.rs \
  src-tauri/src/task_plan_orchestration.rs \
  src-tauri/src/runtime/agent_execution.rs \
  src-tauri/src/runtime/plan_runtime_bridge.rs
then
  echo "PASS targeted rustfmt check"
  PASS=$((PASS + 1))
else
  echo "FAIL targeted rustfmt check"
  FAIL=$((FAIL + 1))
fi

echo
echo "=== diff integrity ==="

if git diff --check; then
  echo "PASS git diff --check"
  PASS=$((PASS + 1))
else
  echo "FAIL git diff --check"
  FAIL=$((FAIL + 1))
fi

echo
echo "============================================================"
echo "PASS=$PASS FAIL=$FAIL"
echo "============================================================"

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
