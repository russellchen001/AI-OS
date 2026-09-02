#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase D Formatting"
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
  document::excel::tests::formatting_operations_validate_ranges_and_attributes \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "formatting input validation"
  }

echo "✅ bounded ranges, bounded attributes, no-op format refused"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::formatting_encodes_fixed_arity_fields_with_a_terminator \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "formatting encoding"
  }

echo "✅ fixed-arity attribute fields with a terminator"

# Every attribute here was proved by verify/probe_excel_formatting_semantics.sh
# to survive save-as-xlsx, close and reopen. This asserts it on the real file.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_d_formatting_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real formatting E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real formatting E2E result marker"

echo "✅ bold, italic, size, font and fill applied to a header range"
echo "✅ number format applied to a data cell and only to it"
echo "✅ column width and row height applied"
echo "✅ every attribute read back from the reopened saved copy"
echo "✅ source workbook preserved"

for phase in \
  verify_p15_excel_phase_a \
  verify_p15_excel_phase_b_mutation \
  verify_p15_excel_phase_b_read \
  verify_p15_excel_phase_c_structural
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
