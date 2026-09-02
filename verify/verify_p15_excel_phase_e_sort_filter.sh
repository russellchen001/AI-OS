#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase E Sort and Filter"
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

SCRIPT_SRC="$TMP_DIR/edit.applescript"
awk '/^const EDIT_SCRIPT: &str = r#"$/{capture=1; next} /^"#;$/{capture=0} capture' \
  src-tauri/src/document/excel.rs >"$SCRIPT_SRC"

[ -s "$SCRIPT_SRC" ] || fail "Extract embedded AppleScript"

/usr/bin/osacompile -o "$TMP_DIR/edit.scpt" "$SCRIPT_SRC" \
  >"$TMP_DIR/osacompile.log" 2>&1 || {
    cat "$TMP_DIR/osacompile.log"
    fail "Embedded AppleScript compiles"
  }

echo "✅ embedded AppleScript compiles"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::sort_and_filter_operations_fail_closed_on_ambiguous_input \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "sort and filter input validation"
  }

echo "✅ key inside the range, explicit order and hasHeader, bounded field"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::sort_and_filter_encode_probe_rows_that_skip_a_header \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "sort and filter encoding"
  }

echo "✅ probe rows skip the header row"

# verify/probe_excel_sort_filter_semantics.sh established that autofilter can be
# judged by `autofilter mode` plus each row's `hidden` state, and that both
# survive save-as-xlsx, close and reopen. This asserts it on the real file.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_e_sort_filter_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real sort and filter E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real sort and filter E2E result marker"

echo "✅ ascending sort reordered the data and left the header at row 1"
echo "✅ filter criteria hid the non-matching row and kept the matching one"
echo "✅ clearing the filter put the hidden row back"
echo "✅ filter state and hidden rows read back from the reopened saved copy"
echo "✅ source workbook preserved"

for phase in \
  verify_p15_excel_phase_a \
  verify_p15_excel_phase_b_mutation \
  verify_p15_excel_phase_b_read \
  verify_p15_excel_phase_c_structural \
  verify_p15_excel_phase_d_formatting
do
  bash "verify/$phase.sh" >"$TMP_DIR/$phase.log" 2>&1 || {
    tail -60 "$TMP_DIR/$phase.log"
    fail "$phase regression"
  }
  echo "✅ $phase regression"
done

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
