#!/bin/bash
set -u

NAME="P15 Apple Pages Document Capability"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# The bundle identifier is com.apple.Pages. com.apple.iWork.Pages does not
# resolve, which is worth failing loudly on rather than reporting as "not
# installed".
VERSION="$(
  /usr/bin/osascript -e 'tell application id "com.apple.Pages" to get version' 2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP $NAME: APP_NOT_INSTALLED_OR_AUTOMATION_DENIED:Pages"
  exit 0
fi

echo "✅ Pages $VERSION answers AppleScript"

BEFORE_DOCS="$(
  /usr/bin/osascript -e 'tell application id "com.apple.Pages" to return count of documents'
)" || fail "Pages preflight state"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_paths_fail_closed_on_shape_existence_and_overwrite \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "path contract"
  }

echo "✅ absolute paths, correct extension, existence and no-overwrite"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_read_parser_refuses_a_short_payload \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "read parser contract"
  }

echo "✅ a payload shorter than it declared is refused, not reported as complete"

# Every AppleScript form below was proved by
# verify/probe_iwork_pages_numbers_semantics.sh before it was written.
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::pages::tests::pages_document_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real Pages E2E"
  }

grep -q "test result: ok" "$LOG" || fail "real Pages E2E result marker"

echo "✅ document.create wrote a real .pages file"
echo "✅ document.read reopened it and returned its paragraphs, words and characters"
echo "✅ document.convert exported a real PDF"
echo "✅ creating over an existing document is refused"

AFTER_DOCS="$(
  /usr/bin/osascript -e 'tell application id "com.apple.Pages" to return count of documents'
)" || fail "Pages final state"

[ "$BEFORE_DOCS" = "$AFTER_DOCS" ] ||
  fail "Pages was left holding documents ($BEFORE_DOCS -> $AFTER_DOCS)"

echo "✅ Pages remains running"
echo "✅ no document left open"

echo "PASS $NAME"
