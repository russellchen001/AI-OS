#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT_DIR/src-tauri/Cargo.toml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 OpenClaw history compatibility: $1"
  exit 1
}

echo "P15 OpenClaw History Compatibility"

if cargo test --manifest-path "$MANIFEST" \
  filesystem_output_accepts_nested_openclaw_transcript_messages \
  --quiet >"$TMP/nested.log" 2>&1; then
  echo "✓ real nested OpenClaw transcript is accepted"
else
  grep -E "error|FAILED|failures:" "$TMP/nested.log" | head -20
  fail "nested transcript parsing"
fi

for script in \
  verify_p15_file_scan_confirmation.sh \
  verify_p15_file_read.sh \
  verify_p15_file_write.sh \
  verify_p15_file_move.sh
do
  if "$ROOT_DIR/verify/$script" >"$TMP/$script.log" 2>&1; then
    echo "✓ $script"
  else
    grep -E "FAIL|error|FAILED|failures:" "$TMP/$script.log" | head -20
    fail "$script"
  fi
done

if cargo test --manifest-path "$MANIFEST" --quiet >"$TMP/rust.log" 2>&1; then
  echo "✓ full Rust regression"
else
  grep -E "error|FAILED|failures:" "$TMP/rust.log" | head -30
  fail "full Rust regression"
fi

echo "PASS P15 OpenClaw history compatibility"
exit 0
