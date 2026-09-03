#!/bin/bash
set -u

NAME="P15 Structured Office Files (no application required)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
FIXTURE="$TMP_DIR/created.xlsx"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# This layer is what makes Office a capability rather than a set of
# per-application integrations. .xlsx is a ZIP of XML, so it is read directly --
# no Excel, no Numbers, no WPS. These first checks need no application at all.
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::structured:: \
  --lib >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "structured reader contract"
  }

grep -q "running 19 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "structured reader test count changed"
}

echo "✅ cell references placed by reference, not by document order"
echo "✅ XML entities decoded, rich-text runs joined"
echo "✅ worksheets resolved through relationships, a dangling one refused"
echo "✅ out-of-bounds cells truncate and say so, rather than growing the grid"
echo "✅ a file that is not an archive is refused, not read as empty"
echo "✅ writing escapes what would break the XML and keeps text that looks numeric"
echo "✅ writing refuses to overwrite, and leaves no part-file behind when it refuses"
echo "✅ a .docx is read without Word: paragraphs, tables, images, run formatting"
echo "✅ a .pptx is read without PowerPoint, in presentation order, tables included"
echo "✅ a broken package is refused, not reported as an empty document or deck"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::registry::tests::provider_order_and_local_first_policy_are_stable \
  --lib >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "provider floor contract"
  }

echo "✅ available on every machine, ranked below every installed application"
echo "✅ declares nothing it cannot execute"

# The agreement test needs Excel, because its whole point is that both paths
# read the same real Excel file identically.
VERSION="$(
  /usr/bin/osascript -e 'tell application "Microsoft Excel" to get version' 2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP $NAME: Excel unavailable, cross-check not run"
  echo "PASS $NAME"
  exit 0
fi

KEEP_FIXTURE=1 \
KEEP_FIXTURE_PATH="$FIXTURE" \
bash verify/verify_p15_spreadsheet_create.sh \
  >"$TMP_DIR/create.log" 2>&1 || {
    tail -40 "$TMP_DIR/create.log"
    fail "Spreadsheet Create fixture"
  }

[ -s "$FIXTURE" ] || fail "Preserved XLSX fixture"

AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::structured::tests::structured_and_excel_agree_on_the_same_workbook \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "Excel agreement"
  }

grep -q "test result: ok" "$LOG" || fail "Excel agreement result marker"

echo "✅ a workbook Excel wrote reads identically WITHOUT Excel"
echo "✅ escaping, booleans and numbers survive the file round trip"
echo "✅ worksheet identity agrees with what Excel reports"

AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::structured::tests::excel_can_open_what_the_structured_layer_wrote \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "Excel opens what this layer wrote"
  }

grep -q "test result: ok" "$LOG" || fail "Excel open result marker"

echo "✅ a workbook written WITHOUT Excel opens in Excel and reads back correctly"

# The document and presentation halves of the same claim: both providers answer
# the same capability, so a caller must not be able to tell which one answered.
if /usr/bin/osascript -e 'tell application "Microsoft Word" to get version' >/dev/null 2>&1; then
  cargo test \
    --manifest-path src-tauri/Cargo.toml \
    document::structured::tests::structured_and_word_agree_on_the_same_document \
    --lib -- --ignored >"$LOG" 2>&1 || {
      tail -100 "$LOG"
      fail "Word agreement"
    }

  grep -q "test result: ok" "$LOG" || fail "Word agreement result marker"

  echo "✅ a document Word wrote reads identically WITHOUT Word"
  echo "✅ heading formatting, image and table cells agree across both providers"
else
  echo "SKIP $NAME: Word unavailable, document cross-check not run"
fi

if /usr/bin/osascript -e 'tell application "Microsoft PowerPoint" to get version' >/dev/null 2>&1; then
  cargo test \
    --manifest-path src-tauri/Cargo.toml \
    document::structured::tests::structured_and_powerpoint_agree_on_the_same_presentation \
    --lib -- --ignored >"$LOG" 2>&1 || {
      tail -100 "$LOG"
      fail "PowerPoint agreement"
    }

  grep -q "test result: ok" "$LOG" || fail "PowerPoint agreement result marker"

  echo "✅ a deck PowerPoint wrote reads identically WITHOUT PowerPoint"
  echo "✅ slide order, titles, bodies and table cells agree across both providers"
else
  echo "SKIP $NAME: PowerPoint unavailable, presentation cross-check not run"
fi

echo "PASS $NAME"
