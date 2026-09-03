#!/bin/bash
# Isolated real-Mac probe of Pages and Numbers automation.
#
# The Provider Registry declares all eight Office capabilities for Apple iWork,
# but only presentation.read and presentation.create have an adapter (Keynote).
# Pages and Numbers have none, and the existing iWork E2E only checks that the
# apps exist and answer `get version` -- which the handoff explicitly says does
# not count as capability.
#
# So nothing is known about what these two can actually do. This asks, for each:
#
#   1. can a document be created, given content, and SAVED to a real file
#   2. can that file be closed, reopened, and its content READ BACK
#   3. what does the read-back actually look like -- shape and rendering
#   4. can it export to a portable format (PDF for Pages, CSV for Numbers)
#
# Round 1 answered most of it:
#   Pages   create/write/save/close/reopen/read body text   works
#           count of words and characters                   works
#           export ... to POSIX file <no extension> as PDF  FAILS (error 6),
#                                                           and leaks an open doc
#   Numbers create/write/save/close/reopen/read cells       works
#           a NEW table is 22 rows x 7 columns already      -- there is no
#                                                           "used range"; reads
#                                                           must trim
#           sheet and table names are LOCALIZED             ("工作表 1", "表格 1")
#                                                           -- address by index,
#                                                           never by name
#           value of every cell of range / of row           list, as expected
#           a "=SUM(...)" string really becomes a formula   value 30.0, formula
#                                                           readable
#           export ... as CSV                               works
#
# Round 2 settles the export form and what an empty cell reads back as.
#
# Each candidate is its own AppleScript file and its own osascript invocation,
# wrapped in `with timeout`, so one bad form cannot take the run down and a
# stall reports instead of hanging. Documents are closed without saving; the
# apps are never quit, and the open-document count is reported before and after
# so a leak is visible.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-iwork.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

WORK_DIR="$(mktemp -d)"
OUT_DIR="$HOME/Documents/ai-os-iwork-probe"
mkdir -p "$OUT_DIR"
trap 'rm -rf "$WORK_DIR"; rm -rf "$OUT_DIR"' EXIT

pages_body() {
  case "$1" in
    create_save_reopen)
      cat <<'BODY'
set madeDocument to make new document
set liveText to "made"
set body text of madeDocument to "Alpha paragraph." & return & "Bravo paragraph."
set liveText to liveText & "|wrote"
save madeDocument in POSIX file savedPath
set liveText to liveText & "|saved"
close madeDocument saving no
set liveText to liveText & "|closed"

set reopened to open POSIX file savedPath
set readBack to body text of reopened as text
set liveText to liveText & "|reopened chars=" & ((count characters of readBack) as text)
set liveText to liveText & "|paras=" & ((count of paragraphs of body text of reopened) as text)
set liveText to liveText & "|first=[" & (paragraph 1 of body text of reopened as text) & "]"
close reopened saving no
BODY
      ;;
    export_pdf_with_extension)
      cat <<'BODY'
set madeDocument to make new document
set body text of madeDocument to "Exported content."
save madeDocument in POSIX file savedPath
set liveText to "saved"
export madeDocument to POSIX file (exportPath & ".pdf") as PDF
set liveText to liveText & "|exported"
close madeDocument saving no
BODY
      ;;
    export_pdf_into_directory)
      cat <<'BODY'
set madeDocument to make new document
set body text of madeDocument to "Exported content."
save madeDocument in POSIX file savedPath
set liveText to "saved"
do shell script "/bin/mkdir -p " & quoted form of exportPath
export madeDocument to POSIX file exportPath as PDF
set liveText to liveText & "|exported"
close madeDocument saving no
BODY
      ;;
    word_and_char_counts)
      cat <<'BODY'
set madeDocument to make new document
set body text of madeDocument to "one two three"
set liveText to "words=" & ((count of words of body text of madeDocument) as text)
set liveText to liveText & "|chars=" & ((count characters of body text of madeDocument) as text)
close madeDocument saving no
BODY
      ;;
  esac
}

numbers_body() {
  case "$1" in
    create_save_reopen)
      cat <<'BODY'
set madeDocument to make new document
set liveText to "made"
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "Region"
    set value of cell "B1" to "Total"
    set value of cell "A2" to "North"
    set value of cell "B2" to 42
