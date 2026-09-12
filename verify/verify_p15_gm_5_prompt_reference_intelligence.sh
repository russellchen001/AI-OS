#!/bin/bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
PASS=0
FAIL=0

ok() { printf '✓ %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '✗ %s\n' "$1"; FAIL=$((FAIL + 1)); }

run_group() {
  local label="$1"
  local filter="$2"
  local output
  if output=$(cargo test --manifest-path "$MANIFEST" --lib "$filter" -- --nocapture 2>&1) &&
     printf '%s\n' "$output" | grep -q 'test result: ok'; then
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
printf '%s\n' 'VERIFY P15 GM-5 — PROMPT / REFERENCE INTELLIGENCE'
printf '%s\n' '============================================================'

run_group 'structured CreativeIntent / PromptSpec behavior' 'generative_media::prompt_intelligence::tests'
run_group 'expert prompt verbatim preservation' 'deliberate_expert_prompt_remains_verbatim'
run_group 'Provider is fixed before prompt compilation' 'structured_fields_compile_for_the_fixed_provider'
run_group 'compiler preserves manual model and cannot switch Provider' 'manual_model_is_preserved_while_provider_stays_fixed'
run_group 'model advisor preserves manual model/profile choices' 'manual_model_and_profile_are_never_replaced'
run_group 'local recommendation stays within selected Provider instance' 'advice_is_scoped_to_the_selected_provider_instance'
run_group 'image ReferenceSpec is normalized with provenance' 'image_response_normalizes_with_traceable_provenance'
run_group 'video ReferenceSpec retains bounded temporal intelligence' 'video_response_preserves_bounded_temporal_events'
run_group 'local reference workflow rejects non-loopback cloud endpoints' 'non_loopback_endpoint_is_rejected'
run_group 'normal reference handling is bounded and typed' 'reference_signature_validation_is_typed'
run_group 'normal execution does not perform implicit Setup / Repair' 'ready_requires_matching_real_smoke_evidence_and_model_snapshot'
run_group 'explicit Setup / Repair confirmation and safe archive behavior' 'generative_media::comfyui_reference_setup::tests'
run_group 'generated request consumes normalized reference constraints' 'normalized_reference_constraints_feed_the_compiled_request'
run_group 'prompt metadata stores digest rather than raw prompt' 'metadata_contains_only_prompt_digest_not_prompt_text'
run_group 'Ready requires normalized real smoke and complete model evidence' 'ready_promotion_requires_smoke_text_normalizable_as_reference_spec'
run_group 'smoke fixture remains 64x64 RGB for current ComfyUI PyAV' 'setup_smoke_fixture_is_64_by_64_rgb_for_current_comfyui_pyav'
run_group 'Provider execution identity remains exact' 'generative_media::executor::tests'

if [[ "${AI_OS_GM5_LIVE:-0}" == "1" ]]; then
  run_live 'real local Reference VLM Setup / Repair smoke' 'generative_media::comfyui_reference_setup::tests::live_reference_vlm_setup_and_smoke'
  run_live 'real Ready adapter ReferenceSpec normalization smoke' 'generative_media::comfyui_reference::tests::live_ready_adapter_normalizes_reference_spec'
else
  printf '%s\n' 'SKIP real local Reference VLM smoke — set AI_OS_GM5_LIVE=1 for explicit Setup / Repair'
fi

if [[ "${AI_OS_SKIP_AR_AGGREGATE:-0}" == "1" ]]; then
  printf '%s\n' 'SKIP nested AR aggregate — parent closure owns AR verification'
elif bash "$ROOT/verify/verify_ar1a_agent_skill_contract.sh" >/dev/null 2>&1 &&
   bash "$ROOT/verify/verify_ar1b_agent_owned_execution.sh" >/dev/null 2>&1 &&
   bash "$ROOT/verify/verify_ar1c_agent_skill_transport.sh" >/dev/null 2>&1; then
  ok 'AR-1 Agent-owned execution boundary'
else
  bad 'AR-1 Agent-owned execution boundary'
fi

printf '%s\n' '============================================================'
printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
printf '%s\n' '============================================================'

if [[ "$FAIL" -ne 0 ]]; then
  printf '%s\n' 'FAIL P15 GM-5 PROMPT / REFERENCE INTELLIGENCE'
  exit 1
fi

printf '%s\n' 'PASS P15 GM-5 PROMPT / REFERENCE INTELLIGENCE'
