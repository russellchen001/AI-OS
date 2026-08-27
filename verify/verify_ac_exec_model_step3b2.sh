#!/bin/bash
set -u

NAME="AC-EXEC-MODEL Step 3B-2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
cd "$ROOT" || exit 1

fail() {
  echo "✗ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests \
  --lib >"$TMP_DIR/tests.log" 2>&1 ||
  {
    tail -30 "$TMP_DIR/tests.log"
    fail "adapter tests"
  }

grep -Eq "running [1-9][0-9]* tests" "$TMP_DIR/tests.log" ||
  {
    tail -30 "$TMP_DIR/tests.log"
    fail "adapter tests did not run"
  }

grep -q "download_agent_fallback_is_limited_to_retryable_file_verification_failure ... ok" \
  "$TMP_DIR/tests.log" ||
  {
    tail -30 "$TMP_DIR/tests.log"
    fail "sequential candidate fallback test missing"
  }

grep -q "test result: ok\." "$TMP_DIR/tests.log" ||
  {
    tail -30 "$TMP_DIR/tests.log"
    fail "adapter test result"
  }

echo "✓ OpenClaw adapter behavior passed"

cargo check --manifest-path src-tauri/Cargo.toml >"$TMP_DIR/check.log" 2>&1 ||
  {
    tail -30 "$TMP_DIR/check.log"
    fail "Rust compile"
  }

echo "✓ Rust compile passed"
echo "✓ Sequential AI Center execution-agent fallback is build-valid"
echo "PASS $NAME"
exit 0
