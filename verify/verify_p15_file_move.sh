#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/src-tauri/Cargo.toml"
RESULT_DIR="$(mktemp -d)"
trap 'rm -rf "$RESULT_DIR"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 file move: $1"
  exit 1
}

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
  echo "FAIL P15 file move: $label"
  exit 1
}

run_test \
  "filesystem move requires explicit one-time confirmation" \
  "one_time_user_confirmation_allows_filesystem_move" \
  "move-permission"

run_test \
  "filesystem move reaches the OpenClaw agent and returns the moved paths" \
  "filesystem_move_runs_agent_and_returns_moved_result" \
  "move-agent"

run_test \
  "missing source, existing destination, and execution failure remain fail closed" \
  "filesystem_move_reports_fail_closed_statuses" \
  "move-status"

run_test \
  "overwrite, relative paths, and identical paths are rejected before Gateway execution" \
  "filesystem_move_rejects_overwrite_relative_and_same_paths" \
  "move-validation"

if (
  cd "$ROOT_DIR" &&
  node --experimental-strip-types --input-type=module -e '
    import { describeChatTaskError } from "./src/services/tasks.ts";
    const expected = "OpenClaw Runtime could not complete this file move. Check OpenClaw and try again.";
    const actual = describeChatTaskError("safe runtime failure", true, "file move");
    if (actual !== expected) throw new Error(`expected ${expected}, received ${actual}`);
    const ask = describeChatTaskError(new Error("provider failed"), false, "file move");
    if (!ask.includes("selected AI connection")) throw new Error("ASK error semantics changed");
  ' >"$RESULT_DIR/error-mapping.log" 2>&1
); then
  echo "✓ file move errors use Work/OpenClaw semantics"
else
  head -20 "$RESULT_DIR/error-mapping.log"
  fail "file move error mapping"
fi

if (cd "$ROOT_DIR" && npm run build >"$RESULT_DIR/frontend-build.log" 2>&1); then
  echo "✓ source/destination picker and move result UI build successfully"
else
  grep -E "error TS|ERROR|Build failed|failed to build" "$RESULT_DIR/frontend-build.log" | head -20
  fail "frontend build"
fi

echo "PASS P15 file move"
