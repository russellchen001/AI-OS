#!/bin/bash
# Isolated real-Excel probe of MULTIPLE charts on one worksheet.
#
# The single-chart probe passed and the adapter still hung. A workbook left
# behind by the hung run showed exactly one chart, created correctly -- so the
# stall is in what happens after the first chart, not in creating one.
#
# Round 1 found it: `repeat with candidateChart in chart objects of <sheet>`
# hangs Excel indefinitely once a chart exists on that sheet, and `with timeout`
# does NOT rescue it -- the process has to be killed. That loop is exactly how
# the adapter checked for a duplicate chart name.
#
# Round 2 looks for a way to ask the same question without enumerating the
# elements.
#
# Every candidate is wrapped in `with timeout of 45 seconds`, so a stall
# reports as error -1712 instead of hanging, and builds a progress string as it
# goes, so the error names the step that stalled.
#
# Each candidate runs as its own AppleScript file and its own osascript
# invocation. Excel is never quit; the workbook count is reported before and
# after so a leak is visible.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-chart-multi.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

chart_step() {
  # $1 index, $2 chart type line, $3 name
  cat <<STEP
set madeChart$1 to make new chart object at chartTargetSheet
set liveText to liveText & "|made$1"
set source data chart of madeChart$1 source (range "A1:B3" of chartTargetSheet) plot by columns
set liveText to liveText & "|src$1"
$2
set liveText to liveText & "|type$1"
set name of madeChart$1 to "$3"
set liveText to liveText & "|name$1"
STEP
}

candidate_body() {
  case "$1" in
    one_chart)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      ;;
    two_charts)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      chart_step 2 'set chart type of chart of madeChart2 to bar clustered' 'ChartTwo'
      ;;
    two_charts_unnamed)
      echo 'set chartTargetSheet to active sheet of probeBook'
      echo 'set madeChart1 to make new chart object at chartTargetSheet'
      echo 'set liveText to liveText & "|made1"'
      echo 'set madeChart2 to make new chart object at chartTargetSheet'
      echo 'set liveText to liveText & "|made2"'
      ;;
    scan_count_only)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      echo 'set chartCount to count of chart objects of chartTargetSheet'
      echo 'set liveText to liveText & "|count=" & (chartCount as text)'
      ;;
    scan_by_index)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      echo 'set chartCount to count of chart objects of chartTargetSheet'
      echo 'set liveText to liveText & "|count=" & (chartCount as text)'
      echo 'repeat with chartIndex from 1 to chartCount'
      echo '    set scannedName to (name of chart object chartIndex of chartTargetSheet) as text'
      echo '    set liveText to liveText & "|[" & scannedName & "]"'
      echo 'end repeat'
      ;;
    scan_every_name)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      echo 'set scannedNames to name of every chart object of chartTargetSheet'
      echo 'set liveText to liveText & "|names=" & ((count of scannedNames) as text)'
      ;;
    scan_exists_by_name)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      echo 'set liveText to liveText & "|taken=" & ((exists chart object "ChartOne" of chartTargetSheet) as text)'
      echo 'set liveText to liveText & "|free=" & ((exists chart object "ChartTwo" of chartTargetSheet) as text)'
      ;;
    scan_repeat_elements_KNOWN_HANG)
      # Kept so the finding stays reproducible. This one hangs; it is last in
      # the list so it cannot block the candidates that matter.
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      echo 'repeat with candidateChart in chart objects of chartTargetSheet'
      echo '    set liveText to liveText & "|saw"'
      echo 'end repeat'
      ;;
    four_charts)
      echo 'set chartTargetSheet to active sheet of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ByColumn'
      chart_step 2 'set chart type of chart of madeChart2 to bar clustered' 'ByBar'
      chart_step 3 'set chart type of chart of madeChart3 to line markers' 'ByLine'
      chart_step 4 'set chart type of chart of madeChart4 to pie chart' 'ByPie'
      ;;
    chart_on_nonactive_sheet)
      echo 'set extraSheet to make new worksheet at end of probeBook'
      echo 'set name of extraSheet to "Charted"'
      echo 'set value of range "A1" of extraSheet to "Name"'
      echo 'set value of range "B1" of extraSheet to "Score"'
      echo 'set value of range "A2" of extraSheet to "Bravo"'
      echo 'set value of range "B2" of extraSheet to 2'
      echo 'set value of range "A3" of extraSheet to "Alpha"'
      echo 'set value of range "B3" of extraSheet to 1'
      echo 'activate object worksheet 1 of probeBook'
      echo 'set liveText to liveText & "|otherSheetActive"'
      echo 'set chartTargetSheet to worksheet "Charted" of probeBook'
      chart_step 1 'set chart type of chart of madeChart1 to column clustered' 'ChartOne'
      chart_step 2 'set chart type of chart of madeChart2 to bar clustered' 'ChartTwo'
      ;;
  esac
}

write_script() {
  local candidate="$1" path="$2"
  {
    cat <<'HEAD'
on run argv
    set outcomeText to "ok"
    set liveText to "start"

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
            with timeout of 45 seconds
HEAD
    candidate_body "$candidate" | sed 's/^/                /'
    cat <<'TAIL'
            end timeout
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
        end try

        try
            close probeBook saving no
        end try

        set booksAfter to count of workbooks
    end tell

    return outcomeText & " :: reached=[" & liveText & "] :: books " & (booksBefore as text) & "->" & (booksAfter as text)
end run
TAIL
  } > "$path"
}

say "=== multiple charts on one worksheet: where does it stall? ==="
say "each candidate reports the last step it reached before erroring or timing out"
say "-----"

for candidate in \
  scan_count_only scan_by_index scan_every_name scan_exists_by_name \
  four_charts chart_on_nonactive_sheet
do
  script="$WORK_DIR/$candidate.applescript"
  write_script "$candidate" "$script"

  if ! /usr/bin/osacompile -o "$WORK_DIR/$candidate.scpt" "$script" >"$WORK_DIR/$candidate.compile" 2>&1; then
    printf '%-32s COMPILE FAIL :: %s\n' "$candidate" "$(tr '\n' ' ' <"$WORK_DIR/$candidate.compile")" | tee -a "$LOG"
    continue
  fi

  started=$(date +%s)
  result="$(/usr/bin/osascript "$script" 2>&1)"
  elapsed=$(( $(date +%s) - started ))
  printf '%-32s [%3ds] %s\n' "$candidate" "$elapsed" "$result" | tee -a "$LOG"
done

say ""
say "log: $LOG"
