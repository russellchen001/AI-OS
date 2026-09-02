#!/usr/bin/env bash
# Isolated real-Excel probe of whole-row and whole-column structural semantics.
#
# Phase C failed because an AppleScript form that compiled did not shift real
# content the way it was assumed to. Compilation is not evidence, so this probe
# decides between the candidate forms by reading actual cell values back.
#
# It never touches a user workbook: each candidate gets its own new workbook,
# which is closed without saving. Excel is never quit, and the workbook count is
# reported before and after so a leak would be visible.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$ROOT/office-probe-excel-structural.log"
: > "$LOG"

if [ ! -d "/Applications/Microsoft Excel.app" ]; then
  echo "SKIP: Microsoft Excel is not installed" | tee -a "$LOG"
  exit 0
fi

echo "fixture per candidate: A1=R1 B1=C1 C1=C2 / A2=R2 A3=R3" | tee -a "$LOG"
echo "-----" | tee -a "$LOG"

/usr/bin/osascript <<'APPLESCRIPT' 2>&1 | tee -a "$LOG"
on readCell(sheetRef, addr)
    tell application "Microsoft Excel"
        try
            set raw to value of range addr of sheetRef
            if raw is missing value then return ""
            return raw as text
        on error
            return "<unreadable>"
        end try
    end tell
end readCell

on probeOne(label, opKind)
    tell application "Microsoft Excel"
        set countBefore to count of workbooks
        set wb to make new workbook
        set sh to active sheet of wb

        set value of range "A1" of sh to "R1"
        set value of range "B1" of sh to "C1"
        set value of range "C1" of sh to "C2"
        set value of range "A2" of sh to "R2"
        set value of range "A3" of sh to "R3"

        set outcome to "ok"
        try
            if opKind is "insert_col_plain" then
                insert into range (range "B:B" of sh)
            else if opKind is "insert_col_shift" then
                insert into range (range "B:B" of sh) shift shift to right
            else if opKind is "insert_col_entire" then
                insert into range (entire column of range "B1" of sh)
            else if opKind is "delete_col_plain" then
                delete range (range "A:A" of sh)
            else if opKind is "delete_col_shift" then
                delete range (range "A:A" of sh) shift shift to left
            else if opKind is "delete_col_entire" then
                delete range (entire column of range "A1" of sh)
            else if opKind is "insert_row_plain" then
                insert into range (range "2:2" of sh)
            else if opKind is "insert_row_shift" then
                insert into range (range "2:2" of sh) shift shift down
            else if opKind is "insert_row_entire" then
                insert into range (entire row of range "A2" of sh)
            else if opKind is "delete_row_plain" then
                delete range (range "1:1" of sh)
            else if opKind is "delete_row_shift" then
                delete range (range "1:1" of sh) shift shift up
            else if opKind is "delete_row_entire" then
                delete range (entire row of range "A1" of sh)
            end if
        on error errText number errNum
            set outcome to "ERROR " & errNum & " :: " & errText
        end try

        set a1 to my readCell(sh, "A1")
        set b1 to my readCell(sh, "B1")
        set c1 to my readCell(sh, "C1")
        set d1 to my readCell(sh, "D1")
        set a2 to my readCell(sh, "A2")
        set a3 to my readCell(sh, "A3")

        close wb saving no
        set countAfter to count of workbooks

        return label & " :: " & outcome & " :: A1=[" & a1 & "] B1=[" & b1 & "] C1=[" & c1 & "] D1=[" & d1 & "] A2=[" & a2 & "] A3=[" & a3 & "] :: workbooks " & (countBefore as text) & "->" & (countAfter as text)
    end tell
end probeOne

set candidates to {¬
    {"insert col B  plain        ", "insert_col_plain"}, ¬
    {"insert col B  shift right  ", "insert_col_shift"}, ¬
    {"insert col B  entire column", "insert_col_entire"}, ¬
    {"delete col A  plain        ", "delete_col_plain"}, ¬
    {"delete col A  shift left   ", "delete_col_shift"}, ¬
    {"delete col A  entire column", "delete_col_entire"}, ¬
    {"insert row 2  plain        ", "insert_row_plain"}, ¬
    {"insert row 2  shift down   ", "insert_row_shift"}, ¬
    {"insert row 2  entire row   ", "insert_row_entire"}, ¬
    {"delete row 1  plain        ", "delete_row_plain"}, ¬
    {"delete row 1  shift up     ", "delete_row_shift"}, ¬
    {"delete row 1  entire row   ", "delete_row_entire"}}

