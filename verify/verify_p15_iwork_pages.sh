#!/bin/bash
set -u

NAME="P15 Apple Pages Document Capability"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_paths_fail_closed_on_shape_existence_and_overwrite \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "path contract"
  }

echo "✅ absolute paths, correct extension, existence and no-overwrite"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_read_parser_refuses_a_short_payload \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "read parser contract"
  }

echo "✅ a payload shorter than it declared is refused, not reported as complete"
echo "PASS $NAME"
