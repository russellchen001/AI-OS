#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"

run_check() {
  local label="$1"
  shift
  if "$@" >/tmp/ai-os-word-verifier.log 2>&1; then
    printf '✓ %s\n' "$label"
  else
    printf '✗ %s\n' "$label"
    tail -40 /tmp/ai-os-word-verifier.log
    printf 'FAIL Word Common Capability: %s\n' "$label"
    exit 1
  fi
}

run_check "bounded Word unit contract" cargo test --manifest-path "$MANIFEST" document::word::tests:: -- --skip word_readback_real_e2e --skip word_pdf_export_real_e2e --skip word_realistic_workflow_real_e2e
run_check "Word create/read real E2E" cargo test --manifest-path "$MANIFEST" document::word::tests::word_readback_real_e2e -- --ignored --exact
run_check "Word isolated PDF export E2E" cargo test --manifest-path "$MANIFEST" document::word::tests::word_pdf_export_real_e2e -- --ignored --exact
run_check "Word realistic edit/image/PDF E2E" cargo test --manifest-path "$MANIFEST" document::word::tests::word_realistic_workflow_real_e2e -- --ignored --exact
run_check "document.edit confirmation gate" cargo test --manifest-path "$MANIFEST" runtime::openclaw_permission::tests::document_edit_requires_and_accepts_one_time_user_confirmation -- --exact
run_check "document.convert confirmation gate" cargo test --manifest-path "$MANIFEST" runtime::openclaw_permission::tests::document_convert_requires_and_accepts_one_time_user_confirmation -- --exact

printf 'PASS Word Common Capability\n'
