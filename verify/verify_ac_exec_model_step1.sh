#!/bin/bash

set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Step 1"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  providers::tests::execution_agent_candidates_follow_ai_center_order_and_context_requirement \
  -- --exact 2>&1; then
  echo "✓ Capability context requirement and AI Center ordering passed"
else
  echo "✗ Execution-agent candidate routing failed"
  echo "FAIL AC-EXEC-MODEL Step 1: candidate routing"
  exit 1
fi

if cargo check --manifest-path src-tauri/Cargo.toml >/tmp/ac-exec-model-step1-check.log 2>&1; then
  echo "✓ Rust compile passed"
else
  echo "✗ Rust compile failed"
  grep -E "error(\[|:)" /tmp/ac-exec-model-step1-check.log | head -20
  echo "FAIL AC-EXEC-MODEL Step 1: compile"
  exit 1
fi

echo "PASS AC-EXEC-MODEL Step 1"
exit 0
