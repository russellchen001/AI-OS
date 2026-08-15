#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/src-tauri/Cargo.toml"
RESULT_DIR="$(mktemp -d)"
trap 'rm -rf "$RESULT_DIR"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 file write: $1"
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
  echo "FAIL P15 file write: $label"
  exit 1
}

run_test \
  "confirmed filesystem write is allowed while move remains denied" \
  "one_time_user_confirmation_allows_filesystem_write_but_not_move" \
  "write-permission"

run_test \
  "filesystem write reaches the OpenClaw agent and returns bytes written" \
  "filesystem_write_runs_agent_and_returns_created_file_result" \
  "write-agent"

run_test \
  "existing and failed targets return explicit fail-closed results" \
  "filesystem_write_reports_existing_and_failed_targets" \
  "write-status"

run_test \
  "overwrite, oversized content, and relative paths are rejected before Gateway execution" \
  "filesystem_write_rejects_overwrite_oversize_and_relative_paths" \
  "write-limits"

if (
  cd "$ROOT_DIR" &&
  node --experimental-strip-types --input-type=module -e '
    import { describeChatTaskError } from "./src/services/tasks.ts";
    const expected = "OpenClaw Runtime could not complete this file write. Check OpenClaw and try again.";
    const actual = describeChatTaskError("safe runtime failure", true, "file write");
    if (actual !== expected) throw new Error(`expected ${expected}, received ${actual}`);
    const ask = describeChatTaskError(new Error("provider failed"), false, "file write");
    if (!ask.includes("selected AI connection")) throw new Error("ASK error semantics changed");
  ' >"$RESULT_DIR/error-mapping.log" 2>&1
); then
  echo "✓ file write errors use Work/OpenClaw semantics"
else
  head -20 "$RESULT_DIR/error-mapping.log"
  fail "file write error mapping"
fi

if (cd "$ROOT_DIR" && npm run build >"$RESULT_DIR/frontend-build.log" 2>&1); then
  echo "✓ save-path picker and bounded write result UI build successfully"
else
  grep -E "error TS|ERROR|Build failed|failed to build" "$RESULT_DIR/frontend-build.log" | head -20
  fail "frontend build"
fi

echo "PASS P15 file write"
