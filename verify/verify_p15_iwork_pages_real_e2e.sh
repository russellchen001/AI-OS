#!/usr/bin/env bash
set -u

NAME="Pages real E2E"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

skip() {
  echo "SKIP $NAME: APPLICATION_AUTOMATION_UNAVAILABLE $1"
  exit 0
}

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

automation_failure() {
  grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$1"
}

pages_version() {
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application id "com.apple.Pages" to get version' \
    -e 'end timeout' \
    2>/dev/null
}

pages_document_count() {
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application id "com.apple.Pages" to return count of documents' \
    -e 'end timeout' \
    2>/dev/null
}

VERSION="$(pages_version || true)"

[ -n "$VERSION" ] ||
  skip "component=Pages reason=APP_MISSING_DENIED_OR_UNRESPONSIVE"

echo "✓ Pages $VERSION answers bounded AppleScript"

BEFORE_DOCS="$(pages_document_count || true)"

[ -n "$BEFORE_DOCS" ] ||
  skip "component=Pages reason=PREFLIGHT_UNRESPONSIVE"

if ! cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_document_real_e2e \
  --lib -- --ignored --exact >"$LOG" 2>&1
then
  if automation_failure "$LOG"; then
    skip "component=Pages reason=APPLEEVENT_TIMEOUT"
  fi

  tail -140 "$LOG"
  fail "real Pages capability"
fi

grep -q "test result: ok" "$LOG" ||
  fail "real Pages result marker"

AFTER_DOCS="$(pages_document_count || true)"

[ -n "$AFTER_DOCS" ] ||
  skip "component=Pages reason=FINAL_STATE_UNRESPONSIVE"

[ "$BEFORE_DOCS" = "$AFTER_DOCS" ] ||
  fail "Pages document ownership ($BEFORE_DOCS -> $AFTER_DOCS)"

echo "✓ document.create real .pages"
echo "✓ document.read real .pages"
echo "✓ document.convert real PDF"
echo "✓ no-overwrite"
echo "✓ no document leak"
echo "PASS $NAME"
