#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ai-os-p15-alignment.XXXXXX")"
trap 'rm -rf "$LOG_DIR"' EXIT

run_check() {
  local key="$1"
  local label="$2"
  shift 2
  local log="$LOG_DIR/$key.log"
  if "$@" >"$log" 2>&1; then
    echo "✓ $label"
  else
    echo "✗ $label"
    tail -20 "$log" || true
    echo "FAIL P15 real-world skill alignment: $label"
    exit 1
  fi
}

run_check browser "Browser discovery and verified evidence states" \
  cargo test --manifest-path "$MANIFEST" browser_evidence_distinguishes_discovery_and_verified_states
run_check planner "Evidence freshness, Timing dependencies and Distill output" \
  cargo test --manifest-path "$MANIFEST" stores_evidence_timing_dependencies_and_distill_output
run_check closure "TaskClosureStage compatibility and confirmation regression" \
  bash "$ROOT/verify/verify_p15_task_closure_contract.sh"
run_check download "Download completed requires a real destination file" \
  bash "$ROOT/verify/verify_p15_download_complete.sh"
run_check file_write "Filesystem write post-action validation" \
  bash "$ROOT/verify/verify_p15_file_write.sh"
run_check file_move "Filesystem move post-action validation" \
  bash "$ROOT/verify/verify_p15_file_move.sh"
run_check models "Local model lifecycle validation contract" \
  bash "$ROOT/verify/verify_p15_local_model_core_skill.sh"
run_check calendar "Calendar create validation contract" \
  bash "$ROOT/verify/verify_p15_email_calendar_create.sh"
run_check mail "Mail send confirmation contract" \
  bash "$ROOT/verify/verify_p15_email_calendar_send_confirmation.sh"
run_check document_create "Document create post-action validation" \
  bash "$ROOT/verify/verify_p15_document_create.sh"
run_check document_convert "Document convert post-action validation" \
  bash "$ROOT/verify/verify_p15_document_convert.sh"
run_check spreadsheet_create "Spreadsheet create real read-back validation" \
  bash "$ROOT/verify/verify_p15_spreadsheet_create.sh"
run_check spreadsheet_read "Spreadsheet read real Excel E2E" \
  bash "$ROOT/verify/verify_p15_spreadsheet_read.sh"

echo "PASS P15 real-world skill alignment"
exit 0
