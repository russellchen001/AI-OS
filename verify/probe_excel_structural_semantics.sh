#!/usr/bin/env bash
# Isolated real-Excel probe of whole-row, whole-column and formatting semantics.
#
# Phase C failed because an AppleScript form that compiled did not shift real
# content the way it was assumed to. Compilation is not evidence, so every
# candidate here is judged by reading actual cell values back.
#
# Each candidate runs as its own osascript invocation, so a mistake in one
# cannot stop the others. Each gets its own new workbook, closed without
# saving: no user workbook is touched, Excel is never quit, and the workbook
# count is reported before and after so a leak would be visible.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$ROOT/office-probe-excel-structural.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

if [ ! -d "/Applications/Microsoft Excel.app" ]; then
  say "SKIP: Microsoft Excel is not installed"
  exit 0
fi

STRUCTURAL_SCRIPT="$(mktemp -t aios_excel_structural)"
FORMAT_SCRIPT="$(mktemp -t aios_excel_format)"
trap 'rm -f "$STRUCTURAL_SCRIPT" "$FORMAT_SCRIPT"' EXIT

cat > "$STRUCTURAL_SCRIPT" <<'APPLESCRIPT'
on cellText(sheetReference, cellAddress)
    tell application "Microsoft Excel"
        try
            set rawValue to value of range cellAddress of sheetReference
            if rawValue is missing value then return ""
            return rawValue as text
        on error
            return "<unreadable>"
        end try
    end tell
end cellText

on run argv
    set operationKind to item 1 of argv
    set outcomeText to "ok"

    tell application "Microsoft Excel"
        set booksBefore to count of workbooks
        set probeBook to make new workbook
        set targetSheet to active sheet of probeBook

        set value of range "A1" of targetSheet to "R1"
        set value of range "B1" of targetSheet to "C1"
        set value of range "C1" of targetSheet to "C2"
        set value of range "A2" of targetSheet to "R2"
        set value of range "A3" of targetSheet to "R3"

        try
            if operationKind is "insert_col_plain" then
                insert into range (range "B:B" of targetSheet)
            else if operationKind is "insert_col_shift" then
                insert into range (range "B:B" of targetSheet) shift shift to right
            else if operationKind is "insert_col_entire" then
                insert into range (entire column of range "B1" of targetSheet)
            else if operationKind is "delete_col_plain" then
                delete range (range "A:A" of targetSheet)
            else if operationKind is "delete_col_shift" then
                delete range (range "A:A" of targetSheet) shift shift to left
            else if operationKind is "delete_col_entire" then
                delete range (entire column of range "A1" of targetSheet)
            else if operationKind is "insert_row_plain" then
                insert into range (range "2:2" of targetSheet)
            else if operationKind is "insert_row_shift" then
                insert into range (range "2:2" of targetSheet) shift shift down
            else if operationKind is "insert_row_entire" then
                insert into range (entire row of range "A2" of targetSheet)
            else if operationKind is "delete_row_plain" then
                delete range (range "1:1" of targetSheet)
            else if operationKind is "delete_row_shift" then
                delete range (range "1:1" of targetSheet) shift shift up
            else if operationKind is "delete_row_entire" then
                delete range (entire row of range "A1" of targetSheet)
            end if
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
        end try

        set readA1 to my cellText(targetSheet, "A1")
        set readB1 to my cellText(targetSheet, "B1")
        set readC1 to my cellText(targetSheet, "C1")
        set readD1 to my cellText(targetSheet, "D1")
        set readA2 to my cellText(targetSheet, "A2")
        set readA3 to my cellText(targetSheet, "A3")

        close probeBook saving no
        set booksAfter to count of workbooks
    end tell

    return outcomeText & " :: A1=[" & readA1 & "] B1=[" & readB1 & "] C1=[" & readC1 & "] D1=[" & readD1 & "] A2=[" & readA2 & "] A3=[" & readA3 & "] :: books " & (booksBefore as text) & "->" & (booksAfter as text)
end run
APPLESCRIPT

cat > "$FORMAT_SCRIPT" <<'APPLESCRIPT'
on run argv
    set operationKind to item 1 of argv
    set outcomeText to "ok"
    set observedText to ""

    tell application "Microsoft Excel"
        set booksBefore to count of workbooks
        set probeBook to make new workbook
        set targetSheet to active sheet of probeBook

        set value of range "A1" of targetSheet to "Name"
        set value of range "B1" of targetSheet to "Score"
        set value of range "A2" of targetSheet to "Bravo"
        set value of range "B2" of targetSheet to 2
        set value of range "A3" of targetSheet to "Alpha"
        set value of range "B3" of targetSheet to 1

        try
            if operationKind is "number_format" then
                set number format of range "B2" of targetSheet to "0.00"
                set observedText to (number format of range "B2" of targetSheet) as text
            else if operationKind is "bold_font" then
                set bold of font object of range "A1" of targetSheet to true
                set observedText to (bold of font object of range "A1" of targetSheet) as text
            else if operationKind is "interior_rgb" then
                set color of interior object of range "A1" of targetSheet to {255, 0, 0}
                set observedText to "applied"
            else if operationKind is "interior_index" then
                set color index of interior object of range "A1" of targetSheet to 3
                set observedText to (color index of interior object of range "A1" of targetSheet) as text
            else if operationKind is "column_width" then
                set column width of column 1 of targetSheet to 24
                set observedText to (column width of column 1 of targetSheet) as text
            else if operationKind is "row_height" then
                set row height of row 1 of targetSheet to 30
                set observedText to (row height of row 1 of targetSheet) as text
            else if operationKind is "sort_ascending" then
                sort (range "A1:B3" of targetSheet) key1 (range "A1" of targetSheet) order1 sort ascending header header yes
                set observedText to (value of range "A2" of targetSheet) as text
            else if operationKind is "autofilter_apply" then
                autofilter range (range "A1:B3" of targetSheet)
                set observedText to "applied"
            end if
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
        end try

        close probeBook saving no
        set booksAfter to count of workbooks
    end tell

    return outcomeText & " :: observed=[" & observedText & "] :: books " & (booksBefore as text) & "->" & (booksAfter as text)
end run
APPLESCRIPT

say "fixture: A1=R1 B1=C1 C1=C2 / A2=R2 A3=R3"
say "expected  insert col B : A1=R1 B1=[]   C1=C1 D1=C2"
say "expected  delete col A : A1=C1 B1=C2   C1=[]"
say "expected  insert row 2 : A1=R1 A2=[]   A3=R2"
say "expected  delete row 1 : A1=R2 A2=R3"
say "-----"

for candidate in \
  insert_col_plain insert_col_shift insert_col_entire \
  delete_col_plain delete_col_shift delete_col_entire \
  insert_row_plain insert_row_shift insert_row_entire \
  delete_row_plain delete_row_shift delete_row_entire
do
  result="$(/usr/bin/osascript "$STRUCTURAL_SCRIPT" "$candidate" 2>&1)"
  printf '%-22s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "=== formatting / sort / filter primitives ==="
say "sort expectation: sorting by column A ascending puts Alpha in A2"
say "-----"

for candidate in \
  number_format bold_font interior_rgb interior_index \
  column_width row_height sort_ascending autofilter_apply
do
  result="$(/usr/bin/osascript "$FORMAT_SCRIPT" "$candidate" 2>&1)"
  printf '%-22s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "written to: $LOG"