set report to ""
repeat with entry in candidates
    set label to item 1 of entry
    set opKind to item 2 of entry
    try
        set line to my probeOne(label, opKind)
    on error errText number errNum
        set line to label & " :: FATAL " & errNum & " :: " & errText
    end try
    set report to report & line & linefeed
end repeat

return report
APPLESCRIPT

echo | tee -a "$LOG"
echo "=== formatting / sort / filter primitives (next Excel items) ===" | tee -a "$LOG"

/usr/bin/osascript <<'APPLESCRIPT2' 2>&1 | tee -a "$LOG"
on tryOne(label, opKind)
    tell application "Microsoft Excel"
        set wb to make new workbook
        set sh to active sheet of wb

        set value of range "A1" of sh to "Name"
        set value of range "B1" of sh to "Score"
        set value of range "A2" of sh to "Bravo"
        set value of range "B2" of sh to 2
        set value of range "A3" of sh to "Alpha"
        set value of range "B3" of sh to 1

        set outcome to "ok"
        set observed to ""
        try
            if opKind is "number_format" then
                set number format of range "B2" of sh to "0.00"
                set observed to (number format of range "B2" of sh) as text
            else if opKind is "bold" then
                set bold of font object of range "A1" of sh to true
                set observed to (bold of font object of range "A1" of sh) as text
            else if opKind is "interior_color" then
                set color of interior object of range "A1" of sh to {255, 0, 0}
                set observed to "set"
            else if opKind is "color_index" then
                set color index of interior object of range "A1" of sh to 3
                set observed to (color index of interior object of range "A1" of sh) as text
            else if opKind is "column_width" then
                set column width of column 1 of sh to 24
                set observed to (column width of column 1 of sh) as text
            else if opKind is "row_height" then
                set row height of row 1 of sh to 30
                set observed to (row height of row 1 of sh) as text
            else if opKind is "sort_range" then
                sort (range "A1:B3" of sh) key1 (range "A1" of sh) order1 sort ascending header header yes
                set observed to (value of range "A2" of sh) as text
            else if opKind is "autofilter" then
                autofilter range (range "A1:B3" of sh)
                set observed to "applied"
            else if opKind is "freeze_header" then
                set observed to "skipped: needs an active window"
            end if
        on error errText number errNum
            set outcome to "ERROR " & errNum & " :: " & errText
        end try

        close wb saving no
        return label & " :: " & outcome & " :: observed=[" & observed & "]"
    end tell
end tryOne

set candidates to {¬
    {"number format      ", "number_format"}, ¬
    {"bold font          ", "bold"}, ¬
    {"interior color rgb ", "interior_color"}, ¬
    {"interior color idx ", "color_index"}, ¬
    {"column width       ", "column_width"}, ¬
    {"row height         ", "row_height"}, ¬
    {"sort by column A   ", "sort_range"}, ¬
    {"autofilter         ", "autofilter"}}

set report to ""
repeat with entry in candidates
    try
        set line to my tryOne(item 1 of entry, item 2 of entry)
    on error errText number errNum
        set line to (item 1 of entry) & " :: FATAL " & errNum & " :: " & errText
    end try
    set report to report & line & linefeed
end repeat
return report
APPLESCRIPT2

echo "-----" | tee -a "$LOG"
echo "sort expectation: after sorting by column A ascending, A2 becomes Alpha" | tee -a "$LOG"
echo "expected after 'insert col B' : A1=R1 B1=[] C1=C1 D1=C2" | tee -a "$LOG"
echo "expected after 'delete col A' : A1=C1 B1=C2 C1=[]" | tee -a "$LOG"
echo "expected after 'insert row 2' : A1=R1 A2=[] A3=R2" | tee -a "$LOG"
echo "expected after 'delete row 1' : A1=R2 A2=R3" | tee -a "$LOG"
echo | tee -a "$LOG"
echo "written to: $LOG" | tee -a "$LOG"
