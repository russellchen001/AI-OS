#!/bin/bash
set -u

NAME="P15-3 Spreadsheet Read"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$ROOT/verify/fixtures/p15-spreadsheet-read.xlsx"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
cd "$ROOT" || exit 1

fail() {
  echo "✗ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

run_test() {
  local filter="$1" expected="$2" label="$3" log="$TMP_DIR/test.log"
  cargo test --manifest-path src-tauri/Cargo.toml "$filter" --lib >"$log" 2>&1 ||
    { tail -30 "$log"; fail "$label"; }
  grep -q "running $expected test" "$log" ||
    { tail -30 "$log"; fail "$label did not run"; }
  echo "✓ $label"
}

echo "$NAME"

cargo check --manifest-path src-tauri/Cargo.toml >"$TMP_DIR/check.log" 2>&1 ||
  { tail -30 "$TMP_DIR/check.log"; fail "Rust compile"; }
echo "✓ Rust compile"

run_test \
  "runtime::skills::registry::tests::spreadsheet_capabilities_resolve_to_office_skill" \
  1 "Skill Registry"

run_test \
  "runtime::openclaw_permission::tests::spreadsheet_read_requires_and_accepts_one_time_user_confirmation" \
  1 "Permission confirmation"

run_test \
  "document::registry::tests::spreadsheet_read_uses_first_available_office_provider" \
  1 "Office Provider routing"

run_test \
  "runtime::openclaw_gateway_adapter::tests::spreadsheet_read_" \
  3 "Input, Excel helper and Gateway behavior"

[ -s "$FIXTURE" ] || fail "Real XLSX fixture missing"

/usr/bin/osascript - "$FIXTURE" <<'APPLESCRIPT' | /usr/bin/head -c 65536 >"$TMP_DIR/result.txt"
on joinRow(rowValues)
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to tab
    set rowText to rowValues as text
    set AppleScript's text item delimiters to oldDelimiters
    return rowText
end joinRow

on run argv
    set workbookPath to item 1 of argv
    set openedWorkbook to missing value
    tell application "Microsoft Excel"
        try
            open workbook workbook file name workbookPath
            set openedWorkbook to active workbook
            tell worksheet 1 of openedWorkbook
                set sheetName to name
                set usedValues to value of used range
            end tell
            set rowCount to count of usedValues
            set columnCount to count of item 1 of usedValues
            set outputText to ""
            repeat with rowValues in usedValues
                set outputText to outputText & my joinRow(contents of rowValues) & linefeed
            end repeat
            close openedWorkbook saving no
            return "AIOS_SHEET=" & sheetName & linefeed & ¬
                "AIOS_ROWS=" & rowCount & linefeed & ¬
                "AIOS_COLUMNS=" & columnCount & linefeed & ¬
                "AIOS_CONTENT_BEGIN" & linefeed & outputText
        on error errorMessage number errorNumber
            if openedWorkbook is not missing value then
                try
                    close openedWorkbook saving no
                end try
            end if
            return "AIOS_FAILED=" & errorNumber & ":" & errorMessage
        end try
    end tell
end run
APPLESCRIPT
RESULT="$(<"$TMP_DIR/result.txt")"

[[ "$RESULT" == AIOS_SHEET=* ]] || fail "Real Excel read"
[[ "$RESULT" == *$'AIOS_ROWS=2\nAIOS_COLUMNS=2'* ]] ||
  fail "Real Excel dimensions"
[[ "$RESULT" == *$'Name\tValue\nAlpha\t42.0'* ]] ||
  fail "Real Excel TSV content"
echo "✓ Real XLSX opened, read and closed without saving"

echo "PASS $NAME"
exit 0
