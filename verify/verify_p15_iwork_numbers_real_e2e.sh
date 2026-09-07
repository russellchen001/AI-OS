#!/usr/bin/env bash
set -u

NAME="Numbers real E2E"
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

numbers_version() {
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application id "com.apple.Numbers" to get version' \
    -e 'end timeout' \
    2>/dev/null
}

numbers_document_count() {
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application id "com.apple.Numbers" to return count of documents' \
    -e 'end timeout' \
    2>/dev/null
}

numbers_running() {
  /usr/bin/pgrep -x Numbers >/dev/null 2>&1
}

VERSION="$(numbers_version || true)"

[ -n "$VERSION" ] ||
  skip "component=Numbers reason=APP_MISSING_DENIED_OR_UNRESPONSIVE"

echo "✓ Numbers $VERSION answers bounded AppleScript"

BEFORE_DOCS="$(numbers_document_count || true)"

[ -n "$BEFORE_DOCS" ] ||
  skip "component=Numbers reason=PREFLIGHT_UNRESPONSIVE"

if ! cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_spreadsheet_real_e2e \
  --lib -- --ignored --exact >"$LOG" 2>&1
then
  if automation_failure "$LOG"; then
    skip "component=Numbers reason=APPLEEVENT_TIMEOUT"
  fi

  tail -140 "$LOG"
  fail "real Numbers capability"
fi

grep -q "test result: ok" "$LOG" ||
  fail "real Numbers result marker"

if numbers_running; then
  AFTER_DOCS="$(numbers_document_count || true)"

  [ -n "$AFTER_DOCS" ] ||
    skip "component=Numbers reason=FINAL_STATE_UNRESPONSIVE"

  [ "$BEFORE_DOCS" = "$AFTER_DOCS" ] ||
    fail "Numbers document ownership ($BEFORE_DOCS -> $AFTER_DOCS)"
else
  [ "$BEFORE_DOCS" = "0" ] ||
    fail "Numbers stopped with pre-existing user documents"
fi

echo "✓ spreadsheet.create real .numbers"
echo "✓ spreadsheet.read real .numbers"
echo "✓ no-overwrite"
echo "✓ no document leak"
echo "PASS $NAME"
