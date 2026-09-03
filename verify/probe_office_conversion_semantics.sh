#!/bin/bash
# What does "convert" actually mean to each application?
#
# The owner settled the product question: conversion is two things, and both
# must work.
#
#   1. Cross-suite format conversion, so an iWork application can be handed an
#      Office file and an Office application an iWork file.
#   2. PDF export, from any Office or iWork file.
#
# The destination format decides which one a request is, so the routing needs to
# know, for every (source, destination) pair, which application can do it and
# with which verb. That is what this asks. Nothing is written against a guess.
#
# One thing is already known and is the reason this probe exists in the shape it
# does: `open POSIX file` returns `missing value` for a foreign format, because
# iWork converts on import into a NEW UNSAVED document whose `file` is missing
# value. So an imported document can only be found by which id appeared, which
# is also what makes it safe to close -- only a document this probe caused is
# ever touched. No application is ever quit.
#
# Every question reports on its own line so one failure cannot be read as
# another's.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

LOG="${TMPDIR:-/tmp}/aios-probe-office-conversion.log"
: > "$LOG"

say() { printf '%s\n' "$1" | tee -a "$LOG"; }

# iWork sandboxing is least trouble under Documents, which is where the Pages
# and Numbers adapters already put their work.
WORK="$HOME/Documents/ai-os-conversion-probe"
rm -rf "$WORK"
mkdir -p "$WORK"

# The helper every iWork question needs, kept in one place.
IWORK_PRELUDE='
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
    -- The file is coerced to an alias BEFORE it is sent. Handing iWork a bare
    -- `POSIX file` specifier while the application is not frontmost makes `open`
    -- return success and do nothing at all: no document, no window, no error,
    -- for as long as you care to wait. An alias resolves in the sending process,
    -- so what arrives is a real file reference. Proven by asking the same file
    -- four ways: POSIX file silently did nothing, alias worked in 1s,
    -- `/usr/bin/open -a` worked in 2s, and `activate` before the POSIX form also
    -- worked -- which is what makes it a specifier problem and not a file one.
    set targetAlias to (POSIX file targetPath) as alias
    set knownBefore to my documentIds(appId)
    set openReport to "open ok"
    tell application id appId
        try
            open targetAlias
        on error e number n
            set openReport to "open ERROR " & n & " :: " & e
        end try
    end tell
    repeat 60 times
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

on sizeOf(aPath)
    try
        set szText to do shell script "/usr/bin/stat -f %z " & quoted form of aPath
        return szText
    on error
        return "MISSING"
    end try
end sizeOf
'

ask() {
  local label="$1"
  shift
  say ""
  say "--- $label ---"
  /usr/bin/osascript - "$@" 2>&1 | tee -a "$LOG"
}

cleanup() {
  say ""
  say "=== cleanup: closing only documents under the probe directory ==="
  for spec in "com.apple.Pages" "com.apple.Numbers" "com.apple.Keynote" "com.microsoft.Excel"; do
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
}
trap cleanup EXIT

say "=== fixtures ==="
printf 'Conversion alpha.\nConversion bravo.\n' > "$WORK/source.txt"
if /usr/bin/textutil -convert docx -output "$WORK/from-word.docx" "$WORK/source.txt" 2>>"$LOG"; then
  say "docx fixture ok (textutil, no Word automation needed)"
else
  say "docx fixture FAILED"
fi

ask "xlsx fixture, through the Excel form already proven here" \
  "$WORK/from-excel.xlsx" <<'APPLESCRIPT'
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
            close active workbook saving no
            return "xlsx fixture ok"
        on error e number n
            return "xlsx fixture ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Q1  Excel: can it write a PDF itself? ==="
say "If it can, .xlsx -> .pdf needs no other application."
ask "Excel save as PDF" "$WORK/from-excel.xlsx" "$WORK/excel-export.pdf" <<'APPLESCRIPT'
on run argv
    set sourcePath to item 1 of argv
    set pdfPath to item 2 of argv
    tell application "Microsoft Excel"
        try
            open POSIX file sourcePath
            set theBook to active workbook
            set report to "opened=" & (name of theBook as text)
            try
                save workbook as theBook filename pdfPath file format PDF file format
                set report to report & " | save-as-PDF ok"
            on error e2 number n2
                set report to report & " | save-as-PDF ERROR " & n2 & " :: " & e2
                try
                    tell theBook to save as filename pdfPath file format PDF file format
                    set report to report & " | tell-form ok"
                on error e3 number n3
                    set report to report & " | tell-form ERROR " & n3 & " :: " & e3
                end try
            end try
            close active workbook saving no
            return report
        on error e number n
            return "ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Q2  Pages: .docx in, .pages and .docx and .pdf out ==="
