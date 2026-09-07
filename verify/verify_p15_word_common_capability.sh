#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
RUN_REAL="${AI_OS_RUN_WORD_REAL_E2E:-0}"
LOG="/tmp/ai-os-word-verifier.log"

cd "$ROOT" || exit 1

run_check() {
  local label="$1"
  shift

  if "$@" >"$LOG" 2>&1; then
    printf '✓ %s\n' "$label"
  else
    printf '✗ %s\n' "$label"
    tail -40 "$LOG"
    printf 'FAIL Word Common Capability: %s\n' "$label"
    exit 1
  fi
}

run_external_check() {
  local label="$1"
  shift

  if "$@" >"$LOG" 2>&1; then
    printf '✓ %s\n' "$label"
    return 0
  fi

  if grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$LOG"; then
    printf 'SKIP Word real E2E: APPLICATION_AUTOMATION_UNAVAILABLE component=%s\n' "$label"
    exit 0
  fi

  printf '✗ %s\n' "$label"
  tail -60 "$LOG"
  printf 'FAIL Word Common Capability: %s\n' "$label"
  exit 1
}

# Core contract only.
run_check \
  "bounded Word unit contract" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::word::tests:: \
    -- \
    --skip word_readback_real_e2e \
    --skip word_pdf_export_real_e2e \
    --skip word_realistic_workflow_real_e2e \
    --skip word_writes_the_format_the_extension_promises_real_e2e

run_check \
  "document.edit confirmation gate" \
  cargo test \
    --manifest-path "$MANIFEST" \
    runtime::openclaw_permission::tests::document_edit_requires_and_accepts_one_time_user_confirmation \
    -- --exact

run_check \
  "document.convert confirmation gate" \
  cargo test \
    --manifest-path "$MANIFEST" \
    runtime::openclaw_permission::tests::document_convert_requires_and_accepts_one_time_user_confirmation \
    -- --exact

if [ "$RUN_REAL" != "1" ]; then
  printf 'PASS Word Common Capability\n'
  exit 0
fi

# Real Word automation belongs to External E2E.
VERSION="$(
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application "Microsoft Word" to get version' \
    -e 'end timeout' \
    2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP Word real E2E: APPLICATION_UNAVAILABLE_OR_UNRESPONSIVE"
  exit 0
fi

run_external_check \
  "Word create/read real E2E" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::word::tests::word_readback_real_e2e \
    -- --ignored --exact

run_external_check \
  "Word writes the format the extension promises" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::word::tests::word_writes_the_format_the_extension_promises_real_e2e \
    -- --ignored --exact

run_external_check \
  "Word isolated PDF export E2E" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::word::tests::word_pdf_export_real_e2e \
    -- --ignored --exact

run_external_check \
  "Word realistic edit/image/PDF E2E" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::word::tests::word_realistic_workflow_real_e2e \
    -- --ignored --exact

run_external_check \
  "Structured document agrees with Word" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::structured::tests::structured_and_word_agree_on_the_same_document \
    --lib -- --ignored --exact

printf 'PASS Word real E2E\n'
