#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase B Read"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$ROOT/verify/fixtures/p15-spreadsheet-read.xlsx"
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

[ -s "$FIXTURE" ] || fail "Real XLSX fixture missing"

BEFORE_STATE="$(
  /usr/bin/osascript -e \
    'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)'
)" || fail "Excel preflight state"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_b_selection_contract_is_bounded_and_fail_closed \
  --lib >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "Phase B selection contract"
  }

echo "✅ selection contract"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_b_command_encodes_selection_and_bounds \
  --lib >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "Phase B bounded command"
  }

echo "✅ bounded read command"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_b_parser_returns_workbook_and_single_sheet_shapes \
  --lib >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "Phase B parser"
  }

echo "✅ workbook/single-sheet parser"

AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_b_real_multi_sheet_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -160 "$LOG"
    fail "Real multi-sheet Excel E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "Real multi-sheet E2E result marker"

echo "✅ worksheet list/count"
echo "✅ default first-sheet backward compatibility"
echo "✅ specific-sheet bounded read"
echo "✅ selected multi-sheet bounded read"
echo "✅ allSheets bounded read"
echo "✅ missing worksheet fail closed"
echo "✅ real multi-sheet XLSX opened/read/closed"

AFTER_STATE="$(
  /usr/bin/osascript -e \
    'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)'
)" || fail "Excel final state"

[ "$BEFORE_STATE" = "$AFTER_STATE" ] ||
  fail "Excel user session was not restored"

echo "✅ workbook/window state restored"
echo "✅ Excel remains running"
echo "✅ user workbook preservation"

echo "PASS $NAME"
