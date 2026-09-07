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

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  document::iwork_convert::tests \
  >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "conversion request contract"
  }

grep -q "running 6 tests" "$LOG" || {
  tail -30 "$LOG"
  fail "conversion contract test count changed"
}

echo "✅ destination decides export vs conversion"
echo "✅ unsupported formats are refused"
echo "✅ path and overwrite checks fail closed"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  document::resolver::tests::conversion \
  >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "conversion routing table"
  }

grep -q "running 2 tests" "$LOG" || {
  tail -30 "$LOG"
  fail "conversion routing table count changed"
}

echo "✅ conversion routing table"
echo "✅ Word and macOS conversion ownership"
echo "✅ PDF editable conversion routing"
echo "✅ unsupported installed-provider combinations fail closed"

echo "PASS $NAME"
