#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase G Formula-Aware Read"
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

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_include_formulas_is_opt_in_and_typed \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "includeFormulas input validation"
  }

echo "✅ includeFormulas is opt-in and must be a boolean"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_g_command_asks_excel_for_formulas_only_when_requested \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "command encoding"
  }

echo "✅ one AppleScript, the argv decides whether formulas are read"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_g_parser_reports_formulas_sparsely \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "parser contract"
  }

echo "✅ sparse formulas, absent key when not requested, short count is truncation"

# verify/probe_excel_formula_read_semantics.sh established that
# `formula of used range` returns the same 2D list as `value of used range`,
# that a constant cell reports the constant (so only a leading "=" marks a real
# formula), and that `formula` is English even on a Chinese-locale Excel.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  runtime::openclaw_gateway_adapter::tests::spreadsheet_read_phase_g_formula_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real formula read E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real formula read E2E result marker"

echo "✅ values-only read is unchanged and reports no formula keys"
echo "✅ formula-aware read returns the computed value AND the formula"
echo "✅ only the cell that holds a formula is reported"
echo "✅ used range reported so the grid maps to real addresses"

# Phase B read is the contract this extends; it must still hold.
bash verify/verify_p15_excel_phase_b_read.sh >"$TMP_DIR/phase_b_read.log" 2>&1 || {
  tail -60 "$TMP_DIR/phase_b_read.log"
  fail "Phase B read regression"
}
echo "✅ Phase B read regression"

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
