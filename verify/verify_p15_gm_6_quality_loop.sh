#!/bin/bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
PASS=0
FAIL=0

ok() { printf '✓ %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '✗ %s\n' "$1"; FAIL=$((FAIL + 1)); }

run_test() {
  local label="$1"
  local test_name="$2"
  local output
  if output=$(cargo test --manifest-path "$MANIFEST" --lib "$test_name" -- --exact 2>&1) &&
     printf '%s\n' "$output" | grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed'; then
    ok "$label"
  else
    bad "$label"
    printf '%s\n' "$output" | grep -E 'FAILED|^error|panicked|test result:' | tail -20
  fi
}

run_live() {
  local label="$1"
  local test_name="$2"
  local output
  if output=$(cargo test --manifest-path "$MANIFEST" --lib "$test_name" -- --ignored --exact --nocapture 2>&1) &&
     printf '%s\n' "$output" | grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed'; then
    ok "$label"
  else
    bad "$label"
    printf '%s\n' "$output" | grep -E 'FAILED|^error|panicked|test result:' | tail -20
  fi
}

printf '%s\n' '============================================================'
printf '%s\n' 'VERIFY P15 GM-6 — BOUNDED QUALITY LOOP'
printf '%s\n' '============================================================'

PREFIX='generative_media::quality_loop::tests::'
run_test 'successful generation does not retry unnecessarily' "${PREFIX}successful_generation_records_assessment_without_retry"
run_test 'poor result produces normalized assessment and correction' "${PREFIX}poor_result_gets_one_normalized_correction_then_succeeds"
run_test 'request/reference/composition/temporal issues are normalized' "${PREFIX}assessment_normalizes_reference_composition_and_temporal_signals"
run_test 'retry count is hard-capped and cannot loop forever' "${PREFIX}retry_count_is_hard_capped_and_exhaustion_is_normalized"
run_test 'cancellation stops the loop before further execution' "${PREFIX}cancellation_stops_before_first_or_corrective_attempt"
run_test 'manual Provider and account remain exact across correction' "${PREFIX}poor_result_gets_one_normalized_correction_then_succeeds"
run_test 'cloud budget and estimator boundary prevent unauthorized retry' "${PREFIX}cloud_retry_requires_explicit_authorization_cost_bound_and_estimator"
run_test 'permission/auth/policy denial prevents retry' "${PREFIX}policy_and_auth_denials_never_retry"
run_test 'unexpected Provider/account identity is rejected immediately' "${PREFIX}unexpected_provider_or_account_identity_stops_immediately"
run_test 'attempt and quality metadata are recorded without prompt/media' "${PREFIX}successful_generation_records_assessment_without_retry"
run_test 'retry exhaustion returns a normalized terminal error' "${PREFIX}retry_count_is_hard_capped_and_exhaustion_is_normalized"
run_test 'Local First cannot silently fall back to Cloud' 'generative_media::router::tests::local_first_with_no_local_provider_never_auto_routes_to_cloud'
run_test 'Provider error cannot trigger another Provider' 'generative_media::executor::tests::provider_error_is_not_replaced_by_another_provider'

if [[ "${AI_OS_GM6_LIVE:-0}" == "1" ]]; then
  run_live 'real local bounded correction reaches ComfyUI exactly once' "${PREFIX}live_local_bounded_correction_reaches_real_comfyui_once"
else
  printf '%s\n' 'SKIP real local bounded correction smoke — set AI_OS_GM6_LIVE=1'
fi

if bash "$ROOT/verify/verify_ar1a_agent_skill_contract.sh" >/dev/null 2>&1 &&
   bash "$ROOT/verify/verify_ar1b_agent_owned_execution.sh" >/dev/null 2>&1 &&
   bash "$ROOT/verify/verify_ar1c_agent_skill_transport.sh" >/dev/null 2>&1; then
  ok 'quality loop remains behind Agent Skill transport and SkillInvocationGateway'
else
  bad 'quality loop remains behind Agent Skill transport and SkillInvocationGateway'
fi

printf '%s\n' '============================================================'
printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
printf '%s\n' '============================================================'

if [[ "$FAIL" -ne 0 ]]; then
  printf '%s\n' 'FAIL P15 GM-6 BOUNDED QUALITY LOOP'
  exit 1
fi

printf '%s\n' 'PASS P15 GM-6 BOUNDED QUALITY LOOP'
