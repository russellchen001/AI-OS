#!/bin/bash
# Isolated real-Excel probe of formatting semantics.
#
# Two questions the formatting phase cannot be written without answering:
#
#   1. which property names actually exist (italic and font size were never
#      probed; bold, number format, colour index, column width and row height
#      were)
#   2. whether each attribute SURVIVES save-as-xlsx, close and reopen -- the
#      adapter validates by reading the reopened saved copy, so an attribute
#      that is correct live but lost on save cannot be validated that way
#
# Every candidate runs as its own osascript invocation against its own fresh
# workbook, which is closed without saving. Excel is never quit, and the
# workbook count is reported before and after so a leak is visible.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-formatting.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

SCRIPT="$(mktemp -t aios_excel_fmt)"
OUT_DIR="$(mktemp -d)"
trap 'rm -f "$SCRIPT"; rm -rf "$OUT_DIR"' EXIT

cat > "$SCRIPT" <<'APPLESCRIPT'
on run argv
    set operationKind to item 1 of argv
    set savedPath to item 2 of argv
    set outcomeText to "ok"
    set liveText to ""
    set reopenedText to ""

    tell application "Microsoft Excel"
        set booksBefore to count of workbooks
        set probeBook to make new workbook
        set targetSheet to active sheet of probeBook

        set value of range "A1" of targetSheet to "Header"
        set value of range "B1" of targetSheet to 1234.5
        set value of range "A2" of targetSheet to "Body"
        set value of range "B2" of targetSheet to 2

        try
            if operationKind is "bold" then
                set bold of font object of range "A1:B1" of targetSheet to true
                set liveText to (bold of font object of range "A1" of targetSheet) as text
            else if operationKind is "italic" then
                set italic of font object of range "A1:B1" of targetSheet to true
                set liveText to (italic of font object of range "A1" of targetSheet) as text
            else if operationKind is "font_size" then
                set font size of font object of range "A1:B1" of targetSheet to 18
                set liveText to (font size of font object of range "A1" of targetSheet) as text
            else if operationKind is "font_name" then
                set name of font object of range "A1:B1" of targetSheet to "Courier New"
                set liveText to (name of font object of range "A1" of targetSheet) as text
            else if operationKind is "number_format" then
                set number format of range "B1:B2" of targetSheet to "0.00"
                set liveText to (number format of range "B1" of targetSheet) as text
            else if operationKind is "fill_index" then
                set color index of interior object of range "A1:B1" of targetSheet to 6
                set liveText to (color index of interior object of range "A1" of targetSheet) as text
            else if operationKind is "column_width" then
                set column width of column 1 of targetSheet to 24
                set liveText to (column width of column 1 of targetSheet) as text
            else if operationKind is "row_height" then
                set row height of row 1 of targetSheet to 30
                set liveText to (row height of row 1 of targetSheet) as text
            end if

            -- The real question: does it survive a round trip through the file?
            save workbook as probeBook filename savedPath file format Excel XML file format
            set probeBook to active workbook
            close probeBook saving no

            open workbook workbook file name savedPath
            set reopenedBook to active workbook
            set reopenedSheet to active sheet of reopenedBook

            if operationKind is "bold" then
                set reopenedText to (bold of font object of range "A1" of reopenedSheet) as text
            else if operationKind is "italic" then
                set reopenedText to (italic of font object of range "A1" of reopenedSheet) as text
            else if operationKind is "font_size" then
                set reopenedText to (font size of font object of range "A1" of reopenedSheet) as text
            else if operationKind is "font_name" then
                set reopenedText to (name of font object of range "A1" of reopenedSheet) as text
            else if operationKind is "number_format" then
                set reopenedText to (number format of range "B1" of reopenedSheet) as text
            else if operationKind is "fill_index" then
                set reopenedText to (color index of interior object of range "A1" of reopenedSheet) as text
            else if operationKind is "column_width" then
                set reopenedText to (column width of column 1 of reopenedSheet) as text
            else if operationKind is "row_height" then
                set reopenedText to (row height of row 1 of reopenedSheet) as text
            end if

            close reopenedBook saving no
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
            try
                close (every workbook whose full name is savedPath) saving no
            end try
            try
                if probeBook exists then close probeBook saving no
            end try
        end try

        set booksAfter to count of workbooks
    end tell

    return outcomeText & " :: live=[" & liveText & "] reopened=[" & reopenedText & "] :: books " & (booksBefore as text) & "->" & (booksAfter as text)
end run
APPLESCRIPT

say "=== formatting attributes: does each one exist, and does it survive save+reopen? ==="
say "expected  bold          live=true      reopened=true"
say "expected  italic        live=true      reopened=true"
say "expected  font_size     live=18.0      reopened=18.0"
say "expected  font_name     live=Courier New"
say "expected  number_format live=0.00      reopened=0.00"
say "expected  fill_index    live=6         reopened=6"
say "expected  column_width  live=24.0      reopened=24.0"
say "expected  row_height    live=30.0      reopened=30.0"
say "-----"

index=0
for candidate in bold italic font_size font_name number_format fill_index column_width row_height
do
  index=$((index + 1))
  saved="$OUT_DIR/probe-$index.xlsx"
  result="$(/usr/bin/osascript "$SCRIPT" "$candidate" "$saved" 2>&1)"
  printf '%-16s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "log: $LOG"
