#!/bin/bash
set -u

NAME="P15 Apple Numbers Spreadsheet Capability"
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

# The bundle identifier is com.apple.Numbers; com.apple.iWork.Numbers does not
# resolve.
NUMBERS_BUNDLE_ID="com.apple.Numbers"

VERSION="$(
  /usr/bin/osascript -e 'tell application id "com.apple.Numbers" to get version' 2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP $NAME: APP_NOT_INSTALLED_OR_AUTOMATION_DENIED:Numbers"
  exit 0
fi

echo "✅ Numbers $VERSION answers AppleScript"

numbers_running() {
  /usr/bin/pgrep -x Numbers >/dev/null 2>&1
}

numbers_document_count() {
  /usr/bin/osascript \
    -e 'tell application id "com.apple.Numbers" to return count of documents' \
    2>/dev/null
}

ensure_numbers_ready() {
  /usr/bin/open -gj -b "$NUMBERS_BUNDLE_ID" >/dev/null 2>&1 || return 1

  local attempt=0
  while [ "$attempt" -lt 120 ]; do
    if numbers_document_count >/dev/null 2>&1; then
      return 0
    fi

    /bin/sleep 0.25
    attempt=$((attempt + 1))
  done

  return 1
}

ensure_numbers_ready || fail "Numbers launch/readiness"

BEFORE_DOCS="$(numbers_document_count)" ||
  fail "Numbers preflight state"

echo "✅ Numbers application readiness established"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_paths_and_create_bounds_fail_closed \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "path contract"
  }

echo "✅ absolute paths, .numbers extension, no-overwrite, bounded content"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_trims_the_blank_grid_a_new_table_starts_with \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "grid trimming contract"
  }

echo "✅ the 22x7 of blanks a new table starts with is trimmed away"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_read_parser_returns_a_trimmed_single_sheet \
  --lib >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "read parser contract"
  }

echo "✅ localized sheet names reported, never used to address anything"

# Every AppleScript form below was proved by
# verify/probe_iwork_pages_numbers_semantics.sh before it was written.
cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::numbers::tests::numbers_spreadsheet_real_e2e \
  --lib -- --ignored >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "real Numbers E2E"
  }

grep -q "test result: ok" "$LOG" || fail "real Numbers E2E result marker"

echo "✅ spreadsheet.create wrote a real .numbers file and grew the table to fit"
echo "✅ spreadsheet.read reopened it and returned the content, trimmed"
echo "✅ single sheet, declared as the provider limit it is"
echo "✅ creating over an existing spreadsheet is refused"

if numbers_running; then
  AFTER_DOCS="$(numbers_document_count)" ||
    fail "Numbers final state"

  [ "$BEFORE_DOCS" = "$AFTER_DOCS" ] ||
    fail "Numbers was left holding documents ($BEFORE_DOCS -> $AFTER_DOCS)"

  echo "✅ Numbers is still running and its document count was restored"
else
  # An application with no documents may be automatically terminated by macOS.
  # That is not the adapter quitting it. If there were pre-existing documents,
  # however, disappearance would be a real ownership failure.
  [ "$BEFORE_DOCS" = "0" ] ||
    fail "Numbers stopped after starting with $BEFORE_DOCS pre-existing documents"

  echo "✅ Numbers had no document leak before natural application termination"
fi

echo "✅ verifier never requires the Numbers process to remain alive"
echo "✅ no document left open"

echo "PASS $NAME"
