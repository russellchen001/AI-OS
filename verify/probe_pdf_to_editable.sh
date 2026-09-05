#!/bin/bash
# Can anything on this machine turn a PDF back into an editable document?
#
# Everything else the PDF capability does is done to the file itself, in Rust,
# on any machine. Re-typesetting is the one thing that genuinely cannot be:
# a PDF has no paragraphs to reflow, so changing one means laying the page out
# again, and that needs a word processor. This asks which of them -- if any --
# can do it here, so the answer can be wired the way OCR is: used where the
# platform offers it, honestly absent where it does not.
#
# It answers three questions and nothing else:
#
#   1. Will Word open a PDF at all, driven by AppleScript?
#   2. If it does, can it save the result as .docx?
#   3. How much of the document survives -- does the text come back whole?
#
# It PROVES what landed on disk by its magic bytes and by reading the text back,
# rather than trusting the application's own report.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

WORK="$HOME/Documents/ai-os-pdf-editable-probe"
rm -rf "$WORK"
mkdir -p "$WORK"

SOURCE="$WORK/source.pdf"
TARGET="$WORK/converted.docx"

echo "== fixture"

# cupsfilter turns plain text into a PDF with a real text layer and needs no
# application, which is what makes the fixture's content known in advance.
printf 'Paragraph one, which must survive.\nParagraph two, which must also survive.\n' \
  > "$WORK/source.txt"

if ! /usr/sbin/cupsfilter "$WORK/source.txt" > "$SOURCE" 2>/dev/null; then
  echo "cupsfilter refused the fixture -- cannot probe"
  exit 1
fi

echo "made $SOURCE ($(wc -c < "$SOURCE") bytes)"

echo
echo "== 1. does Word open a PDF"

/usr/bin/osascript <<OSA
try
  tell application "Microsoft Word"
    activate
    -- A PDF is not a Word format, so this is the question: does it accept one
    -- and offer to convert, or does it refuse outright?
    open (POSIX file "$SOURCE" as alias)
    delay 3
    set n to count of documents
    return "opened, documents now: " & n
  end tell
on error message number code
  return "refused: " & message & " (" & code & ")"
end try
OSA

echo
echo "== 2. can it save that as .docx"

/usr/bin/osascript <<OSA
try
  tell application "Microsoft Word"
    if (count of documents) is 0 then return "no document is open"
    set d to active document
    save as d file name (POSIX path of "$TARGET") file format format document default
    close d saving no
    return "saved"
  end tell
on error message number code
  return "could not save: " & message & " (" & code & ")"
end try
OSA

echo
echo "== 3. what landed on disk"

if [ -f "$TARGET" ]; then
  head="$(/usr/bin/xxd -p -l 8 "$TARGET" | tr -d '\n')"
  case "$head" in
    504b*) echo "$head  ZIP, so a real .docx  ($(wc -c < "$TARGET") bytes)" ;;
    *)     echo "$head  NOT a .docx" ;;
  esac

  echo
  echo "-- text that came back:"
  /usr/bin/textutil -convert txt -stdout "$TARGET" 2>/dev/null | sed 's/^/   /'
else
  echo "nothing was written to $TARGET"
fi

echo
echo "== leftovers are in $WORK -- delete when done"
