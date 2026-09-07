#!/bin/bash
set -u

NAME="P15 Structured Office Files (no application required)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
FIXTURE="$TMP_DIR/created.xlsx"
LOG="$TMP_DIR/test.log"
RUN_REAL="${AI_OS_RUN_STRUCTURED_OFFICE_REAL_E2E:-0}"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

external_fail_or_skip() {
  local label="$1"
  local log="$2"

  if grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$log"; then
    echo "SKIP $NAME: APPLICATION_AUTOMATION_UNAVAILABLE component=$label"
    exit 0
  fi

  tail -100 "$log"
  fail "$label"
}

echo "$NAME"

# Core contract: these capabilities work directly on OOXML packages and do
# not require Excel, Word, PowerPoint, Numbers, Pages, WPS or any cloud account.

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
echo "✅ out-of-bounds cells truncate rather than growing the grid"
echo "✅ a file that is not an archive is refused, not read as empty"
echo "✅ writing escapes XML and preserves numeric-looking text"
echo "✅ writing refuses overwrite and leaves no partial file"
echo "✅ .docx is read without Word"
echo "✅ .pptx is read without PowerPoint"
echo "✅ broken packages are refused"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::registry::tests::provider_order_and_local_first_policy_are_stable \
  --lib >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "provider floor contract"
  }

echo "✅ portable structured provider remains below installed native applications"
echo "✅ provider declares only executable capabilities"

# Real Office application agreement belongs to External E2E.
if [ "$RUN_REAL" != "1" ]; then
  echo "PASS $NAME"
  exit 0
fi

echo
echo "External interop mode"

RAN=0

# Excel interop
VERSION="$(
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application "Microsoft Excel" to get version' \
    -e 'end timeout' \
    2>/dev/null || true
)"

if [ -n "$VERSION" ]; then
  RAN=1

  if ! KEEP_FIXTURE=1 \
    KEEP_FIXTURE_PATH="$FIXTURE" \
    bash verify/verify_p15_spreadsheet_create.sh \
    >"$TMP_DIR/create.log" 2>&1
  then
    external_fail_or_skip "Spreadsheet Create fixture" "$TMP_DIR/create.log"
  fi

  [ -s "$FIXTURE" ] || fail "Preserved XLSX fixture"

  if ! AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
    cargo test \
      --manifest-path src-tauri/Cargo.toml \
      document::structured::tests::structured_and_excel_agree_on_the_same_workbook \
      --lib -- --ignored --exact >"$LOG" 2>&1
  then
    external_fail_or_skip "Excel agreement" "$LOG"
  fi

  grep -q "test result: ok" "$LOG" || fail "Excel agreement result marker"

  echo "✅ structured reader agrees with Excel"

  if ! AI_OS_EXCEL_EDIT_FIXTURE="$FIXTURE" \
    cargo test \
      --manifest-path src-tauri/Cargo.toml \
      document::structured::tests::excel_can_open_what_the_structured_layer_wrote \
      --lib -- --ignored --exact >"$LOG" 2>&1
  then
    external_fail_or_skip "Excel opens structured output" "$LOG"
  fi

  grep -q "test result: ok" "$LOG" || fail "Excel open result marker"

  echo "✅ Excel opens structured output"
else
  echo "SKIP Excel interop: APPLICATION_UNAVAILABLE"
fi

# PowerPoint interop
VERSION="$(
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application "Microsoft PowerPoint" to get version' \
    -e 'end timeout' \
    2>/dev/null || true
)"

if [ -n "$VERSION" ]; then
  RAN=1

  if ! cargo test \
    --manifest-path src-tauri/Cargo.toml \
    document::structured::tests::structured_and_powerpoint_agree_on_the_same_presentation \
    --lib -- --ignored --exact >"$LOG" 2>&1
  then
    external_fail_or_skip "PowerPoint agreement" "$LOG"
  fi

  grep -q "test result: ok" "$LOG" || fail "PowerPoint agreement result marker"

  echo "✅ structured presentation agrees with PowerPoint"
else
  echo "SKIP PowerPoint interop: APPLICATION_UNAVAILABLE"
fi

if [ "$RAN" -eq 0 ]; then
  echo "SKIP $NAME: OFFICE_APPLICATIONS_UNAVAILABLE"
  exit 0
fi

echo "PASS $NAME"
