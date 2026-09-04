#!/bin/bash
set -u

NAME="P15 PDF (read, recognise, merge, split, rotate, lock, annotate, fill)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/pdf.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# The Office skill could WRITE a PDF from every application it drives and READ
# none, which made PDF a one-way street: hand it a PDF and it could do nothing.
# Everything here but recognition is done in Rust, on the file itself, so it
# needs no application AND no particular operating system -- which is why the
# rotate and password tests below are ordinary tests rather than macOS ones.

cargo test --manifest-path "$MANIFEST" --lib document::pdf::tests \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "PDF request contract"
  }

grep -q "running 5 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "PDF contract test count changed"
}

echo "✅ page ranges read the way people write them, and a stray comma is tolerated"
echo "✅ paths fail closed before anything is opened, overwrite included"
echo "✅ a merge refuses a destination that is one of its own sources"
echo "✅ pages turn, relatively, and keep their words -- checked in the written file"
echo "✅ a password really locks the file: the words are gone from the bytes"
echo "✅ the wrong password is refused, not answered with an empty document"
echo "✅ the password is never echoed back into the result"
echo "✅ unlocking gives back a file that opens with nothing at all"
echo "✅ notes are left on the page asked for, and read back with their author"
echo "✅ a form reports its fields, and a widget is not mistaken for a remark"
echo "✅ filling sets the value AND the appearance state the document uses"
echo "✅ one wrong field name leaves no half-filled form behind"

cargo test --manifest-path "$MANIFEST" --lib \
  document::resolver::tests::every_capability_and_format_reaches_the_adapter_it_should \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "PDF routing"
  }

echo "✅ .pdf reaches the PDF adapter, and page, lock and form work are PDF-only"

cargo test --manifest-path "$MANIFEST" --lib \
  document::pdf::tests::pdf_is_read_split_merged_and_recognised_real_e2e \
  -- --ignored --exact >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "PDF real end to end"
  }

grep -q "test result: ok" "$LOG" || fail "PDF end-to-end result marker"

echo "✅ a text PDF is read from its text layer, and says it was"
echo "✅ a SCANNED page is recognised and says so, instead of reading as empty"
echo "✅ recognition can be turned off, and then the scan is honestly empty"
echo "✅ split takes the page asked for, proven by what the extracted file says"
echo "✅ merge concatenates, and refuses to overwrite against a real file"

echo "PASS $NAME"
