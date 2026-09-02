#!/bin/bash
# Isolated real-Excel probe of chart semantics.
#
# Earlier work established that chart creation is technically possible. That is
# not what a chart operation needs to be written against. The open questions are
# the same two that decided Phase D and Phase E:
#
#   1. which creation, source-setting and chart-type forms actually compile and
#      run
#   2. what can a chart be judged by from the REOPENED SAVED COPY, since that is
#      where the adapter validates
#
# Each candidate is written into its OWN AppleScript file. A shared file was
# tried first and was worthless: `set source data of chart of X to Y` fails to
# compile, because `set source data` is a command in Excel's dictionary, and
# that single mistake took down all ten candidates with a compile error before
# any of them ran.
#
# Fixture, every candidate:
#   A1=Name  B1=Score
#   A2=Bravo B2=2
#   A3=Alpha B3=1
#
# Excel is never quit; the workbook count is reported before and after so a leak
# is visible.
#
# Round 1 settled creation and identity:
#   make new chart object at <sheet>            works
#   name of <chart object>                      settable, survives reopen
#   chart type of chart of <chart object>       readable, survives reopen
#   default chart type                          column clustered
#   set (source data of chart of X) to <range>  FAILS (-10006)
#   set source data chart of X source (<range>) works
#   chart type `line`                           FAILS (-10006)
#   chart type `pie`                            FAILS (-2753, undefined)
#   make new chart at end of <workbook>         FAILS (-50)
#
# Round 2 asks what the line and pie constants are actually called, and whether
# the source data can be SEEN after a reopen -- a chart that exists but plots
# nothing would pass every round 1 check.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-chart.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

# candidate | applescript body | live read expression
candidate_body() {
  case "$1" in
    create_only)
      echo 'make new chart object at targetSheet'
      ;;
    create_with_geometry)
      echo 'make new chart object at targetSheet with properties {width:300, height:200, left position:200, top:20}'
      ;;
    source_command_form)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set source data chart of madeChart source (range "A1:B3" of targetSheet)'
      ;;
    source_command_plotby)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set source data chart of madeChart source (range "A1:B3" of targetSheet) plot by columns'
      ;;
    type_column)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to column clustered'
      ;;
    type_line_chart)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to line chart'
      ;;
    type_line_markers)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to line markers'
      ;;
    type_pie_chart)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to pie chart'
      ;;
    type_pie_exploded)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to pie exploded'
      ;;
    type_bar_clustered)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to bar clustered'
      ;;
    type_xy_scatter)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set chart type of chart of madeChart to xy scatter'
      ;;
    source_then_type)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set source data chart of madeChart source (range "A1:B3" of targetSheet) plot by columns'
      echo 'set chart type of chart of madeChart to column clustered'
      echo 'set name of madeChart to "AiosChart"'
      ;;
    name_readback)
      echo 'set madeChart to make new chart object at targetSheet'
      echo 'set name of madeChart to "AiosChart"'
      ;;
    chart_sheet)
      echo 'make new chart at end of probeBook'
      ;;
  esac
}

candidate_live() {
  case "$1" in
    chart_sheet)
      echo 'set liveText to "charts=" & ((count of charts of probeBook) as text) & " sheets=" & ((count of worksheets of probeBook) as text)'
      ;;
    *)
      echo 'set liveText to "objects=" & ((count of chart objects of targetSheet) as text)'
      ;;
  esac
}

candidate_reopened() {
  case "$1" in
    chart_sheet)
      echo 'set reopenedText to "charts=" & ((count of charts of reopenedBook) as text) & " sheets=" & ((count of worksheets of reopenedBook) as text)'
      ;;
    *)
      cat <<'READ'
set reopenedSheet to worksheet 1 of reopenedBook
set reopenedCount to count of chart objects of reopenedSheet
set reopenedText to "objects=" & (reopenedCount as text)
if reopenedCount > 0 then
    set reopenedChart to chart object 1 of reopenedSheet
    try
        set reopenedText to reopenedText & " name=" & ((name of reopenedChart) as text)
    on error
        set reopenedText to reopenedText & " name=UNREADABLE"
    end try
    try
        set reopenedText to reopenedText & " type=" & ((chart type of chart of reopenedChart) as text)
    on error
        set reopenedText to reopenedText & " type=UNREADABLE"
    end try
    -- A chart that exists but plots nothing would satisfy every check above.
    try
        set seriesCount to count of series of chart of reopenedChart
        set reopenedText to reopenedText & " series=" & (seriesCount as text)
        if seriesCount > 0 then
            try
                set reopenedText to reopenedText & " f1=" & ((formula of series 1 of chart of reopenedChart) as text)
            on error
                set reopenedText to reopenedText & " f1=UNREADABLE"
            end try
        end if
    on error
        set reopenedText to reopenedText & " series=UNREADABLE"
    end try
end if
READ
      ;;
  esac
}

write_script() {
  local candidate="$1" path="$2"
  {
    cat <<'HEAD'
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

        try
HEAD
    candidate_body "$candidate" | sed 's/^/            /'
    candidate_live "$candidate" | sed 's/^/            /'
    cat <<'MID'

            save workbook as probeBook filename savedPath file format Excel XML file format
            set probeBook to active workbook
            close probeBook saving no

            open workbook workbook file name savedPath
            set reopenedBook to active workbook

MID
    candidate_reopened "$candidate" | sed 's/^/            /'
    cat <<'TAIL'

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
TAIL
  } > "$path"
}

say "=== charts: which forms work, and what survives save+reopen? ==="
say "fixture  A1=Name B1=Score / A2=Bravo B2=2 / A3=Alpha B3=1"
say ""
say "the question is not whether a chart can be made -- that is already known --"
say "but what a chart can be JUDGED by after the file is closed and reopened."
say "-----"

index=0
for candidate in \
  create_only source_command_form source_command_plotby \
  type_column type_line_chart type_line_markers \
  type_pie_chart type_pie_exploded type_bar_clustered type_xy_scatter \
  name_readback source_then_type
do
  index=$((index + 1))
  script="$WORK_DIR/$candidate.applescript"
  saved="$WORK_DIR/probe-$index.xlsx"
  write_script "$candidate" "$script"

  if ! /usr/bin/osacompile -o "$WORK_DIR/$candidate.scpt" "$script" >"$WORK_DIR/$candidate.compile" 2>&1; then
    printf '%-24s COMPILE FAIL :: %s\n' "$candidate" "$(tr '\n' ' ' <"$WORK_DIR/$candidate.compile")" | tee -a "$LOG"
    continue
  fi

  result="$(/usr/bin/osascript "$script" "$saved" 2>&1)"
  printf '%-24s %s\n' "$candidate" "$result" | tee -a "$LOG"
done

say ""
say "log: $LOG"
