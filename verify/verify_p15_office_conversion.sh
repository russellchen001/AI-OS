#!/bin/bash
set -u

NAME="P15 Office Conversion (cross-suite and PDF)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/office-conversion.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# The owner settled what convert means, and it is two things: cross-suite
# format conversion so an iWork application can be handed an Office file and an
# Office application an iWork file, and PDF export from any of them. The
# DESTINATION decides which one a request is.

cargo test --manifest-path "$MANIFEST" --lib \
  document::iwork_convert::tests \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "conversion request contract"
  }

grep -q "running 6 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "conversion contract test count changed"
}

echo "✅ the destination decides: .pdf is an export, anything else a conversion"
echo "✅ a format the application does not handle is refused by name"
echo "✅ paths fail closed before any application is asked, overwrite included"

# One filter, not two: cargo test takes a single TESTNAME. The shared prefix
# matches both routing tables, and the count guard is what makes sure it still
# does.
cargo test --manifest-path "$MANIFEST" --lib \
  document::resolver::tests::conversion \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "conversion routing table"
  }

grep -q "running 2 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "conversion routing table count changed"
}

echo "✅ every (source, destination) pair reaches the adapter that can do it"
echo "✅ Word wins only the conversions it performs, and .docx to .doc is still macOS's"
echo "✅ PDF back into a document routes to Word -- the only thing here that reads one"
echo "✅ without Word there is NO route out of a PDF, rather than one that would refuse"
echo "✅ what no installed application can do says so, rather than routing anyway"

# Everything above needs no application. Everything below is the real evidence.
have() {
  /usr/bin/osascript -e "tell application \"$1\" to get version" >/dev/null 2>&1
}

if have "Pages"; then
  cargo test --manifest-path "$MANIFEST" --lib \
    document::iwork_convert::tests::pages_converts_both_directions_and_exports_pdf_real_e2e \
    -- --ignored >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Pages conversion"
    }
  grep -q "test result: ok" "$LOG" || fail "Pages conversion result marker"
  echo "✅ Pages: a .docx becomes a .pages, back to a .docx with its text intact, and a PDF"
  echo "✅ Pages refuses to overwrite against the real application, not only in the path checks"
else
  echo "SKIP $NAME: Pages unavailable"
fi

if have "Numbers"; then
  cargo test --manifest-path "$MANIFEST" --lib \
    document::iwork_convert::tests::numbers_converts_both_directions_and_exports_pdf_real_e2e \
    -- --ignored >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Numbers conversion"
    }
  grep -q "test result: ok" "$LOG" || fail "Numbers conversion result marker"
  echo "✅ Numbers: a .xlsx written with NO application becomes a .numbers, back, and a PDF"
else
  echo "SKIP $NAME: Numbers unavailable"
fi

if have "Keynote"; then
  cargo test --manifest-path "$MANIFEST" --lib \
    document::iwork_convert::tests::keynote_converts_both_directions_and_exports_pdf_real_e2e \
    -- --ignored >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Keynote conversion"
    }
  grep -q "test result: ok" "$LOG" || fail "Keynote conversion result marker"
  echo "✅ Keynote: a .key becomes a .pptx carrying its title, back to a .key, and a PDF"
  echo "✅ proven without PowerPoint installed anywhere in the path"
else
  echo "SKIP $NAME: Keynote unavailable"
fi

if have "Microsoft Word"; then
  cargo test --manifest-path "$MANIFEST" --lib \
    document::word::tests::word_converts_a_pdf_back_into_a_document_real_e2e \
    -- --ignored >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Word PDF import"
    }
  grep -q "test result: ok" "$LOG" || fail "Word PDF import result marker"
  echo "✅ Word turns a PDF back into a real .docx -- proven by its ZIP magic bytes"
  echo "✅ the sentence that went in comes back out, and the layout loss is admitted"
else
  echo "SKIP $NAME: Word unavailable"
fi

if have "Microsoft Excel"; then
  cargo test --manifest-path "$MANIFEST" --lib \
    document::excel::tests::excel_exports_a_pdf_real_e2e \
    -- --ignored >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Excel PDF export"
    }
  grep -q "test result: ok" "$LOG" || fail "Excel PDF export result marker"
  echo "✅ Excel writes a real PDF itself, leaves the workbook unchanged, and refuses to overwrite"
else
  echo "SKIP $NAME: Excel unavailable"
fi

echo "PASS $NAME"
