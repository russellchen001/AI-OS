#!/bin/bash
set -u

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0

check_test() {
  local label="$1"
  local test_name="$2"
  local output="$3"

  if printf '%s\n' "$output" | grep -Fq "test $test_name ... ok"; then
    echo "✓ $label"
    PASS=$((PASS + 1))
  else
    echo "✗ $label"
    FAIL=$((FAIL + 1))
  fi
}

echo "============================================================"
echo "VERIFY P15 — OPENCLAW NO-VIABLE EXHAUSTION GATE"
echo "============================================================"

if transport_output="$(
  cd src-tauri &&
    cargo test --lib runtime::agent_skill_transport -- --nocapture 2>&1
)"; then
  check_test \
    "unproven unavailable is rejected" \
    "runtime::agent_skill_transport::mano_fallback_contract_tests::execution_unavailable_requires_every_exposed_capability_to_be_evaluated" \
    "$transport_output"
  check_test \
    "all legal candidates evaluated permits no-viable" \
    "runtime::agent_skill_transport::mano_fallback_contract_tests::all_exposed_capabilities_evaluated_allows_no_viable_execution_path" \
    "$transport_output"
  check_test \
    "available capability blocks premature no-viable" \
    "runtime::agent_skill_transport::tests::available_capability_prevents_premature_no_viable_execution_path" \
    "$transport_output"
  check_test \
    "failure continues to another available capability" \
    "runtime::agent_skill_transport::tests::one_capability_failure_continues_to_another_available_capability" \
    "$transport_output"
  check_test \
    "failed capability cannot be reclassified as no-viable" \
    "runtime::agent_skill_transport::tests::failed_capability_cannot_be_reclassified_as_no_viable_path" \
    "$transport_output"
  check_test \
    "successful capability cannot be reclassified as no-viable" \
    "runtime::agent_skill_transport::tests::successful_skill_cannot_be_reclassified_as_no_viable_path" \
    "$transport_output"
  check_test \
    "protocol and gateway errors keep typed safety mapping" \
    "runtime::agent_skill_transport::tests::transport_error_mapping_never_bypasses_runtime_gateway" \
    "$transport_output"
else
  echo "✗ OpenClaw transport suite"
  printf '%s\n' "$transport_output" | grep -E 'FAILED|error:|test result:' | head -20
  FAIL=$((FAIL + 7))
fi

if bridge_output="$(
  cd src-tauri &&
    cargo test --lib runtime::plan_runtime_bridge::mano_fallback_error_contract_tests -- --nocapture 2>&1
)"; then
  check_test \
    "only NoViableExecutionPath retains the Mano trigger signal" \
    "runtime::plan_runtime_bridge::mano_fallback_error_contract_tests::only_no_viable_execution_path_keeps_a_dedicated_plan_runtime_signal" \
    "$bridge_output"
  check_test \
    "permission and invalid request cannot trigger Mano" \
    "runtime::plan_runtime_bridge::mano_fallback_error_contract_tests::permission_and_invalid_request_never_become_mano_fallback_signal" \
    "$bridge_output"
else
  echo "✗ Plan Runtime Mano trigger boundary suite"
  printf '%s\n' "$bridge_output" | grep -E 'FAILED|error:|test result:' | head -20
  FAIL=$((FAIL + 2))
fi

echo "============================================================"
echo "PASS=$PASS FAIL=$FAIL"
echo "============================================================"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL P15 OPENCLAW NO-VIABLE EXHAUSTION GATE"
  exit 1
fi

echo "PASS P15 OPENCLAW NO-VIABLE EXHAUSTION GATE"