end tell
set liveText to liveText & "|wrote"
save madeDocument in POSIX file savedPath
set liveText to liveText & "|saved"
close madeDocument saving no
set liveText to liveText & "|closed"

set reopened to open POSIX file savedPath
tell table 1 of sheet 1 of reopened
    set liveText to liveText & "|A1=[" & ((value of cell "A1") as text) & "]"
    set liveText to liveText & "|B2=[" & ((value of cell "B2") as text) & "]"
    set liveText to liveText & "|rows=" & ((count of rows) as text)
    set liveText to liveText & "|cols=" & ((count of columns) as text)
end tell
close reopened saving no
BODY
      ;;
    sheet_and_table_names)
      cat <<'BODY'
set madeDocument to make new document
set liveText to "sheets=" & ((count of sheets of madeDocument) as text)
set liveText to liveText & "|sheet1=[" & (name of sheet 1 of madeDocument as text) & "]"
set liveText to liveText & "|tables=" & ((count of tables of sheet 1 of madeDocument) as text)
set liveText to liveText & "|table1=[" & (name of table 1 of sheet 1 of madeDocument as text) & "]"
close madeDocument saving no
BODY
      ;;
    bulk_cell_read)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "x"
    set value of cell "B1" to 1
    set everyValue to value of every cell of range "A1:B1"
end tell
set liveText to "class=" & (class of everyValue as text)
set liveText to liveText & "|count=" & ((count of everyValue) as text)
close madeDocument saving no
BODY
      ;;
    row_wise_read)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "x"
    set value of cell "B1" to 1
    set rowValues to value of every cell of row 1
end tell
set liveText to "class=" & (class of rowValues as text)
set liveText to liveText & "|count=" & ((count of rowValues) as text)
close madeDocument saving no
BODY
      ;;
    formula_cell)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to 10
    set value of cell "A2" to 20
    set value of cell "A3" to "=SUM(A1:A2)"
    set liveText to "A3value=[" & ((value of cell "A3") as text) & "]"
    set liveText to liveText & "|A3formula=[" & ((formula of cell "A3") as text) & "]"
end tell
close madeDocument saving no
BODY
      ;;
    empty_cell_and_trim)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "only"
    set liveText to "rows=" & ((count of rows) as text) & "|cols=" & ((count of columns) as text)
    set emptyValue to value of cell "C3"
    if emptyValue is missing value then
        set liveText to liveText & "|C3=missing value"
    else
        set liveText to liveText & "|C3=[" & (emptyValue as text) & "] class=" & (class of emptyValue as text)
    end if
    set liveText to liveText & "|A1class=" & (class of (value of cell "A1") as text)
end tell
close madeDocument saving no
BODY
      ;;
    export_xlsx)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "Region"
    set value of cell "B1" to 42
end tell
save madeDocument in POSIX file savedPath
set liveText to "saved"
export madeDocument to POSIX file (exportPath & ".xlsx") as Microsoft Excel
set liveText to liveText & "|exported"
close madeDocument saving no
BODY
      ;;
    add_sheet_and_table)
      cat <<'BODY'
set madeDocument to make new document
set liveText to "sheetsBefore=" & ((count of sheets of madeDocument) as text)
set extraSheet to make new sheet at end of sheets of madeDocument
set name of extraSheet to "AiosSheet"
set liveText to liveText & "|sheetsAfter=" & ((count of sheets of madeDocument) as text)
set liveText to liveText & "|named=[" & (name of sheet 2 of madeDocument as text) & "]"
set liveText to liveText & "|tablesOnNew=" & ((count of tables of extraSheet) as text)
close madeDocument saving no
BODY
      ;;
    grow_rows_and_columns)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set liveText to "before=" & ((count of rows) as text) & "x" & ((count of columns) as text)
    repeat 3 times
        add row below last row
    end repeat
    repeat 2 times
        add column after last column
    end repeat
    set liveText to liveText & "|after=" & ((count of rows) as text) & "x" & ((count of columns) as text)
    set value of cell "I25" to "corner"
    set liveText to liveText & "|I25=[" & ((value of cell "I25") as text) & "]"
end tell
close madeDocument saving no
BODY
      ;;
    shrink_rows)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set liveText to "before=" & ((count of rows) as text)
    repeat 5 times
        remove last row
    end repeat
    set liveText to liveText & "|after=" & ((count of rows) as text)
end tell
close madeDocument saving no
BODY
      ;;
    separator_inside_tell)
      cat <<'BODY'
