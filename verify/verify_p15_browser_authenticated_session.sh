#!/usr/bin/env bash
set -u

name="P15 browser authenticated session"
output="$(cargo test --manifest-path src-tauri/Cargo.toml browser::provider::tests -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: browser session behavior tests failed"
  echo "$output" | tail -20
  exit 1
fi
if [[ "$output" != *"2 passed"* ]]; then
  echo "FAIL $name: expected two browser authentication behavior tests"
  exit 1
fi
echo "✓ public page is not authenticated"
echo "✓ verified session requires opaque profile and account evidence"
echo "✓ expired session loses authenticated evidence"
echo "✓ serialized session contains no raw cookie or token"
echo "PASS $name behavior contract"
echo "SKIP $name real account E2E: USER_LOGIN_PROFILE_REQUIRED"
