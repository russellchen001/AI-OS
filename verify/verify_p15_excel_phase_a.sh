#!/bin/bash
set -u

NAME="P15 Microsoft Excel Phase A"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
FIXTURE="$TMP_DIR/created.xlsx"
trap 'rm -rf "$TMP_DIR"' EXIT
cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

run_test() {
  local filter="$1" expected="$2" label="$3" extra="${4:-}" log="$TMP_DIR/test.log"
  if [ "$extra" = "ignored" ]; then
    AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" cargo test --manifest-path src-tauri/Cargo.toml "$filter" --lib -- --ignored >"$log" 2>&1 || {
      tail -30 "$log"
      fail "$label"
    }
  else
    cargo test --manifest-path src-tauri/Cargo.toml "$filter" --lib >"$log" 2>&1 || {
      tail -30 "$log"
      fail "$label"
    }
  fi
  grep -q "running $expected test" "$log" || {
    tail -30 "$log"
    fail "$label did not run"
  }
  echo "✅ $label"
}

echo "$NAME"

BEFORE_STATE="$(/usr/bin/osascript -e 'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)')" ||
  fail "Excel preflight state"

KEEP_FIXTURE=1 KEEP_FIXTURE_PATH="$FIXTURE" \
  bash verify/verify_p15_spreadsheet_create.sh >"$TMP_DIR/create.log" 2>&1 || {
    tail -30 "$TMP_DIR/create.log"
    fail "Spreadsheet Create preserved fixture"
  }
grep -q "PASS P15-3 Spreadsheet Create" "$TMP_DIR/create.log" ||
  fail "Spreadsheet Create verifier did not pass"
[ -s "$FIXTURE" ] || fail "Preserved XLSX fixture"
echo "✅ Existing Spreadsheet Create preserved a real XLSX"

run_test "document::excel::tests::" 17 "Excel Phase A through Phase E validation tests"
run_test "spreadsheet_edit_" 3 "Permission and Office Provider routing"
run_test "document::excel::tests::excel_phase_a_real_e2e" 1 "Combined value, formula, clear, save-copy and read-back" ignored
run_test "runtime::openclaw_gateway_adapter::tests::spreadsheet_edit_runtime_real_e2e" 1 "Provider-neutral Runtime spreadsheet.edit dispatch" ignored

AFTER_STATE="$(/usr/bin/osascript -e 'tell application "Microsoft Excel" to return (count of workbooks as text) & "|" & (count of windows as text) & "|" & (name of every workbook as text)')" ||
  fail "Excel final state"
[ "$BEFORE_STATE" = "$AFTER_STATE" ] ||
  fail "Excel workbook/window state was not restored"
echo "✅ Excel remained running and user-owned workbooks were preserved"

echo "PASS $NAME"
exit 0
