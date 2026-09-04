#!/bin/bash
# Which file format does Word actually write for a .doc?
#
# `create_word_document` saves with `file format format document` whatever the
# path is called, so a .doc request gets DOCX bytes under a .doc name. Before
# fixing that by branching on the extension, this asks Word which enumeration
# name it accepts and PROVES what landed on disk by its magic bytes rather than
# by its file name:
#
#   .docx  is a ZIP        -> starts "PK"
#   .doc   is OLE2         -> starts D0 CF 11 E0 A1 B1 1A E1
#
# A file that is named .doc and starts with PK is exactly the defect.
#
# Named probe_ rather than verify_ so verify_all.sh does not pick it up.
set -u

WORK="$HOME/Documents/ai-os-word-format-probe"
rm -rf "$WORK"
mkdir -p "$WORK"

magic() {
  if [ ! -f "$1" ]; then
    printf 'MISSING'
    return
  fi
  /usr/bin/xxd -p -l 8 "$1" 2>/dev/null | tr -d '\n'
}

describe() {
  local file="$1" head
  head="$(magic "$file")"
  case "$head" in
    504b*)      printf '%s  ZIP (docx)          %s\n' "$head" "$file" ;;
    d0cf11e0*)  printf '%s  OLE2 (real .doc)    %s\n' "$head" "$file" ;;
    MISSING)    printf 'MISSING                         %s\n' "$file" ;;
    *)          printf '%s  something else      %s\n' "$head" "$file" ;;
  esac
}

echo "=== what the adapter does today: 'format document' onto a .doc path ==="
/usr/bin/osascript - "$WORK/today.doc" <<'APPLESCRIPT'
on run argv
    set outputPath to item 1 of argv
    tell application id "com.microsoft.Word"
        try
            set made to make new document
            set content of text object of made to "Format probe."
            save as made file name outputPath file format format document default add to recent files false
            close active document saving no
            return "saved with: format document"
        on error e number n
            try
                close active document saving no
            end try
            return "ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT
describe "$WORK/today.doc"

echo
echo "=== the old-format enumeration, one script per candidate ==="
echo "Two mistakes cost a round each, and both are worth writing down."
echo "Word.sdef names it 'format document97', with no space -- 'format"
echo "document 97' is a COMPILE error, and putting three candidates in ONE"
echo "script let that one kill the other two and made all three look"
echo "rejected. Then: 'format document default' is a SINGLE enumerator name."
echo "Reading it as 'format document' plus a 'default' parameter leaves a"
echo "stray identifier, which is the same syntax error wearing a disguise."
for enum in "format document97" "format template97" "format rtf"; do
  target="$WORK/$(printf '%s' "$enum" | tr ' ' '-').doc"
  printf -- '-- %s --\n' "$enum"
  /usr/bin/osascript -e "
    on run
        tell application id \"com.microsoft.Word\"
            try
                set made to make new document
                set content of text object of made to \"Format probe.\"
                save as made file name \"$target\" file format $enum add to recent files false
                close active document saving no
                return \"accepted\"
            on error e number n
                try
                    close active document saving no
                end try
                return \"ERROR \" & n & \" :: \" & e
            end try
        end tell
    end run
  " 2>&1
  describe "$target"
done

echo
echo "=== control: 'format document' onto a .docx path ==="
/usr/bin/osascript - "$WORK/control.docx" <<'APPLESCRIPT'
on run argv
    tell application id "com.microsoft.Word"
        try
            set made to make new document
            set content of text object of made to "Format probe."
            save as made file name (item 1 of argv) file format format document default add to recent files false
            close active document saving no
            return "accepted"
        on error e number n
            try
                close active document saving no
            end try
            return "ERROR " & n & " :: " & e
        end try
    end tell
end run
APPLESCRIPT
describe "$WORK/control.docx"

echo
echo "=== files ==="
ls -l "$WORK"
rm -rf "$WORK"
