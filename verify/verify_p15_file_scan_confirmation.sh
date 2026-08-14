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
  echo "FAIL P15 file scan confirmation: $label"
  exit 1
}

if (
  cd "$ROOT_DIR" &&
  node --experimental-strip-types --input-type=module -e '
    import { describeChatTaskError } from "./src/services/tasks.ts";
    const expect = (actual, expected) => {
      if (actual !== expected) throw new Error(`expected ${expected}, received ${actual}`);
    };
    expect(
      describeChatTaskError("pairing required", true),
      "OpenClaw pairing is required. Pair OpenClaw and try the folder scan again.",
    );
    expect(
      describeChatTaskError("OpenClaw action is not permitted.", true),
      "OpenClaw permission was denied for this folder scan.",
    );
    expect(
      describeChatTaskError("connection unavailable", true),
      "OpenClaw is unavailable. Start or connect OpenClaw and try the folder scan again.",
    );
    const safeFallback = describeChatTaskError("token=secret", true);
    if (safeFallback.includes("secret") || safeFallback.includes("selected AI")) {
      throw new Error("Work fallback leaked raw detail or used AI Center wording");
    }
    expect(
      describeChatTaskError(new Error("NO_CONNECTED_PROVIDER"), false),
      "Connect and test an AI in My AI before starting a conversation.",
    );
    expect(
      describeChatTaskError(new Error("provider failed"), false),
      "AI‑OS could not complete this request. Check the selected AI connection and try again.",
    );
  ' >"$RESULT_DIR/frontend-error-mapping.log" 2>&1
); then
  echo "✓ Work errors use safe OpenClaw wording and ASK errors remain unchanged"
else
  echo "✗ Work/ASK error mapping behavior"
  head -20 "$RESULT_DIR/frontend-error-mapping.log"
  echo "FAIL P15 file scan confirmation: Work/ASK error mapping"
  exit 1
fi

run_test \
  "confirmed filesystem scan reaches the existing Runtime path" \
  "confirmed_filesystem_scan_crosses_permission_gate_with_original_input" \
  "confirmed-scan"

run_test \
  "one-time confirmation is scoped to filesystem.scan" \
  "one_time_user_confirmation_allows_only_filesystem_scan" \
  "confirmation-scope"

run_test \
  "unconfirmed scan remains denied without trusted automation" \
  "unconfirmed_filesystem_scan_remains_denied_without_trusted_automation" \
  "unconfirmed-scan"

run_test \
  "existing Plan Runtime bridge behavior remains valid" \
  "runtime::plan_runtime_bridge" \
  "runtime-bridge"

echo "PASS P15 file scan confirmation"
