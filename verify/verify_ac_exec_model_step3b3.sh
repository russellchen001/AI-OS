#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Step 3B-3"

TEST_LOG="/tmp/ac-exec-step3b3-tests.log"
CHECK_LOG="/tmp/ac-exec-step3b3-check.log"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::download_agent_fallback_is_limited_to_retryable_file_verification_failure \
  --lib -- --exact >"$TEST_LOG" 2>&1; then
  echo "✓ Download fallback policy passed"
else
  tail -30 "$TEST_LOG"
  echo "✗ Download fallback policy failed"
  echo "FAIL AC-EXEC-MODEL Step 3B-3: fallback policy"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests:: \
  --lib >"$TEST_LOG" 2>&1; then
  echo "✓ Existing OpenClaw adapter behavior passed"
else
  tail -30 "$TEST_LOG"
  echo "✗ Existing OpenClaw adapter tests failed"
  echo "FAIL AC-EXEC-MODEL Step 3B-3: adapter regression"
  exit 1
fi

if cargo check \
  --manifest-path src-tauri/Cargo.toml >"$CHECK_LOG" 2>&1; then
  echo "✓ Rust compile passed"
else
  tail -30 "$CHECK_LOG"
  echo "✗ Rust compile failed"
  echo "FAIL AC-EXEC-MODEL Step 3B-3: compile"
  exit 1
fi

echo "✓ Fallback is capped at two execution agents"
echo "✓ Non-file-verification failures stop immediately"
echo "PASS AC-EXEC-MODEL Step 3B-3"
exit 0
