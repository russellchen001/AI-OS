#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase H Realistic Workflow"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
FIXTURE="$TMP_DIR/created.xlsx"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

BEFORE_STATE="$(
  /usr/bin/osascript -e \
    'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)'
)" || fail "Excel preflight state"

KEEP_FIXTURE=1 \
KEEP_FIXTURE_PATH="$FIXTURE" \
bash verify/verify_p15_spreadsheet_create.sh \
  >"$TMP_DIR/create.log" 2>&1 || {
    tail -50 "$TMP_DIR/create.log"
    fail "Spreadsheet Create fixture"
  }

[ -s "$FIXTURE" ] || fail "Preserved XLSX fixture"

echo "✅ Existing Spreadsheet Create produced real XLSX"

# The only test that crosses both halves of the Excel work: the edit adapter
# builds the workbook, and a separate spreadsheet.read invocation reads it back.
# Every per-phase E2E judges the adapter against its own reopened copy; this one
# judges the file as a caller actually receives it.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::excel_phase_h_realistic_workflow_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -160 "$LOG"
    fail "realistic workflow E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "realistic workflow E2E result marker"

echo "✅ typed cells, formulas, sort, formatting, width, chart and filter in one workbook"
echo "✅ descending sort by a computed total reordered the rows"
echo "✅ writes made before the sort reported as displaced, not asserted stale"
echo "✅ formatting, width, chart and filter asserted at fixed addresses after it"
echo "✅ file read back through spreadsheet.read in its own invocation"
echo "✅ row order, hidden-not-deleted filtering and rewritten formulas all hold"
echo "✅ source workbook preserved"

AFTER_STATE="$(
  /usr/bin/osascript -e \
    'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)'
)" || fail "Excel final state"

[ "$BEFORE_STATE" = "$AFTER_STATE" ] ||
  fail "Excel user session was not restored"

echo "✅ Excel remains running"
echo "✅ workbook/window state restored"
echo "✅ user-owned workbook preserved"

echo "PASS $NAME"
