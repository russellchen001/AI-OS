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

  result=$(/usr/bin/osascript - "$source" "$output" "$content" <<'APPLESCRIPT'
on excelColumnName(columnNumber)
    set letters to "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    set resultText to ""
    set remaining to columnNumber
    repeat while remaining > 0
        set letterIndex to ((remaining - 1) mod 26) + 1
        set resultText to character letterIndex of letters & resultText
        set remaining to (remaining - letterIndex) div 26
    end repeat
    return resultText
end excelColumnName

on valuesMatch(actualValue, expectedValue)
    try
        set expectedNumber to expectedValue as number
        return actualValue = expectedNumber
    on error
        return (actualValue as text) = expectedValue
    end try
end valuesMatch

on run argv
    set outputPath to item 2 of argv
    set expectedContent to item 3 of argv
    set openedWorkbook to missing value
    set currentPhase to "create"

    tell application "Microsoft Excel"
        try
            set openedWorkbook to make new workbook
            set currentPhase to "write"
            set oldDelimiters to AppleScript's text item delimiters
            set AppleScript's text item delimiters to linefeed
            set sourceRows to text items of expectedContent
            repeat with rowIndex from 1 to count of sourceRows
                set sourceRow to item rowIndex of sourceRows
                set AppleScript's text item delimiters to tab
                set rowValues to text items of sourceRow
                repeat with columnIndex from 1 to count of rowValues
                    set cellAddress to my excelColumnName(columnIndex) & rowIndex
                    set value of range cellAddress of worksheet 1 of openedWorkbook to contents of item columnIndex of rowValues
                end repeat
            end repeat
            set AppleScript's text item delimiters to oldDelimiters
            set currentPhase to "save"
            save workbook as openedWorkbook filename outputPath ¬
                file format Excel XML file format
            set currentPhase to "close"
            try
                close openedWorkbook saving no
            end try
            set openedWorkbook to missing value
            open workbook workbook file name outputPath
            repeat 20 times
                repeat with workbookIndex from 1 to count of workbooks
                    set candidateWorkbook to workbook workbookIndex
                    set candidatePath to full name of candidateWorkbook
                    if candidatePath is outputPath then
                        set openedWorkbook to candidateWorkbook
                        exit repeat
                    end if
                end repeat
                if openedWorkbook is not missing value then exit repeat
                delay 0.25
            end repeat
            if openedWorkbook is missing value then error "Excel did not reopen the generated workbook"
            set usedValues to value of used range of worksheet 1 of openedWorkbook
            set contentMatches to (count of usedValues) = (count of sourceRows)
            repeat with rowIndex from 1 to count of sourceRows
                if rowIndex > count of usedValues then
                    set contentMatches to false
                    exit repeat
                end if
                set AppleScript's text item delimiters to tab
                set expectedRow to text items of item rowIndex of sourceRows
                set actualRow to item rowIndex of usedValues
                if (count of actualRow) is not (count of expectedRow) then set contentMatches to false
                repeat with columnIndex from 1 to count of expectedRow
                    if columnIndex > count of actualRow or not my valuesMatch(contents of item columnIndex of actualRow, contents of item columnIndex of expectedRow) then
                        set contentMatches to false
                        exit repeat
                    end if
                end repeat
            end repeat
            set AppleScript's text item delimiters to oldDelimiters
            close openedWorkbook saving no
            set openedWorkbook to missing value
            if contentMatches then return "AIOS_EXCEL_SAVED"
            return "AIOS_CONTENT_MISMATCH"
        on error errorMessage number errorNumber
            if openedWorkbook is not missing value then
                try
                    close openedWorkbook saving no
                end try
            end if
            return "AIOS_FAILED=" & currentPhase & ":" & errorNumber & ":" & errorMessage
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
CREATE_RESULT=""
for attempt in 1 2 3; do
  CREATE_RESULT="$(create_xlsx "$TARGET" "$CONTENT")"
  [[ "$CREATE_RESULT" == AIOS_SPREADSHEET_CREATED=* ]] && break
  sleep 1
done

[[ "$CREATE_RESULT" == AIOS_SPREADSHEET_CREATED=* ]] ||
  fail "Real XLSX creation"
[ -s "$TARGET" ] || fail "Created XLSX missing"

BEFORE_HASH="$(shasum -a 256 "$TARGET" | cut -d' ' -f1)"
EXISTS_RESULT="$(create_xlsx "$TARGET" $'Changed\tValue\nBeta\t99')"
AFTER_HASH="$(shasum -a 256 "$TARGET" | cut -d' ' -f1)"

[ "$EXISTS_RESULT" = "AIOS_EXISTS" ] || fail "Existing target detection"
[ "$BEFORE_HASH" = "$AFTER_HASH" ] || fail "Existing target was overwritten"

echo "✓ Real XLSX create, read and no-overwrite"

if [ "${KEEP_FIXTURE:-0}" = "1" ]; then
  case "${KEEP_FIXTURE_PATH:-}" in
    /*) ;;
    *) fail "KEEP_FIXTURE_PATH must be an absolute path" ;;
  esac
  [ ! -e "$KEEP_FIXTURE_PATH" ] || fail "Preserved fixture target already exists"
  cp "$TARGET" "$KEEP_FIXTURE_PATH" || fail "Preserved fixture copy"
  [ -s "$KEEP_FIXTURE_PATH" ] || fail "Preserved fixture missing"
  echo "✓ Preserved validated fixture: $KEEP_FIXTURE_PATH"
fi

echo "PASS $NAME"
exit 0
