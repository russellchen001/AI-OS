#!/bin/bash
# Isolated real-Excel probe of the worksheet-delete confirmation.
#
# `delete worksheet X of <workbook>` raised a modal ("Microsoft Excel will
# permanently delete this sheet") in the middle of an unattended run, which no
# automation can answer: the run sat there until it was killed, and the modal
# then blocked every later Excel automation too.
#
# It had not raised that modal before, because this machine's Excel happened to
# have `display alerts` off. Depending on that is what made the failure look
# random.
#
# This asks three things:
#   1. can `display alerts` be read and written from AppleScript
#   2. does setting it false actually suppress the delete confirmation
#   3. is the original value restorable, including when the delete errors --
#      leaving a user's Excel with alerts off is not acceptable
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-excel-alerts.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

SCRIPT="$(mktemp -t aios_excel_alerts)"
trap 'rm -f "$SCRIPT"' EXIT

cat > "$SCRIPT" <<'APPLESCRIPT'
on run argv
    set operationKind to item 1 of argv
    set outcomeText to "ok"
    set liveText to ""

    tell application "Microsoft Excel"
        set booksBefore to count of workbooks
        set alertsBefore to display alerts
        set probeBook to make new workbook
        set extraSheet to make new worksheet at end of probeBook
        set name of extraSheet to "Doomed"

        try
            with timeout of 30 seconds
                if operationKind is "read_write_restore" then
                    set liveText to "before=" & (alertsBefore as text)
                    set display alerts to false
                    set liveText to liveText & " off=" & ((display alerts) as text)
                    set display alerts to alertsBefore
                    set liveText to liveText & " restored=" & ((display alerts) as text)

                else if operationKind is "delete_with_alerts_off" then
                    set display alerts to false
                    delete worksheet "Doomed" of probeBook
                    set display alerts to alertsBefore
                    set liveText to "deleted sheets=" & ((count of worksheets of probeBook) as text) & " alerts=" & ((display alerts) as text)

                else if operationKind is "restore_after_error" then
                    set display alerts to false
                    try
                        delete worksheet "NoSuchSheet" of probeBook
                    on error innerText number innerNumber
                        set display alerts to alertsBefore
                        set liveText to "errored=" & innerNumber & " alerts=" & ((display alerts) as text)
                    end try
                end if
            end timeout
        on error errorText number errorNumber
            set outcomeText to "ERROR " & errorNumber & " :: " & errorText
            try
                set display alerts to alertsBefore
            end try
        end try

        try
            set display alerts to false
            close probeBook saving no
            set display alerts to alertsBefore
        end try

        set booksAfter to count of workbooks
    end tell

    return outcomeText & " :: " & liveText & " :: books " & (booksBefore as text) & "->" & (booksAfter as text)
end run
APPLESCRIPT

say "=== worksheet delete confirmation ==="
say "a candidate that hangs means the modal was NOT suppressed"
say "-----"

for candidate in read_write_restore delete_with_alerts_off restore_after_error
do
  started=$(date +%s)
  result="$(/usr/bin/osascript "$SCRIPT" "$candidate" 2>&1)"
  elapsed=$(( $(date +%s) - started ))
  printf '%-26s [%3ds] %s\n' "$candidate" "$elapsed" "$result" | tee -a "$LOG"
done

say ""
say "current display alerts:"
/usr/bin/osascript -e 'tell application "Microsoft Excel" to return (display alerts) as text' 2>&1 | tee -a "$LOG"
say "log: $LOG"
