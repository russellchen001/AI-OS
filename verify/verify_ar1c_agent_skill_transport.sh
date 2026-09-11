#!/bin/bash
set -u

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0

run_test() {
  local name="$1"
  local test_name="$2"
  shift 2
  local output

  if output="$(cd src-tauri && cargo test "$test_name" --lib "$@" 2>&1)" &&
     printf '%s\n' "$output" | grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed'; then
    echo "✓ $name"
    PASS=$((PASS + 1))
  else
    echo "✗ $name"
    printf '%s\n' "$output" | grep -E 'FAILED|error:|test result:' | head -20
    FAIL=$((FAIL + 1))
  fi
}

echo "============================================================"
echo "VERIFY AR-1C — AGENT SKILL TRANSPORT"
echo "============================================================"

run_test \
  "selected Agent exposes allowed Skill capability" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "Agent transport receives normalized Skill exposure" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "OpenClaw-specific protocol stays behind transport adapter" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "Agent-issued Skill request reaches SkillInvocationGateway" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "Runtime-issued SkillInvocationContext is used" \
  runtime::skill_invocation::tests::runtime_context_carries_trace_identity_end_to_end
run_test \
  "Agent cannot self-authorize confirmation" \
  runtime::skill_invocation::tests::agent_skill_request_cannot_self_authorize_confirmation
run_test \
  "permission denial occurs before backend invocation" \
  runtime::agent_skill_transport::tests::permission_denial_happens_before_backend_invocation
run_test \
  "non-exposed capability is denied before backend invocation" \
  runtime::agent_skill_transport::tests::non_exposed_capability_is_rejected_before_backend_invocation
run_test \
  "normalized Skill result returns through transport" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "Agent execution consumes Skill result and completes" \
  runtime::agent_skill_transport::tests::openclaw_transport_round_trips_agent_request_gateway_result_and_completion
run_test \
  "no direct Plan Runtime backend dispatch is reintroduced" \
  runtime::plan_runtime_bridge::tests::registered_skill_enters_selected_agent_instead_of_backend_dispatch
run_test \
  "exact OpenClaw version is not required" \
  runtime::agent_execution::tests::unknown_newer_agent_version_is_accepted_when_capabilities_match
run_test \
  "capability negotiation controls transport admission" \
  runtime::agent_execution::tests::version_alone_cannot_enable_skill_transport
run_test \
  "transport errors do not bypass Runtime governance" \
  runtime::agent_skill_transport::tests::transport_error_mapping_never_bypasses_runtime_gateway
run_test \
  "real OpenClaw Agent reaches existing Skill backend and completes" \
  runtime::agent_skill_transport::tests::real_openclaw_agent_skill_round_trip \
  -- --ignored --exact

echo "============================================================"
echo "PASS=$PASS FAIL=$FAIL"
echo "============================================================"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL AR-1C AGENT SKILL TRANSPORT"
  exit 1
fi

echo "PASS AR-1C AGENT SKILL TRANSPORT"