ask "Pages conversion round trip" \
  "$WORK/from-word.docx" "$WORK/pages-native.pages" "$WORK/pages-export.docx" "$WORK/pages-export.pdf" \
  <<APPLESCRIPT
$IWORK_PRELUDE
on run argv
    set sourcePath to item 1 of argv
    set nativePath to item 2 of argv
    set docxPath to item 3 of argv
    set pdfPath to item 4 of argv

    set outcome to my openForeign("com.apple.Pages", sourcePath)
    set opened to item 1 of outcome
    set report to item 2 of outcome

    if opened is missing value then return "NO NEW DOCUMENT :: " & report

    tell application id "com.apple.Pages"
        try
            set bodyText to (body text of opened) as text
            set report to report & " | chars=" & ((count characters of bodyText) as text)
            if (count characters of bodyText) > 0 then
                set report to report & " first=[" & (paragraph 1 of bodyText as text) & "]"
            end if

            -- Saving the imported document AS .pages is what makes docx -> pages
            -- possible at all. Two forms are asked because the dictionary allows
            -- one and the other is what people write.
            try
                save opened in POSIX file nativePath
                set report to report & " | save-in ok size=" & my sizeOf(nativePath)
            on error e1 number n1
                set report to report & " | save-in ERROR " & n1 & " :: " & e1
                try
                    export opened to POSIX file nativePath as Pages
                    set report to report & " | export-as-Pages ok size=" & my sizeOf(nativePath)
                on error e1b number n1b
                    set report to report & " | export-as-Pages ERROR " & n1b & " :: " & e1b
                end try
            end try

            try
                export opened to POSIX file docxPath as Microsoft Word
                set report to report & " | export-docx ok size=" & my sizeOf(docxPath)
            on error e2 number n2
                set report to report & " | export-docx ERROR " & n2 & " :: " & e2
            end try

            try
                export opened to POSIX file pdfPath as PDF
                set report to report & " | export-pdf ok size=" & my sizeOf(pdfPath)
            on error e3 number n3
                set report to report & " | export-pdf ERROR " & n3 & " :: " & e3
            end try

            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "FAILED " & n & " :: " & e & " || " & report
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Q3  Numbers: .xlsx in, .numbers and .xlsx and .pdf out ==="
ask "Numbers conversion round trip" \
  "$WORK/from-excel.xlsx" "$WORK/numbers-native.numbers" "$WORK/numbers-export.xlsx" "$WORK/numbers-export.pdf" \
  <<APPLESCRIPT
$IWORK_PRELUDE
on render(v)
    if v is missing value then return ""
    return v as text
end render

on run argv
    set sourcePath to item 1 of argv
    set nativePath to item 2 of argv
    set xlsxPath to item 3 of argv
    set pdfPath to item 4 of argv

    set outcome to my openForeign("com.apple.Numbers", sourcePath)
    set opened to item 1 of outcome
    set report to item 2 of outcome

    if opened is missing value then return "NO NEW DOCUMENT :: " & report

    -- The document appears before the conversion has finished, so sheet 1 exists
    -- while "table 1 of sheet 1" is still an invalid index (-1719). Round one of
    -- the earlier interop probe read that as a refusal to open; it is not, and
    -- the fix is to wait for the table rather than for the document.
    set tableWait to "tables never appeared"
    repeat with tick from 1 to 60
        tell application id "com.apple.Numbers"
            try
                if (count of tables of sheet 1 of opened) > 0 then
                    set tableWait to "tables after " & (tick as text) & " quarter-seconds"
                    exit repeat
                end if
            end try
        end tell
        delay 0.25
    end repeat
    set report to report & " | " & tableWait

    tell application id "com.apple.Numbers"
        try
            set report to report & " | sheets=" & ((count of sheets of opened) as text)
            tell table 1 of sheet 1 of opened
                set report to report & " A1=[" & my render(value of cell "A1") & "]"
                set report to report & " B2=[" & my render(value of cell "B2") & "]"
            end tell

            try
                save opened in POSIX file nativePath
                set report to report & " | save-in ok size=" & my sizeOf(nativePath)
            on error e1 number n1
                set report to report & " | save-in ERROR " & n1 & " :: " & e1
            end try

            try
                export opened to POSIX file xlsxPath as Microsoft Excel
                set report to report & " | export-xlsx ok size=" & my sizeOf(xlsxPath)
            on error e2 number n2
                set report to report & " | export-xlsx ERROR " & n2 & " :: " & e2
            end try

            try
                export opened to POSIX file pdfPath as PDF
                set report to report & " | export-pdf ok size=" & my sizeOf(pdfPath)
            on error e3 number n3
                set report to report & " | export-pdf ERROR " & n3 & " :: " & e3
            end try

            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "FAILED " & n & " :: " & e & " || " & report
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Q4  Keynote: it makes its own .pptx fixture, then reads it back ==="
say "PowerPoint is not driven for a fixture -- doing that hung an earlier probe."
ask "Keynote makes a .pptx and a .pdf from its own document" \
  "$WORK/keynote-source.key" "$WORK/keynote-export.pptx" "$WORK/keynote-export.pdf" \
  <<APPLESCRIPT
