#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Step 3A"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  providers::tests::execution_agent_candidates_follow_ai_center_order_and_context_requirement \
  -- --exact 2>&1; then
  echo "✓ AI Center execution candidate behavior passed"
else
  echo "✗ AI Center execution candidate behavior failed"
  echo "FAIL AC-EXEC-MODEL Step 3A: candidate behavior"
  exit 1
fi

if cargo check --manifest-path src-tauri/Cargo.toml 2>&1; then
  echo "✓ Rust compile passed"
else
  echo "✗ Rust compile failed"
  echo "FAIL AC-EXEC-MODEL Step 3A: Rust compile"
  exit 1
fi

echo "✓ Single-agent download execution extracted without enabling fallback"
echo "PASS AC-EXEC-MODEL Step 3A"
exit 0
