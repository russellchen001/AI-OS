#!/bin/bash
# Can Apple iWork handle Microsoft Office formats?
#
# This decides whether the Office skill is general or merely a set of
# per-application adapters. If Pages can open a .docx and Numbers a .xlsx, a Mac
# without Microsoft Office still serves document.read and spreadsheet.read: the
# capability survives the absence of any one application, which is the whole
# point of a provider-neutral skill.
#
# Round 1 looked like three flat refusals and was not:
#
#   Pages    -1700  «class pTxt» of missing value
#   Numbers  -1719  can't get table 1 of sheet 1 of document id "D88D06CC-..."
#   Keynote  -1700  can't convert missing value to specifier
#
# Numbers named a real document id while claiming it had no table, which is not
# what refusing to open looks like. `open POSIX file` returns `missing value`
# for a foreign format -- iWork converts on import and the returned specifier is
# not the converted document. The fix is the pattern keynote.rs already uses for
# its own format: open, then FIND the document by polling, and give the
# conversion time to finish.
#
# Every step reports on its own so one failure cannot be mistaken for another's.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-iwork-interop.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }
run() { /usr/bin/osascript - "$@" 2>&1 | tee -a "$LOG"; }

WORK="$HOME/Documents/ai-os-interop-probe"
rm -rf "$WORK"
mkdir -p "$WORK"

