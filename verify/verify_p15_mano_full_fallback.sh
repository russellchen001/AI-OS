#!/bin/bash

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEST_OUTPUT="$(mktemp -t ai-os-mano-fallback-tests.XXXXXX)"
REAL_CLI_OUTPUT="$(mktemp -t ai-os-mano-real-cli.XXXXXX)"
REGRESSION_OUTPUT="$(mktemp -t ai-os-mano-fallback-regressions.XXXXXX)"
trap 'rm -f "$TEST_OUTPUT" "$REAL_CLI_OUTPUT" "$REGRESSION_OUTPUT"' EXIT

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
  mp1_agent_input_cannot_enable_cloud \
  'Agent input cannot enable Cloud'
require_test \
  mp1_task_confirmation_is_required_before_any_cli_probe \
  'task-entry confirmation is enforced before Mano starts'
require_test \
  mp1_sensitive_material_is_rejected_before_any_cli_probe \
  'credentials and secret material are rejected before Mano starts'
require_test \
  mp1_local_cancellation_signal_is_written_without_invoking_cli_stop \
  'Local cancellation uses the upstream stop flag without Cloud access'
require_test \
  mp1_explicitly_authorized_cloud_runs_as_one_bounded_black_box \
  'authorized Cloud delegates one complete task to the upstream CLI'
require_test \
  mp1_task_text_and_input_do_not_leak_into_progress_or_result \
  'task text and input do not leak into AI-OS progress or result'
require_test \
  mp1_concurrency_is_one_and_second_execution_is_rejected \
  'concurrency is one and a second execution is rejected'
require_test \
  mp1_runtime_cancel_uses_official_stop_and_reaches_cancelled_terminal_state \
  'Runtime cancellation uses stop and reaches Cancelled'
require_test \
  mp1_timeout_stops_the_process_and_reaches_timed_out_terminal_state \
  'bounded timeout stops the process and reaches TimedOut'

if (cd "$ROOT_DIR/src-tauri" && cargo test --lib \
  runtime::mano_fallback::tests::mp5_real_cli_is_discovered_and_local_not_ready_is_normalized \
  -- --ignored --exact --nocapture) >"$REAL_CLI_OUTPUT" 2>&1 \
  && grep -Eq 'test .*::mp5_real_cli_is_discovered_and_local_not_ready_is_normalized \.\.\. ok' "$REAL_CLI_OUTPUT" \
  && grep -Eq 'test result: ok\. 1 passed; 0 failed;' "$REAL_CLI_OUTPUT"; then
  pass 'installed Mano CLI is discovered and Local unsupported/not-ready is normalized'
else
  tail -60 "$REAL_CLI_OUTPUT"
  fail 'installed Mano CLI is discovered and Local unsupported/not-ready is normalized'
fi

if (cd "$ROOT_DIR/src-tauri" && cargo test --lib mp0_ -- --nocapture) >"$REGRESSION_OUTPUT" 2>&1 \
  && grep -Eq 'test .*::mp0_selected_agent_crosses_transport_gateway_backend_registry_and_provider \.\.\. ok' "$REGRESSION_OUTPUT" \
  && grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed;' "$REGRESSION_OUTPUT"; then
  pass 'MP-0 Computer Use foundation remains green and separate'
else
  tail -60 "$REGRESSION_OUTPUT"
  fail 'MP-0 Computer Use foundation remains green and separate'
fi

printf '%s\n' 'AR-1C External E2E is an independent environment check and is not counted as Mano acceptance.'

printf 'PASS=%d FAIL=%d\n' "$PASS_COUNT" "$FAIL_COUNT"

if [ "$FAIL_COUNT" -ne 0 ]; then
  printf 'FAIL: P15 Mano Full Fallback\n'
  exit 1
fi

printf 'PASS: P15 Mano Full Fallback\n'
