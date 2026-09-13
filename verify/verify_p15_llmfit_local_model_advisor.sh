#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

LOG="/tmp/ai-os-llmfit-advisor-verifier.log"

fail() {
  tail -40 "$LOG" 2>/dev/null || true
  echo "✗ $1"
  echo "FAIL P15 llmfit Local Model Advisor: $1"
  exit 1
}

run_test() {
  NAME="$1"
  TEST="$2"
  shift 2
  if (cd src-tauri && cargo test --lib "$TEST" "$@") >"$LOG" 2>&1; then
    grep -F "test $TEST ... ok" "$LOG" >/dev/null || fail "$NAME did not execute"
    echo "✓ $NAME"
  else
    fail "$NAME"
  fi
}

echo "P15 llmfit Local Model Advisor"

if (cd src-tauri && cargo test --lib local_model_advisor::tests) >"$LOG" 2>&1; then
  grep -F "test local_model_advisor::tests::installed_ollama_model_is_assessed_instead_of_assumed_suitable ... ok" "$LOG" >/dev/null \
    || fail "installed Ollama assessment did not execute"
  grep -F "test local_model_advisor::tests::installed_omlx_model_is_assessed_with_mlx_compatibility ... ok" "$LOG" >/dev/null \
    || fail "installed oMLX assessment did not execute"
  grep -F "test local_model_advisor::tests::oversized_and_marginal_models_remain_distinct ... ok" "$LOG" >/dev/null \
    || fail "fit boundary assessment did not execute"
  grep -F "test local_model_advisor::tests::unknown_model_fails_safe_without_invented_metadata ... ok" "$LOG" >/dev/null \
    || fail "unknown-model fail-safe did not execute"
  grep -F "test local_model_advisor::tests::missing_llmfit_keeps_a_real_native_profile_and_unknown_advice ... ok" "$LOG" >/dev/null \
    || fail "missing-runtime fallback did not execute"
  grep -F "test local_model_advisor::tests::adapter_refuses_every_side_effecting_llmfit_command ... ok" "$LOG" >/dev/null \
    || fail "no-auto-download boundary did not execute"
  echo "✓ deterministic fit, quant, context, provider, fallback, and safety contracts"
else
  fail "deterministic advisor contracts"
fi

run_test \
  "AI Center owns llmfit admission and ranking" \
  "providers::tests::ai_center_owns_admission_and_ranking_when_llmfit_evidence_exists"

run_test \
  "existing AI Center ordering and context gate" \
  "providers::tests::execution_agent_candidates_follow_ai_center_order_and_context_requirement"

if ! command -v llmfit >/dev/null 2>&1; then
  fail "official llmfit CLI is unavailable for the real smoke"
fi
run_test \
  "real machine, recommendation, Ollama, and oMLX fit smoke" \
  "local_model_advisor::tests::real_llmfit_machine_recommendation_and_fit_smoke" \
  -- --ignored --exact

echo "PASS P15 llmfit Local Model Advisor"
exit 0
