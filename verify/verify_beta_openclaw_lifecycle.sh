#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ai-os-beta-openclaw.XXXXXX")"
trap 'rm -rf "$LOG_DIR"' EXIT

failures=0

run_behavior_test() {
  name="$1"
  label="$2"
  log="$LOG_DIR/$name.log"

  if (cd src-tauri && cargo test "$name" --lib > "$log" 2>&1) &&
     grep -Eq 'running 1 test' "$log" &&
     grep -Eq "test .*${name} .* ok" "$log"; then
    echo "✓ $label"
  else
    echo "✗ $label"
    tail -30 "$log"
    failures=$((failures + 1))
  fi
}

run_behavior_test managed_state_matrix_is_deterministic "managed state matrix"
run_behavior_test managed_live_state_reflects_live_gateway "live Gateway status"
run_behavior_test managed_unconfigured_state_distinguishes_existing_config "existing installation classification"
run_behavior_test fresh_install_plan_never_places_token_in_argv "fresh install secure plan"
run_behavior_test managed_secret_plan_never_places_token_in_argv "Secret Store stdin plan"
run_behavior_test ordinary_gateway_tokens_pass_through_unchanged "Keychain reference compatibility bridge"
run_behavior_test start_policy_is_bounded "gateway-stopped start policy"
run_behavior_test complete_policy_is_bounded "installed-not-configured Complete Setup policy"
run_behavior_test adopt_policy_preserves_existing_openclaw "existing-unmanaged Adopt policy"
run_behavior_test finish_policy_only_allows_managed_gap "configured-no-gateway Finish Setup policy"
run_behavior_test repair_policy_never_auto_overwrites "broken bounded Repair policy"

production_token_argv=$(
  rg -n -- '--token' src-tauri/src/managed_openclaw*.rs 2>/dev/null |
    grep -v 'assert!' || true
)

if [ -n "$production_token_argv" ]; then
  echo "✗ production managed flow contains --token"
  printf '%s\n' "$production_token_argv"
  failures=$((failures + 1))
else
  echo "✓ production managed flow has no --token argv"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta Managed OpenClaw lifecycle: $failures checks failed"
  exit 1
fi

echo "PASS Beta Managed OpenClaw lifecycle"