cleanup() {
  # Close only what this probe opened, by path, in every application it touched.
  for spec in "com.microsoft.Excel" "com.apple.Pages" "com.apple.Numbers"; do
    /usr/bin/osascript - "$WORK" "$spec" >/dev/null 2>&1 <<'APPLESCRIPT' || true
on run argv
    set workDir to item 1 of argv
    set appId to item 2 of argv
    tell application id appId
        repeat with candidate in (every document)
            try
                set candidateFile to file of candidate
                if candidateFile is not missing value then
                    if (POSIX path of (candidateFile as alias)) starts with workDir then
                        close candidate saving no
                    end if
                end if
            end try
        end repeat
    end tell
end run
APPLESCRIPT
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

say "=== fixtures ==="
say "docx comes from textutil, which needs no Word automation at all."
say "Word's own save-as form is not one its dictionary accepts here, and"
say "driving PowerPoint for a fixture hung the whole run."

printf 'Interop alpha.\nInterop bravo.\n' > "$WORK/source.txt"
if /usr/bin/textutil -convert docx -output "$WORK/from-word.docx" "$WORK/source.txt" 2>>"$LOG"; then
  say "-- docx fixture ok (textutil)"
else
  say "-- docx fixture FAILED (textutil)"
fi

say "-- xlsx fixture (Excel, the one Office form already proven here) --"
run "$WORK/from-excel.xlsx" <<'APPLESCRIPT'
on run argv
    set targetPath to item 1 of argv
    tell application "Microsoft Excel"
        try
            set madeBook to make new workbook
            tell active sheet of madeBook
                set value of range "A1" to "Region"
                set value of range "B1" to "Total"
                set value of range "A2" to "North"
                set value of range "B2" to 42
            end tell
            save workbook as madeBook filename targetPath file format Excel XML file format
            set madeBook to active workbook
            close madeBook saving no
            return "xlsx fixture ok"
        on error e number n
            return "xlsx fixture ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "files on disk after the fixture step:"
ls -l "$WORK" 2>&1 | tee -a "$LOG"

say ""
say "=== Pages opening a .docx ==="
run "$WORK/from-word.docx" "$WORK/pages-roundtrip.docx" <<'APPLESCRIPT'
on documentIds(appId)
    set found to {}
    tell application id appId
        repeat with candidate in (every document)
            try
                set end of found to (id of candidate) as text
            end try
        end repeat
    end tell
    return found
end documentIds

on isKnown(knownIds, candidateId)
    repeat with knownId in knownIds
        if (contents of knownId as text) is candidateId then return true
    end repeat
    return false
end isKnown

on openForeign(appId, targetPath)
    -- iWork imports a foreign format into a NEW, UNSAVED document, so its
    -- `file` is missing value and matching on path can never succeed. The
    -- document is identified by which id appeared, which is also what makes it
    -- safe to close: only a document this probe caused is ever touched.
    set knownBefore to my documentIds(appId)

    set openReport to "ok"
    tell application id appId
        try
            open POSIX file targetPath
        on error e number n
            set openReport to "open ERROR " & n & " :: " & e
        end try
    end tell

    repeat 40 times
        set seenNow to my documentIds(appId)
        repeat with candidateId in seenNow
            if not my isKnown(knownBefore, contents of candidateId as text) then
                tell application id appId
                    repeat with candidate in (every document)
                        try
                            if ((id of candidate) as text) is (contents of candidateId as text) then
                                return {candidate, openReport}
                            end if
                        end try
                    end repeat
                end tell
            end if
        end repeat
        delay 0.25
    end repeat

    return {missing value, openReport}
end openForeign

on run argv
    set sourcePath to item 1 of argv
    set exportPath to item 2 of argv

    set outcome to my openForeign("com.apple.Pages", sourcePath)
    set opened to item 1 of outcome
    set openReport to item 2 of outcome

    if opened is missing value then
        tell application id "com.apple.Pages"
            set openCount to count of documents
        end tell
        return "NO NEW DOCUMENT :: " & openReport & " :: documents open=" & (openCount as text)
    end if

    tell application id "com.apple.Pages"
        try
            set bodyText to (body text of opened) as text
            set report to openReport & " | chars=" & ((count characters of bodyText) as text)
            set report to report & " paras=" & ((count of paragraphs of bodyText) as text)
            if (count characters of bodyText) > 0 then
                set report to report & " first=[" & (paragraph 1 of bodyText as text) & "]"
            end if

            try
                export opened to POSIX file exportPath as Microsoft Word
                set report to report & " | export-docx ok"
            on error e2 number n2
                set report to report & " | export-docx ERROR " & n2 & " :: " & e2
            end try

            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "READ ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Numbers opening a .xlsx ==="
run "$WORK/from-excel.xlsx" "$WORK/numbers-roundtrip.xlsx" <<'APPLESCRIPT'
on documentIds(appId)
    set found to {}
    tell application id appId
        repeat with candidate in (every document)
            try
                set end of found to (id of candidate) as text
            end try
        end repeat
    end tell
    return found
end documentIds

on isKnown(knownIds, candidateId)
    repeat with knownId in knownIds
        if (contents of knownId as text) is candidateId then return true
    end repeat
    return false
end isKnown

on openForeign(appId, targetPath)
    set knownBefore to my documentIds(appId)

    set openReport to "ok"
    tell application id appId
        try
            open POSIX file targetPath
        on error e number n
            set openReport to "open ERROR " & n & " :: " & e
        end try
    end tell

    repeat 40 times
        set seenNow to my documentIds(appId)
        repeat with candidateId in seenNow
            if not my isKnown(knownBefore, contents of candidateId as text) then
                tell application id appId
                    repeat with candidate in (every document)
                        try
                            if ((id of candidate) as text) is (contents of candidateId as text) then
                                return {candidate, openReport}
                            end if
                        end try
                    end repeat
                end tell
            end if
        end repeat
        delay 0.25
    end repeat

    return {missing value, openReport}
end openForeign

on render(v)
    if v is missing value then return ""
    return v as text
end render

on run argv
    set sourcePath to item 1 of argv
    set exportPath to item 2 of argv

    set outcome to my openForeign("com.apple.Numbers", sourcePath)
    set opened to item 1 of outcome
    set openReport to item 2 of outcome

    if opened is missing value then
        tell application id "com.apple.Numbers"
            set openCount to count of documents
        end tell
        return "NO NEW DOCUMENT :: " & openReport & " :: documents open=" & (openCount as text)
    end if

    tell application id "com.apple.Numbers"
        try
            set report to openReport & " | sheets=" & ((count of sheets of opened) as text)
            set report to report & " tables=" & ((count of tables of sheet 1 of opened) as text)

            tell table 1 of sheet 1 of opened
                set report to report & " A1=[" & my render(value of cell "A1") & "]"
                set report to report & " B2=[" & my render(value of cell "B2") & "]"
                set report to report & " rows=" & ((count of rows) as text)
            end tell

            try
                export opened to POSIX file exportPath as Microsoft Excel
                set report to report & " | export-xlsx ok"
            on error e2 number n2
                set report to report & " | export-xlsx ERROR " & n2 & " :: " & e2
            end try

            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "READ ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "Keynote/.pptx is deliberately not asked here: it needs a .pptx fixture,"
say "and building one through PowerPoint automation hung this probe once"
say "already. It is asked separately, from a fixture the PowerPoint adapter"
say "creates -- that path is already proven."

say ""
say "=== artifacts ==="
ls -l "$WORK" 2>&1 | tee -a "$LOG"
say ""
say "log: $LOG"
