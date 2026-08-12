#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: AI Center E2E QA"
  exit 1
}

run_check() {
  local label="$1"
  shift
  if "$@"; then
    echo "✅ $label"
  else
    fail "$label"
  fi
}

run_check "AI Center migration" ./verify/verify_p13_ai_center_migration.sh
run_check "Shared multi-model invocation" ./verify/verify_p13_m5_shared_multi_model.sh
run_check "Frontend production build" npm run build
run_check "Cargo check" cargo check --manifest-path src-tauri/Cargo.toml
run_check "Git diff check" git diff --check

echo "PASS: AI Center E2E QA"
