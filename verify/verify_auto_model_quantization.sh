#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.." || exit 1

NAME="Beta 4.1 Auto local model selection"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

pass_check() {
  echo "✅ $1"
}

fail_check() {
  echo "❌ $1"
  if [ -f "$2" ]; then
    tail -30 "$2"
  fi
  echo "FAIL: $NAME — $1"
  exit 1
}

run_test() {
  local label="$1"
  local test_name="$2"
  local log="$TMP_DIR/${label// /-}.log"

  if cargo test --manifest-path src-tauri/Cargo.toml --lib "$test_name" -- --exact >"$log" 2>&1; then
    pass_check "$label"
  else
    fail_check "$label" "$log"
  fi
}

run_test \
  "4bit install keeps llmfit hardware evidence" \
  "local_model_advisor::tests::installed_quantization_keeps_hardware_fit_evidence"

run_test \
  "Auto selects the hardware-fit installed model" \
  "providers::tests::auto_prefers_the_only_hardware_fit_installed_model_without_a_name_rule"

run_test \
  "First Auto request never uses alphabetical local order while assessment is pending" \
  "providers::tests::auto_does_not_use_alphabetical_local_order_while_assessment_is_pending"

run_test \
  "Auto uses fit evidence that arrives during bounded wait" \
  "providers::tests::auto_uses_fit_evidence_that_arrives_during_the_bounded_wait"

run_test \
  "Timed-out assessment defers unknown local models behind cloud" \
  "providers::tests::auto_uses_cloud_instead_of_unknown_local_models_after_wait_timeout"

run_test \
  "Manual remains single-candidate only" \
  "providers::tests::manual_route_executes_only_the_requested_candidate"

if [ -e src-tauri/src/model_benchmark.rs ]; then
  fail_check "self-built Benchmark module removed" /dev/null
else
  pass_check "self-built Benchmark module removed"
fi

if cargo check --manifest-path src-tauri/Cargo.toml --quiet >"$TMP_DIR/cargo-check.log" 2>&1; then
  pass_check "Rust integration compiles"
else
  fail_check "Rust integration compiles" "$TMP_DIR/cargo-check.log"
fi

echo "PASS: $NAME"
exit 0
