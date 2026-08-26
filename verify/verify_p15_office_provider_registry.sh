#!/bin/bash
set -u

NAME="P15-2 Office Provider Registry"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "✗ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

CHECK_LOG="/tmp/p15-office-provider-check.log"
if cargo check --manifest-path src-tauri/Cargo.toml >"$CHECK_LOG" 2>&1; then
  echo "✓ Rust compile"
else
  tail -30 "$CHECK_LOG"
  fail "Rust compile"
fi

TEST_LOG="/tmp/p15-office-provider-tests.log"
if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::registry::tests \
  >"$TEST_LOG" 2>&1
then
  echo "✓ Provider ordering and Local First policy"
  echo "✓ Native and cloud provider boundaries"
  echo "✓ Unknown capabilities fail closed"
  echo "✓ macOS document.read resolves to Native provider"
else
  tail -40 "$TEST_LOG"
  fail "Office Provider registry behavior"
fi

echo "PASS $NAME"
exit 0
