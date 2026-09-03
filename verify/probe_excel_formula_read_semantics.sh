#!/bin/bash
# Isolated real-Excel probe of formula reading.
#
# The existing spreadsheet.read does one bulk `value of used range` and turns
# the 2D list into TSV. A formula-aware read wants the same shape for formulas,
# and there are three things it cannot be designed without knowing:
#
#   1. does `formula of used range` come back as the same 2D list as `value of
#      used range`, or as something else
#   2. what does `formula` return for a cell holding a CONSTANT -- if it returns
#      the constant, then "has a formula" has to be decided by the leading "="
#   3. does it come back in English on a Chinese-locale Excel, or localized.
#      `formula` and `formula local` are different properties and this machine
#      runs Excel in Chinese, so guessing here would ship a reader that returns
#      function names no consumer can parse.
#
# Each candidate runs as its own AppleScript file and its own osascript
# invocation. Excel is never quit; the workbook count is reported before and
# after so a leak is visible.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-formula-read.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

candidate_body() {
  case "$1" in
    formula_used_range_shape)
      cat <<'BODY'
set usedFormulas to formula of used range of targetSheet
set liveText to "class=" & (class of usedFormulas as text)
if class of usedFormulas is list then
    set liveText to liveText & " rows=" & ((count of usedFormulas) as text)
    set liveText to liveText & " row1class=" & (class of item 1 of usedFormulas as text)
    if class of item 1 of usedFormulas is list then
        set liveText to liveText & " cols=" & ((count of item 1 of usedFormulas) as text)
    end if
end if
BODY
      ;;
    formula_flattened)
      cat <<'BODY'
set usedFormulas to formula of used range of targetSheet
set flatText to ""
repeat with rowValues in usedFormulas
    repeat with cellValue in (contents of rowValues)
        set flatText to flatText & "[" & my safeCell(contents of cellValue) & "]"
    end repeat
    set flatText to flatText & "/"
end repeat
set liveText to flatText
BODY
      ;;
    formula_vs_value_single_cells)
      cat <<'BODY'
set liveText to "B2value=" & my safeCell(value of range "B2" of targetSheet)
set liveText to liveText & " B2formula=" & my safeCell(formula of range "B2" of targetSheet)
set liveText to liveText & " A2value=" & my safeCell(value of range "A2" of targetSheet)
set liveText to liveText & " A2formula=" & my safeCell(formula of range "A2" of targetSheet)
set liveText to liveText & " B4value=" & my safeCell(value of range "B4" of targetSheet)
set liveText to liveText & " B4formula=" & my safeCell(formula of range "B4" of targetSheet)
BODY
      ;;
    formula_local_vs_formula)
      cat <<'BODY'
set liveText to "formula=" & my safeCell(formula of range "B4" of targetSheet)
set liveText to liveText & " local=" & my safeCell(formula local of range "B4" of targetSheet)
set liveText to liveText & " r1c1=" & my safeCell(formula r1c1 of range "B4" of targetSheet)
BODY
      ;;
    formula_empty_cell_outside_used_range)
      cat <<'BODY'
set liveText to "D9formula=[" & my safeCell(formula of range "D9" of targetSheet) & "]"
set liveText to liveText & " usedAddress=" & my safeCell(get address of used range of targetSheet)
BODY
      ;;
  esac
}

write_script() {
  local candidate="$1" path="$2"
  {
    cat <<'HEAD'
on safeCell(cellValue)
    if cellValue is missing value then return ""
    return cellValue as text
end safeCell

on run argv
    set savedPath to item 1 of argv
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
        set value of range "A4" of targetSheet to "Total"
        set formula of range "B4" of targetSheet to "=SUM(B2:B3)"

        try
            with timeout of 45 seconds
HEAD
    candidate_body "$candidate" | sed 's/^/                /'
    cat <<'MID'

                save workbook as probeBook filename savedPath file format Excel XML file format
                set probeBook to active workbook
                close probeBook saving no

                open workbook workbook file name savedPath
                set reopenedBook to active workbook
                set reopenedSheet to active sheet of reopenedBook

                set reopenedText to "B4formula=" & my safeCell(formula of range "B4" of reopenedSheet)
                set reopenedText to reopenedText & " B4value=" & my safeCell(value of range "B4" of reopenedSheet)
                set reopenedText to reopenedText & " B2formula=" & my safeCell(formula of range "B2" of reopenedSheet)

                close reopenedBook saving no
            end timeout
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
MID
  } > "$path"
}

say "=== formula reading: shape, constants, and locale ==="
say "fixture  A1=Name B1=Score / A2=Bravo B2=2 / A3=Alpha B3=1 / A4=Total B4==SUM(B2:B3)"
say "-----"

index=0
for candidate in \
  formula_used_range_shape formula_flattened formula_vs_value_single_cells \
  formula_local_vs_formula formula_empty_cell_outside_used_range
do
  index=$((index + 1))
  script="$WORK_DIR/$candidate.applescript"
  saved="$WORK_DIR/probe-$index.xlsx"
  write_script "$candidate" "$script"

  if ! /usr/bin/osacompile -o "$WORK_DIR/$candidate.scpt" "$script" >"$WORK_DIR/$candidate.compile" 2>&1; then
    printf '%-38s COMPILE FAIL :: %s\n' "$candidate" "$(tr '\n' ' ' <"$WORK_DIR/$candidate.compile")" | tee -a "$LOG"
    continue
  fi

  result="$(/usr/bin/osascript "$script" "$saved" 2>&1)"
  printf '%-38s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "log: $LOG"
