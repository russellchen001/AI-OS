#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase B Mutation"
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

grep -q "PASS P15-3 Spreadsheet Create" "$TMP_DIR/create.log" ||
  fail "Spreadsheet Create marker"

[ -s "$FIXTURE" ] || fail "Preserved XLSX fixture"

echo "✅ Existing Spreadsheet Create produced real XLSX"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::phase_b_mutation_operations_validate_structured_inputs \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "structured mutation validation"
  }

echo "✅ structured mutation validation"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::phase_b_mutation_encoding_preserves_explicit_identity \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "worksheet identity encoding"
  }

echo "✅ worksheet identity encoding"

AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_b_mutation_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real mutation E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real mutation E2E result marker"

echo "✅ AddWorksheet"
echo "✅ pre/post identity snapshot"
echo "✅ unique set-difference discovery"
echo "✅ no worksheet-index assumption"
echo "✅ RenameWorksheet"
echo "✅ named-sheet set_cell/set_formula"
echo "✅ DeleteWorksheet"
echo "✅ final worksheet-set reopen validation"

AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_b_delete_final_sheet_fails_closed \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "final worksheet delete guard"
  }

grep -q "test result: ok" "$LOG" ||
  fail "final worksheet guard result marker"

echo "✅ final worksheet delete fails closed"

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
