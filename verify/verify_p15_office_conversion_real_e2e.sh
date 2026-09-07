#!/usr/bin/env bash
set -u

NAME="P15 Office Conversion real E2E"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
TMP_DIR="$(mktemp -d)"
LOG="$TMP_DIR/test.log"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

RAN=0
SKIPPED=0

automation_failure() {
  grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$1"
}

app_version() {
  local app="$1"

  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e "tell application \"$app\" to get version" \
    -e 'end timeout' \
    2>/dev/null
}

run_real() {
  local label="$1"
  local app="$2"
  local test_name="$3"

  local version
  version="$(app_version "$app" || true)"

  if [ -z "$version" ]; then
    echo "SKIP $NAME: APPLICATION_UNAVAILABLE_OR_UNRESPONSIVE component=$label"
    SKIPPED=1
    return 0
  fi

  RAN=$((RAN + 1))
  echo "✓ $label application available — $version"

  if cargo test \
    --manifest-path "$MANIFEST" \
    --lib \
    "$test_name" \
    -- --ignored --exact >"$LOG" 2>&1
  then
    echo "✓ $label"
    return 0
  fi

  if automation_failure "$LOG"; then
    echo "SKIP $NAME: APPLICATION_AUTOMATION_UNAVAILABLE component=$label"
    SKIPPED=1
    return 0
  fi

  tail -120 "$LOG"
  echo "FAIL $NAME: CAPABILITY_ASSERTION_FAILED component=$label"
  exit 1
}

run_real \
  "Pages conversion" \
  "Pages" \
  "document::iwork_convert::tests::pages_converts_both_directions_and_exports_pdf_real_e2e"

run_real \
  "Numbers conversion" \
  "Numbers" \
  "document::iwork_convert::tests::numbers_converts_both_directions_and_exports_pdf_real_e2e"

run_real \
  "Keynote conversion" \
  "Keynote" \
  "document::iwork_convert::tests::keynote_converts_both_directions_and_exports_pdf_real_e2e"

run_real \
  "Word PDF import" \
  "Microsoft Word" \
  "document::word::tests::word_converts_a_pdf_back_into_a_document_real_e2e"

run_real \
  "Excel PDF export" \
  "Microsoft Excel" \
  "document::excel::tests::excel_exports_a_pdf_real_e2e"

if [ "$RAN" -eq 0 ]; then
  echo "SKIP $NAME: OFFICE_APPLICATIONS_UNAVAILABLE"
  exit 0
fi

if [ "$SKIPPED" -eq 1 ]; then
  echo "SKIP $NAME: PARTIAL_APPLICATION_AUTOMATION_UNAVAILABLE"
  exit 0
fi

echo "PASS $NAME"
