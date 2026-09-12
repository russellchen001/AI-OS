#!/bin/bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="${TMPDIR:-/tmp}/ai-os-generative-media-closure.log"
PASS=0
FAIL=0

ok() { printf '✓ %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '✗ %s\n' "$1"; FAIL=$((FAIL + 1)); tail -40 "$LOG" 2>/dev/null || true; }

run() {
  local label="$1"
  shift
  if "$@" >"$LOG" 2>&1; then
    ok "$label"
  else
    bad "$label"
  fi
}

printf '%s\n' '============================================================'
printf '%s\n' 'VERIFY P15 GENERATIVE MEDIA CLOSURE'
printf '%s\n' '============================================================'

run 'GM-0/GM-1 domain, Provider Registry, router and exact-selection contracts' \
  cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib generative_media::domain::tests
run 'GM-2 Ready detection, Local First, real ComfyUI generation and opaque output' \
  bash "$ROOT/verify/verify_p15_gm_2_final_provider_integration.sh"
run 'GM-3 explicit Setup/Repair, integrity and real Ready boundary' \
  bash "$ROOT/verify/verify_p15_gm_3_setup_repair.sh"
run 'GM-4 cloud auth/account/budget boundary without paid opt-in' \
  bash "$ROOT/verify/verify_p15_gm_4_cloud_providers.sh"
run 'GM-5 Prompt/Reference Intelligence deterministic closure' \
  env AI_OS_SKIP_AR_AGGREGATE=1 bash "$ROOT/verify/verify_p15_gm_5_prompt_reference_intelligence.sh"
run 'GM-6 bounded Quality Loop deterministic closure' \
  env AI_OS_SKIP_AR_AGGREGATE=1 bash "$ROOT/verify/verify_p15_gm_6_quality_loop.sh"
run 'AR-1A Agent/Skill contract and Runtime permission boundary' \
  bash "$ROOT/verify/verify_ar1a_agent_skill_contract.sh"
run 'AR-1B Agent-owned execution and no direct Runtime media dispatch' \
  bash "$ROOT/verify/verify_ar1b_agent_owned_execution.sh"
run 'AR-1C Agent Skill transport, gateway governance and real OpenClaw E2E' \
  bash "$ROOT/verify/verify_ar1c_agent_skill_transport.sh"
run 'full Generative Media Rust regression' \
  cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib generative_media::
run 'repository diff integrity' git -C "$ROOT" diff --check

printf '%s\n' '============================================================'
printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
printf '%s\n' '============================================================'

if [[ "$FAIL" -ne 0 ]]; then
  printf '%s\n' 'FAIL P15 GENERATIVE MEDIA CLOSURE'
  exit 1
fi

printf '%s\n' 'PASS P15 GENERATIVE MEDIA CLOSURE'
