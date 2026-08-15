#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST_PATH="$ROOT_DIR/src-tauri/Cargo.toml"
RESULT_DIR="$(mktemp -d)"
trap 'rm -rf "$RESULT_DIR"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 file read: $1"
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
  echo "FAIL P15 file read: $label"
  exit 1
}

run_test \
  "confirmed filesystem read is allowed while write remains denied" \
  "one_time_user_confirmation_allows_filesystem_read_but_not_write" \
  "read-permission"

run_test \
  "filesystem read reaches the OpenClaw agent and returns limited text" \
  "filesystem_read_runs_agent_and_returns_limited_text_result" \
  "read-agent"

run_test \
  "binary and oversized files return explicit bounded results" \
  "filesystem_read_reports_binary_and_oversized_results_without_content" \
  "read-limits"

if (
  cd "$ROOT_DIR" &&
  node --experimental-strip-types --input-type=module -e '
    import { describeChatTaskError } from "./src/services/tasks.ts";
    const expected = "OpenClaw Runtime could not complete this file read. Check OpenClaw and try again.";
    const actual = describeChatTaskError("safe runtime failure", true, "file read");
    if (actual !== expected) throw new Error(`expected ${expected}, received ${actual}`);
    const ask = describeChatTaskError(new Error("provider failed"), false, "file read");
    if (!ask.includes("selected AI connection")) throw new Error("ASK error semantics changed");
  ' >"$RESULT_DIR/error-mapping.log" 2>&1
); then
  echo "✓ file read errors use Work/OpenClaw semantics"
else
  head -20 "$RESULT_DIR/error-mapping.log"
  fail "file read error mapping"
fi

if (cd "$ROOT_DIR" && npm run build >"$RESULT_DIR/frontend-build.log" 2>&1); then
  echo "✓ file picker and readable result UI build successfully"
else
  grep -E "error TS|ERROR|Build failed|failed to build" "$RESULT_DIR/frontend-build.log" | head -20
  fail "frontend build"
fi

echo "PASS P15 file read"
