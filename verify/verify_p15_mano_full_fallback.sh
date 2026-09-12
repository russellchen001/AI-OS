#!/bin/bash

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEST_OUTPUT="$(mktemp -t ai-os-mano-fallback-tests.XXXXXX)"
REGRESSION_OUTPUT="$(mktemp -t ai-os-mano-fallback-regressions.XXXXXX)"
trap 'rm -f "$TEST_OUTPUT" "$REGRESSION_OUTPUT"' EXIT

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

if ! (cd "$ROOT_DIR/src-tauri" && cargo test --lib mp1_ -- --nocapture) >"$TEST_OUTPUT" 2>&1; then
  tail -60 "$TEST_OUTPUT"
  printf 'FAIL: P15 Mano Full Fallback tests did not complete\n'
  exit 1
fi

require_test \
  mp1_only_no_viable_execution_path_invokes_mano_exactly_once \
  'NoViableExecutionPath invokes Mano exactly once'
require_test \
  mp1_every_other_agent_outcome_bypasses_mano \
  'success and every non-fallback Agent error bypass Mano'
require_test \
  mp1_mano_unavailable_is_a_normalized_terminal_runtime_error \
  'unavailable Mano returns a normalized Runtime error'
require_test \
  mp1_plan_runtime_routes_cancellation_to_mano \
  'Plan Runtime routes cancellation to the Mano adapter'
require_test \
  mp1_cloud_is_rejected_before_cli_probe_without_explicit_authorization \
  'Cloud is rejected before CLI access without explicit authorization'
require_test \
  mp1_unavailable_local_never_switches_to_authorized_cloud \
  'unavailable Local never switches to Cloud'
require_test \
  mp1_task_confirmation_is_required_before_any_cli_probe \
  'task-entry confirmation is enforced before Mano starts'
require_test \
  mp1_sensitive_material_is_rejected_before_any_cli_probe \
  'credentials and secret material are rejected before Mano starts'
require_test \
  mp1_explicitly_authorized_cloud_runs_as_one_bounded_black_box \
  'authorized Cloud delegates one complete task to the upstream CLI'
require_test \
  mp1_runtime_cancel_uses_official_stop_and_reaches_cancelled_terminal_state \
  'Runtime cancellation uses stop and reaches Cancelled'
require_test \
  mp1_timeout_stops_the_process_and_reaches_timed_out_terminal_state \
  'bounded timeout stops the process and reaches TimedOut'

if bash "$ROOT_DIR/verify/verify_p15_mano_provider_foundation.sh" >"$REGRESSION_OUTPUT" 2>&1; then
  pass 'MP-0 Computer Use foundation remains green and separate'
else
  tail -60 "$REGRESSION_OUTPUT"
  fail 'MP-0 Computer Use foundation remains green and separate'
fi

printf 'PASS=%d FAIL=%d\n' "$PASS_COUNT" "$FAIL_COUNT"

if [ "$FAIL_COUNT" -ne 0 ]; then
  printf 'FAIL: P15 Mano Full Fallback\n'
  exit 1
fi

printf 'PASS: P15 Mano Full Fallback\n'
