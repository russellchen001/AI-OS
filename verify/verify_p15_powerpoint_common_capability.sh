#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="/tmp/ai-os-powerpoint-verifier.log"

cd "$ROOT" || exit 1

run_check() {
  local label="$1"
  shift

  if "$@" >"$LOG" 2>&1; then
    printf '✓ %s\n' "$label"
  else
    printf '✗ %s\n' "$label"
    tail -60 "$LOG"
    printf 'FAIL PowerPoint Common Capability: %s\n' "$label"
    exit 1
  fi
}

run_check \
  "bounded PowerPoint unit contract" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::powerpoint::tests::validation_is_bounded_and_fail_closed \
    -- --exact

run_check \
  "PowerPoint format-aware resolver" \
  cargo test \
    --manifest-path "$MANIFEST" \
    document::resolver::tests::local_pptx_routes_to_executable_powerpoint_not_declared_only_fallback \
    -- --exact

run_check \
  "presentation edit/export confirmation gate" \
  cargo test \
    --manifest-path "$MANIFEST" \
    runtime::openclaw_permission::tests::presentation_edit_and_export_require_one_time_user_confirmation \
    -- --exact

printf 'PASS PowerPoint Common Capability\n'
