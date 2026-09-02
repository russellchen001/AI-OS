#!/bin/bash
# Isolated real-Excel probe of sort and autofilter semantics.
#
# Sort already has a proven form and a scalar read-back. Autofilter has
# neither: applying it returns nothing to compare, so this probe exists to find
# what it CAN be judged by, and whether that evidence survives save-as-xlsx,
# close and reopen -- the adapter validates by reading the reopened saved copy.
#
# Candidates that fail are reported rather than aborting the run, which is the
# point of probing property names that may not exist.
#
# Fixture, every candidate:
#   A1=Name  B1=Score
#   A2=Bravo B2=2
#   A3=Alpha B3=1
#
# Every candidate runs as its own osascript invocation against its own fresh
# workbook. Excel is never quit; the workbook count is reported before and
# after so a leak is visible.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-sort-filter.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

SCRIPT="$(mktemp -t aios_excel_sf)"
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

        set value of range "A1" of targetSheet to "Name"
        set value of range "B1" of targetSheet to "Score"
        set value of range "A2" of targetSheet to "Bravo"
        set value of range "B2" of targetSheet to 2
        set value of range "A3" of targetSheet to "Alpha"
        set value of range "B3" of targetSheet to 1

        try
            if operationKind is "sort_ascending" then
                sort (range "A1:B3" of targetSheet) key1 (range "A1" of targetSheet) order1 sort ascending header header yes
                set liveText to (value of range "A2" of targetSheet) as text
            else if operationKind is "sort_descending" then
                sort (range "A1:B3" of targetSheet) key1 (range "A1" of targetSheet) order1 sort descending header header yes
                set liveText to (value of range "A2" of targetSheet) as text
            else if operationKind is "sort_no_header" then
                sort (range "A2:B3" of targetSheet) key1 (range "A2" of targetSheet) order1 sort ascending header header no
                set liveText to (value of range "A2" of targetSheet) as text
            else if operationKind is "sort_second_key" then
                sort (range "A1:B3" of targetSheet) key1 (range "B1" of targetSheet) order1 sort ascending header header yes
                set liveText to (value of range "A2" of targetSheet) as text
            else if operationKind is "filter_mode_readback" then
                autofilter range (range "A1:B3" of targetSheet)
                set liveText to (autofilter mode of targetSheet) as text
            else if operationKind is "filter_criteria_hidden_row" then
                autofilter range (range "A1:B3" of targetSheet) field 2 criteria1 ">1"
                set liveText to "row3hidden=" & ((hidden of row 3 of targetSheet) as text) & " row2hidden=" & ((hidden of row 2 of targetSheet) as text)
            else if operationKind is "filter_criteria_text" then
                autofilter range (range "A1:B3" of targetSheet) field 1 criteria1 "Alpha"
                set liveText to "row2hidden=" & ((hidden of row 2 of targetSheet) as text) & " row3hidden=" & ((hidden of row 3 of targetSheet) as text)
            else if operationKind is "filter_show_all" then
                autofilter range (range "A1:B3" of targetSheet) field 2 criteria1 ">1"
                show all data targetSheet
                set liveText to "row3hidden=" & ((hidden of row 3 of targetSheet) as text) & " mode=" & ((autofilter mode of targetSheet) as text)
            else if operationKind is "filter_range_readback" then
                autofilter range (range "A1:B3" of targetSheet)
                set liveText to (get address of range of autofilter object of targetSheet) as text
            end if

            save workbook as probeBook filename savedPath file format Excel XML file format
            set probeBook to active workbook
            close probeBook saving no

            open workbook workbook file name savedPath
            set reopenedBook to active workbook
            set reopenedSheet to active sheet of reopenedBook

            if operationKind starts with "sort_" then
                set reopenedText to "A1=" & ((value of range "A1" of reopenedSheet) as text) & " A2=" & ((value of range "A2" of reopenedSheet) as text) & " A3=" & ((value of range "A3" of reopenedSheet) as text)
            else if operationKind is "filter_mode_readback" then
                set reopenedText to (autofilter mode of reopenedSheet) as text
            else if operationKind is "filter_criteria_hidden_row" then
                set reopenedText to "mode=" & ((autofilter mode of reopenedSheet) as text) & " row3hidden=" & ((hidden of row 3 of reopenedSheet) as text)
            else if operationKind is "filter_criteria_text" then
                set reopenedText to "mode=" & ((autofilter mode of reopenedSheet) as text) & " row2hidden=" & ((hidden of row 2 of reopenedSheet) as text)
            else if operationKind is "filter_show_all" then
                set reopenedText to "mode=" & ((autofilter mode of reopenedSheet) as text) & " row3hidden=" & ((hidden of row 3 of reopenedSheet) as text)
            else if operationKind is "filter_range_readback" then
                set reopenedText to (get address of range of autofilter object of reopenedSheet) as text
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

say "=== sort and autofilter: what can each be judged by, and does it survive save+reopen? ==="
say "fixture  A1=Name B1=Score / A2=Bravo B2=2 / A3=Alpha B3=1"
say ""
say "expected sort_ascending            live=Alpha            reopened A2=Alpha A3=Bravo"
say "expected sort_descending           live=Bravo            reopened A2=Bravo A3=Alpha"
say "expected sort_no_header            live=Alpha            (sorts A2:B3 with no header row)"
say "expected sort_second_key           live=Alpha            (Alpha scores 1, Bravo 2)"
say "expected filter_mode_readback      live=true             reopened=true"
say "expected filter_criteria_hidden_row  row3hidden=true row2hidden=false, and the same after reopen"
say "expected filter_criteria_text      row2hidden=true row3hidden=false"
say "expected filter_show_all           row3hidden=false, mode still true"
say "unknown  filter_range_readback     property name may not exist -- that is what this is for"
say "-----"

index=0
for candidate in \
  sort_ascending sort_descending sort_no_header sort_second_key \
  filter_mode_readback filter_criteria_hidden_row filter_criteria_text \
  filter_show_all filter_range_readback
do
  index=$((index + 1))
  saved="$OUT_DIR/probe-$index.xlsx"
  result="$(/usr/bin/osascript "$SCRIPT" "$candidate" "$saved" 2>&1)"
  printf '%-28s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "log: $LOG"