set madeDocument to make new document
set tabConstant to "unavailable"
try
    set tabConstant to "[" & tab & "]"
end try
set asciiNine to "[" & (ASCII character 9) & "]"
set liveText to "tab=" & tabConstant & "|ascii9=" & asciiNine
set liveText to liveText & "|tabIsAscii9=" & ((tabConstant is asciiNine) as text)
close madeDocument saving no
BODY
      ;;
    export_csv)
      cat <<'BODY'
set madeDocument to make new document
tell table 1 of sheet 1 of madeDocument
    set value of cell "A1" to "Region"
    set value of cell "B1" to 42
end tell
save madeDocument in POSIX file savedPath
set liveText to "saved"
export madeDocument to POSIX file exportPath as CSV
set liveText to liveText & "|exported"
close madeDocument saving no
BODY
      ;;
  esac
}

write_script() {
  local app_id="$1" candidate="$2" body_fn="$3" path="$4"
  {
    cat <<HEAD
on run argv
    set savedPath to item 1 of argv
    set exportPath to item 2 of argv
    set outcomeText to "ok"
    set liveText to ""
    set madeDocument to missing value
    set reopened to missing value

    tell application id "$app_id"
        set docsBefore to count of documents

        try
            with timeout of 60 seconds
HEAD
    "$body_fn" "$candidate" | sed 's/^/                /'
    cat <<'TAIL'
            end timeout
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
        end try

        -- Close exactly the documents this candidate created. A candidate that
        -- errors leaves an UNSAVED document, whose name matches no prefix, so
        -- closing by name would miss it -- and closing every document would
        -- take the user's own work with it.
        try
            if reopened is not missing value then close reopened saving no
        end try
        try
            if madeDocument is not missing value then close madeDocument saving no
        end try

        set docsAfter to count of documents
    end tell

    return outcomeText & " :: " & liveText & " :: docs " & (docsBefore as text) & "->" & (docsAfter as text)
end run
TAIL
  } > "$path"
}

run_candidates() {
  local label="$1" app_id="$2" extension="$3" body_fn="$4"
  shift 4

  say ""
  say "=== $label ($app_id) ==="

  for candidate in "$@"; do
    local script="$WORK_DIR/$label-$candidate.applescript"
    local saved="$OUT_DIR/$label-$candidate.$extension"
    local exported="$OUT_DIR/$label-$candidate-export"

    rm -f "$saved"
    rm -rf "$exported"

    write_script "$app_id" "$candidate" "$body_fn" "$script"

    if ! /usr/bin/osacompile -o "$WORK_DIR/$candidate.scpt" "$script" \
        >"$WORK_DIR/$candidate.compile" 2>&1; then
      printf '%-24s COMPILE FAIL :: %s\n' "$candidate" \
        "$(tr '\n' ' ' <"$WORK_DIR/$candidate.compile")" | tee -a "$LOG"
      continue
    fi

    local started result elapsed artifacts
    started=$(date +%s)
    result="$(/usr/bin/osascript "$script" "$saved" "$exported" 2>&1)"
    elapsed=$(( $(date +%s) - started ))

    artifacts=""
    [ -e "$saved" ] && artifacts="saved($(du -sk "$saved" | cut -f1)k)"
    [ -e "$exported" ] && artifacts="$artifacts exported($(du -sk "$exported" | cut -f1)k)"

    printf '%-24s [%3ds] %s :: %s\n' \
      "$candidate" "$elapsed" "$result" "${artifacts:-no artifact}" | tee -a "$LOG"
  done

  # A candidate that errors can leave its document open. Close ONLY documents
  # this probe named -- the user may well have their own open, and closing
  # every document would take theirs with it.
  /usr/bin/osascript <<APPLESCRIPT >/dev/null 2>&1
tell application id "$app_id"
    repeat with candidate in (every document)
        try
            if (name of candidate as text) starts with "$label-" then
                close candidate saving no
            end if
        end try
    end repeat
end tell
APPLESCRIPT
}

say "Pages and Numbers: what is actually executable?"
say "output goes under $OUT_DIR and is removed on exit"

run_candidates pages com.apple.Pages pages pages_body \
  export_pdf_with_extension

run_candidates numbers com.apple.Numbers numbers numbers_body \
  grow_rows_and_columns shrink_rows separator_inside_tell

say ""
say "log: $LOG"
