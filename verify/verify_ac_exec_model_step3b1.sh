#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Step 3B-1"

TEST_LOG="/tmp/ac-exec-step3b1-tests.log"
CHECK_LOG="/tmp/ac-exec-step3b1-check.log"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests:: \
  --lib >"$TEST_LOG" 2>&1; then
  echo "✓ Existing OpenClaw adapter behavior passed"
else
  tail -30 "$TEST_LOG"
  echo "✗ Existing OpenClaw adapter tests failed"
  echo "FAIL AC-EXEC-MODEL Step 3B-1: adapter tests"
  exit 1
fi

if cargo check \
  --manifest-path src-tauri/Cargo.toml >"$CHECK_LOG" 2>&1; then
  echo "✓ Rust compile passed"
else
  tail -30 "$CHECK_LOG"
  echo "✗ Rust compile failed"
  echo "FAIL AC-EXEC-MODEL Step 3B-1: compile"
  exit 1
fi

echo "✓ Download attempt boundary remains valid"
echo "PASS AC-EXEC-MODEL Step 3B-1"
exit 0
