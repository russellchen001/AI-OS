#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

check() {
  local name="$1"
  shift
  if "$@"; then
    echo "✓ $name"
    PASS=$((PASS + 1))
  else
    echo "✗ $name"
    FAIL=$((FAIL + 1))
  fi
}

check \
  "Approval, linkage, confirmation, feedback, blocker and legacy behavior" \
  bash -c 'node_modules/.bin/esbuild verify/p16_council_execution.behavior.ts --bundle --platform=node --format=esm --outfile="$1/execution.mjs" >/dev/null && node "$1/execution.mjs"' _ "$TMP_DIR"

check \
  "Task context retains Council and recommendation ids" \
  bash -c 'output="$(cd src-tauri && cargo test task_execution::tests::submit_chat_task_creates_ask_task_ready_for_ai_center --lib 2>&1)"; printf "%s\n" "$output" | grep -Eq "test result: ok\\. 1 passed; 0 failed"'

check \
  "Generic DO Task plans the agent.execute control-plane step" \
  bash -c 'output="$(cd src-tauri && cargo test task_execution::tests::missing_core_skill_capability_plans_generic_agent_execution --lib 2>&1)"; printf "%s\n" "$output" | grep -Eq "test result: ok\\. 1 passed; 0 failed"'

check \
  "Confirmed Skill request retains selected Agent and reaches Runtime" \
  bash -c 'output="$(cd src-tauri && cargo test task_execution::tests::explicit_core_skill_capability_and_input_reach_plan_runtime_request --lib 2>&1)"; printf "%s\n" "$output" | grep -Eq "test result: ok\\. 1 passed; 0 failed"'

check \
  "Council DO Task reaches Planner, Runtime, real OpenClaw and read-only Skill" \
  bash -c 'output="$(cd src-tauri && cargo test task_execution::tests::real_council_recommendation_runs_read_only_skill_through_openclaw --lib -- --ignored --exact 2>&1)"; printf "%s\n" "$output" | grep -Eq "test result: ok\\. 1 passed; 0 failed"'

check "TypeScript" npx tsc --noEmit
check "Git whitespace validation" git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Council Execution Core"
  exit 1
fi

echo "PASS: P16 Council Execution Core"
