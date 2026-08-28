#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT_DIR/src-tauri/Cargo.toml"
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ai-os-p15-task-closure.XXXXXX")"
trap 'rm -rf "$LOG_DIR"' EXIT

fail() {
  echo "✗ $1"
  tail -20 "$2" 2>/dev/null || true
  echo "FAIL P15 task closure contract: $1"
  exit 1
}

DOMAIN_LOG="$LOG_DIR/domain.log"
if cargo test --manifest-path "$MANIFEST" \
  preserves_task_closure_stage_and_legacy_plan_step_compatibility \
  >"$DOMAIN_LOG" 2>&1; then
  echo "✓ TaskClosureStage and legacy PlanStep compatibility"
else
  fail "TaskClosureStage compatibility test failed" "$DOMAIN_LOG"
fi

PLANNER_LOG="$LOG_DIR/planner.log"
if cargo test --manifest-path "$MANIFEST" planner >"$PLANNER_LOG" 2>&1; then
  echo "✓ Planner regression tests"
else
  fail "Planner regression tests failed" "$PLANNER_LOG"
fi

CHECK_LOG="$LOG_DIR/check.log"
if cargo check --manifest-path "$MANIFEST" >"$CHECK_LOG" 2>&1; then
  echo "✓ Rust cargo check"
else
  fail "cargo check failed" "$CHECK_LOG"
fi

echo "PASS P15 task closure contract"
exit 0
