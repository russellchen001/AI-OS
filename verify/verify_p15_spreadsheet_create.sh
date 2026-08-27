#!/bin/bash
set -u

NAME="P15-3 Spreadsheet Create"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
EXCEL_CACHE="$HOME/Library/Containers/com.microsoft.Excel/Data/Library/Caches/com.microsoft.Excel"
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

create_xlsx() {
  local target="$1" content="$2" excel_tmp source output result size

  if [ -e "$target" ]; then
    echo "AIOS_EXISTS"
    return
  fi
  [ -d "$EXCEL_CACHE" ] || {
    echo "AIOS_FAILED"
    return
  }

  excel_tmp="$(mktemp -d "$EXCEL_CACHE/ai-os-spreadsheet.XXXXXX")" || {
    echo "AIOS_FAILED"
    return
  }
  source="$excel_tmp/source.tsv"
  output="$excel_tmp/output.xlsx"

  printf '%s' "$content" >"$source" || {
    rm -rf "$excel_tmp"
    echo "AIOS_FAILED"
    return
  }

  result=$(/usr/bin/osascript - "$source" "$output" <<'APPLESCRIPT'
on run argv
    set sourcePath to item 1 of argv
    set outputPath to item 2 of argv
    set openedWorkbook to missing value

    tell application "Microsoft Excel"
        try
            open workbook workbook file name sourcePath
            set openedWorkbook to active workbook
            save workbook as openedWorkbook filename outputPath ¬
                file format Excel XML file format
            close openedWorkbook saving no
            return "AIOS_EXCEL_SAVED"
        on error
            if openedWorkbook is not missing value then
                try
                    close openedWorkbook saving no
                end try
            end if
            return "AIOS_FAILED"
        end try
    end tell
end run
APPLESCRIPT
  )

  if [ "$result" = "AIOS_EXCEL_SAVED" ] && [ -f "$output" ]; then
    /bin/mv -n "$output" "$target"
    if [ ! -e "$output" ] && [ -f "$target" ]; then
      size="$(/usr/bin/stat -f %z -- "$target")" || {
        rm -rf "$excel_tmp"
        echo "AIOS_FAILED"
        return
      }
      rm -rf "$excel_tmp"
      echo "AIOS_SPREADSHEET_CREATED=$size"
      return
    fi
  fi

  rm -rf "$excel_tmp"
  [ -e "$target" ] && echo "AIOS_EXISTS" || echo "AIOS_FAILED"
}

echo "$NAME"

cargo check --manifest-path src-tauri/Cargo.toml >"$TMP_DIR/check.log" 2>&1 ||
  { tail -30 "$TMP_DIR/check.log"; fail "Rust compile"; }
echo "✓ Rust compile"

run_test \
  "runtime::skills::registry::tests::spreadsheet_capabilities_resolve_to_office_skill" \
  1 "Skill Registry"

run_test \
  "runtime::openclaw_permission::tests::spreadsheet_create_requires_and_accepts_one_time_user_confirmation" \
  1 "Permission confirmation"

run_test \
  "document::registry::tests::spreadsheet_create_uses_first_available_office_provider" \
  1 "Office Provider routing"

run_test \
  "runtime::openclaw_gateway_adapter::tests::spreadsheet_create_" \
  4 "Input, Excel helper and Gateway behavior"

TARGET="$TMP_DIR/created.xlsx"
CONTENT=$'Name\tValue\nAlpha\t42'
CREATE_RESULT="$(create_xlsx "$TARGET" "$CONTENT")"

[[ "$CREATE_RESULT" == AIOS_SPREADSHEET_CREATED=* ]] ||
  fail "Real XLSX creation"
[ -s "$TARGET" ] || fail "Created XLSX missing"

BEFORE_HASH="$(shasum -a 256 "$TARGET" | cut -d' ' -f1)"
EXISTS_RESULT="$(create_xlsx "$TARGET" $'Changed\tValue\nBeta\t99')"
AFTER_HASH="$(shasum -a 256 "$TARGET" | cut -d' ' -f1)"

[ "$EXISTS_RESULT" = "AIOS_EXISTS" ] || fail "Existing target detection"
[ "$BEFORE_HASH" = "$AFTER_HASH" ] || fail "Existing target was overwritten"

/usr/bin/osascript - "$TARGET" <<'APPLESCRIPT' >"$TMP_DIR/readback.txt"
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
                set usedValues to value of used range
            end tell

            set outputText to ""
            repeat with rowValues in usedValues
                set outputText to outputText & my joinRow(contents of rowValues) & linefeed
            end repeat

            close openedWorkbook saving no
            return outputText
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

READBACK="$(<"$TMP_DIR/readback.txt")"
[[ "$READBACK" == *$'Name\tValue\nAlpha\t42.0'* ]] ||
  fail "Created XLSX content mismatch"

echo "✓ Real XLSX create, read and no-overwrite"
echo "PASS $NAME"
exit 0
