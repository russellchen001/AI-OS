#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase C Structural"
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

# The adapter's AppleScript is one embedded raw string. Compiling it on its own
# turns a syntax mistake into a one-second failure here instead of a several
# minute failure inside the real E2E.
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
  document::excel::tests::structural_operations_are_parsed_with_bounded_one_based_indexes \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "structural input validation"
  }

echo "✅ bounded 1-based indexes, no count parameter"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::structural_operations_encode_whole_line_references \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "whole-line encoding"
  }

echo "✅ whole-line references, no shift parameter"

# The accepted AppleScript form was chosen from a real Excel probe, not from
# compilation. This is the assertion that the choice actually moves data.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_c_structural_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real structural E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real structural E2E result marker"

echo "✅ insert_column shifted the row right, proved from the reopened copy"
echo "✅ delete_column shifted the row left, proved from the reopened copy"
echo "✅ insert_row shifted the column down, proved from the reopened copy"
echo "✅ delete_row shifted the column up, proved from the reopened copy"
echo "✅ source workbook preserved"

# Phase A and Phase B must still hold: Phase C reuses their adapter.
bash verify/verify_p15_excel_phase_a.sh >"$TMP_DIR/phase_a.log" 2>&1 || {
  tail -60 "$TMP_DIR/phase_a.log"
  fail "Phase A regression"
}
echo "✅ Phase A regression"

bash verify/verify_p15_excel_phase_b_mutation.sh >"$TMP_DIR/phase_b_mutation.log" 2>&1 || {
  tail -60 "$TMP_DIR/phase_b_mutation.log"
  fail "Phase B mutation regression"
}
echo "✅ Phase B mutation regression"

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
