#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: P14 Memory Retrieval"
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

run_check "Frontend production build" npm run build
run_check "Cargo check" cargo check --manifest-path src-tauri/Cargo.toml
run_check "Git diff check" git diff --check

echo "PASS: P14 Memory Retrieval"