$IWORK_PRELUDE
on run argv
    set nativePath to item 1 of argv
    set pptxPath to item 2 of argv
    set pdfPath to item 3 of argv

    tell application id "com.apple.Keynote"
        try
            set made to make new document
            set report to "created"
            try
                set the object text of the default title item of the current slide of made to "Conversion Title"
                set the object text of the default body item of the current slide of made to "Conversion Body"
                set report to report & " | content set"
            on error eb number nb
                set report to report & " | content ERROR " & nb & " :: " & eb
            end try

            try
                save made in POSIX file nativePath
                set report to report & " | save-in ok size=" & my sizeOf(nativePath)
            on error e1 number n1
                set report to report & " | save-in ERROR " & n1 & " :: " & e1
            end try

            try
                export made to POSIX file pptxPath as Microsoft PowerPoint
                set report to report & " | export-pptx ok size=" & my sizeOf(pptxPath)
            on error e2 number n2
                set report to report & " | export-pptx ERROR " & n2 & " :: " & e2
            end try

            try
                export made to POSIX file pdfPath as PDF
                set report to report & " | export-pdf ok size=" & my sizeOf(pdfPath)
            on error e3 number n3
                set report to report & " | export-pdf ERROR " & n3 & " :: " & e3
            end try

            close made saving no
            return report
        on error e number n
            return "FAILED " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

ask "Keynote opening the .pptx it just wrote" \
  "$WORK/keynote-export.pptx" "$WORK/keynote-from-pptx.key" \
  <<APPLESCRIPT
$IWORK_PRELUDE
on run argv
    set sourcePath to item 1 of argv
    set nativePath to item 2 of argv

    set outcome to my openForeign("com.apple.Keynote", sourcePath)
    set opened to item 1 of outcome
    set report to item 2 of outcome

    if opened is missing value then return "NO NEW DOCUMENT :: " & report

    tell application id "com.apple.Keynote"
        try
            set report to report & " | slides=" & ((count of slides of opened) as text)
            try
                set report to report & " title=[" & (object text of the default title item of slide 1 of opened as text) & "]"
            end try
            try
                save opened in POSIX file nativePath
                set report to report & " | save-in ok size=" & my sizeOf(nativePath)
            on error e1 number n1
                set report to report & " | save-in ERROR " & n1 & " :: " & e1
            end try
            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "FAILED " & n & " :: " & e & " || " & report
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== Q5  does a destination that is refused leave a part-file behind? ==="
say "The no-overwrite rule needs to know."
ask "export onto an existing path" "$WORK/pages-export.pdf" "$WORK/from-word.docx" <<APPLESCRIPT
$IWORK_PRELUDE
on run argv
    set existingPath to item 1 of argv
    set sourcePath to item 2 of argv

    set outcome to my openForeign("com.apple.Pages", sourcePath)
    set opened to item 1 of outcome
    if opened is missing value then return "NO NEW DOCUMENT"

    tell application id "com.apple.Pages"
        try
            set sizeBefore to my sizeOf(existingPath)
            try
                export opened to POSIX file existingPath as PDF
                set report to "export onto existing path SUCCEEDED (it overwrites) before=" & sizeBefore & " after=" & my sizeOf(existingPath)
            on error e number n
                set report to "export onto existing path refused " & n & " :: " & e & " || size still " & my sizeOf(existingPath)
            end try
            close opened saving no
            return report
        on error e number n
            try
                close opened saving no
            end try
            return "FAILED " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT

say ""
say "=== artifacts ==="
ls -l "$WORK" 2>&1 | tee -a "$LOG"
say ""
say "log: $LOG"
