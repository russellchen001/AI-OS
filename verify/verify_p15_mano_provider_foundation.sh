#!/bin/bash

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEST_OUTPUT="$(mktemp -t ai-os-mp0-tests.XXXXXX)"
AR_OUTPUT="$(mktemp -t ai-os-mp0-ar.XXXXXX)"
trap 'rm -f "$TEST_OUTPUT" "$AR_OUTPUT"' EXIT

PASS_COUNT=0
FAIL_COUNT=0

pass() {
  printf '✓ %s\n' "$1"
  PASS_COUNT=$((PASS_COUNT + 1))
}

fail() {
  printf '✗ %s\n' "$1"
  FAIL_COUNT=$((FAIL_COUNT + 1))
}

require_test() {
  local test_name="$1"
  local label="$2"
  if grep -Eq "test .*::${test_name} \.\.\. ok" "$TEST_OUTPUT"; then
    pass "$label"
  else
    fail "$label"
  fi
}

if ! (cd "$ROOT_DIR/src-tauri" && cargo test --lib mp0_ -- --nocapture) >"$TEST_OUTPUT" 2>&1; then
  tail -40 "$TEST_OUTPUT"
  printf 'FAIL: P15 MP-0 Computer Use Provider Foundation tests did not complete\n'
  exit 1
fi

require_test \
  mp0_computer_use_execute_is_registered_as_one_skill_capability \
  'computer.use.execute is registered as Skill capability'
require_test \
  mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider \
  'selected Agent reaches Computer Use through Agent Skill Transport'
require_test \
  mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider \
  'SkillInvocationGateway is crossed'
require_test \
  mp0_planner_dispatches_computer_use_to_selected_agent \
  'Planner does not directly invoke Computer Use provider'
require_test \
  mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider \
  'Runtime does not directly invoke Computer Use provider'
require_test \
  mp0_gateway_blocks_unconfirmed_unexposed_and_trusted_bypass \
  'unconfirmed invocation does not reach provider'
require_test \
  mp0_gateway_blocks_unconfirmed_unexposed_and_trusted_bypass \
  'unexposed capability does not reach provider'
require_test \
  mp0_agent_cannot_choose_provider \
  'Agent cannot choose provider'
require_test \
  mp0_agent_cannot_choose_cloud_or_local_mode \
  'Agent cannot choose cloud/local execution mode'
require_test \
  mp0_version_metadata_does_not_determine_compatibility \
  'version metadata does not determine compatibility'
require_test \
  mp0_missing_required_capability_rejects_provider \
  'missing required capability rejects provider'
require_test \
  mp0_progress_contract_is_bounded_and_emitted \
  'progress contract works'
require_test \
  mp0_mock_provider_simulates_cancellation \
  'cancellation contract works'
require_test \
  mp0_provider_error_is_normalized \
  'normalized provider errors work'
require_test \
  mp0_system_capabilities_are_not_captured_by_computer_use \
  'system capability is not captured by Computer Use'
require_test \
  mp0_browser_capabilities_remain_on_existing_browser_path \
  'browser capability is not captured by Computer Use'
require_test \
  mp0_gateway_blocks_unconfirmed_unexposed_and_trusted_bypass \
  'Trusted Automation cannot silently authorize Computer Use'

if bash "$ROOT_DIR/verify/verify_ar1a_agent_skill_contract.sh" >"$AR_OUTPUT" 2>&1 \
  && bash "$ROOT_DIR/verify/verify_ar1b_agent_owned_execution.sh" >>"$AR_OUTPUT" 2>&1 \
  && bash "$ROOT_DIR/verify/verify_ar1c_agent_skill_transport.sh" >>"$AR_OUTPUT" 2>&1; then
  pass 'AR architecture regression remains green'
else
  tail -60 "$AR_OUTPUT"
  fail 'AR architecture regression remains green'
fi

printf 'PASS=%d FAIL=%d\n' "$PASS_COUNT" "$FAIL_COUNT"

if [ "$FAIL_COUNT" -ne 0 ]; then
  printf 'FAIL: P15 MP-0 Computer Use Provider Foundation\n'
  exit 1
fi

printf 'PASS: P15 MP-0 Computer Use Provider Foundation\n'
