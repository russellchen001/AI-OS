#!/bin/bash
set -u

NAME="P15 Apple Numbers Spreadsheet Capability"
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
  document::numbers::tests::numbers_paths_and_create_bounds_fail_closed \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "path contract"
  }

echo "✅ absolute paths, .numbers extension, no-overwrite, bounded content"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_trims_the_blank_grid_a_new_table_starts_with \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "grid trimming contract"
  }

echo "✅ the default blank grid is trimmed"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_read_parser_returns_a_trimmed_single_sheet \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "read parser contract"
  }

echo "✅ localized sheet names are reported without being used as addresses"
echo "PASS $NAME"
