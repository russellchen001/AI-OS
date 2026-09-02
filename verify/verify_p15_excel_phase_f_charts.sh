#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase F Charts"
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
  document::excel::tests::chart_operations_require_a_known_type_and_an_explicit_name \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "chart input validation"
  }

echo "✅ known chart type, explicit name, bounded source range"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::chart_encoding_carries_both_the_set_and_the_stored_constant \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "chart encoding"
  }

echo "✅ record carries both the set and the stored chart constant"

# verify/probe_excel_chart_semantics.sh established that a chart's name, type
# and series formula all survive save-as-xlsx, close and reopen, and that the
# series formula names the ranges actually plotted. This asserts it on the real
# file.
AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::excel::tests::excel_phase_f_chart_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real chart E2E"
  }

grep -q "test result: ok" "$LOG" ||
  fail "real chart E2E result marker"

echo "✅ column, bar, line and pie charts created and found by name"
echo "✅ each stored chart type matches what Excel actually stores"
echo "✅ each chart plots a real series, proved by its SERIES formula"
echo "✅ chart identity and series read back from the reopened saved copy"
echo "✅ source workbook preserved"

# Earlier phases are NOT run from here. The gate runs every phase once, in
# order; a verifier that also ran its predecessors made the chain quadratic --
# one gate cost 32 real Excel cycles and took half an hour.

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
