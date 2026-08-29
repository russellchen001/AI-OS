#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ai-os-provider-policy.XXXXXX")"
trap 'rm -rf "$LOG_DIR"' EXIT

run_check() {
  local key="$1" label="$2"
  shift 2
  if "$@" >"$LOG_DIR/$key.log" 2>&1; then
    echo "✓ $label"
  else
    echo "✗ $label"
    tail -20 "$LOG_DIR/$key.log" || true
    echo "FAIL P15 provider selection and authorization: $label"
    exit 1
  fi
}

run_check contract "Provider selection, authorization, locality and secret boundaries" \
  cargo test --manifest-path "$MANIFEST" provider_selection::tests
run_check office "Microsoft Graph foundation and Native Excel ownership" \
  cargo test --manifest-path "$MANIFEST" document::registry::tests
run_check browser "Authenticated Browser session metadata excludes secrets" \
  cargo test --manifest-path "$MANIFEST" browser::provider::tests
run_check permission "Granted macOS permission is reused" \
  cargo test --manifest-path "$MANIFEST" macos_permissions::tests
run_check spreadsheet_read "Spreadsheet Read real Excel E2E" \
  bash "$ROOT/verify/verify_p15_spreadsheet_read.sh"
run_check spreadsheet_create "Spreadsheet Create real read-back E2E" \
  bash "$ROOT/verify/verify_p15_spreadsheet_create.sh"
run_check alignment "Real-World Skill Alignment regression" \
  bash "$ROOT/verify/verify_p15_real_world_skill_alignment.sh"

echo "PASS P15 provider selection and authorization"
exit 0
