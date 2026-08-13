#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/src-tauri/Cargo.toml"
RESULT_DIR="$(mktemp -d)"
trap 'rm -rf "$RESULT_DIR"' EXIT

run_test() {
  local label="$1"
  local filter="$2"
  local output_file="$RESULT_DIR/$3.log"

  if cargo test --manifest-path "$MANIFEST_PATH" "$filter" --quiet >"$output_file" 2>&1; then
    echo "✓ $label"
    return 0
  fi

  echo "✗ $label"
  grep -E "error|FAILED|failures:" "$output_file" | head -20
  echo "FAIL P15 file execution contract: $label"
  exit 1
}

run_test \
  "explicit capability and input reach the Plan Runtime request" \
  "explicit_core_skill_capability_and_input_reach_plan_runtime_request" \
  "explicit-capability"

run_test \
  "sessions.create fallback remains unchanged" \
  "missing_core_skill_capability_preserves_sessions_create_fallback" \
  "sessions-fallback"

run_test \
  "existing Plan Runtime bridge behavior remains valid" \
  "runtime::plan_runtime_bridge" \
  "runtime-bridge"

echo "PASS P15 file execution contract"
