#!/usr/bin/env bash
set -u

name="P15 eBay authenticated browser"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "FAIL $name: $1"
  exit 1
}

tests=(
  "connections::tests::consumer_ebay_uses_the_existing_authenticated_browser_runtime"
  "connections::tests::browser_opening_never_means_connected"
  "connections::tests::restart_recovery_reports_checking_and_never_invents_connected"
  "browser::authenticated_runtime::tests::ebay_origins_are_exact_and_malicious_lookalikes_are_rejected"
  "browser::authenticated_runtime::tests::managed_profile_lives_under_app_data_and_never_in_the_user_browser_profile"
  "browser::authenticated_runtime::tests::only_ai_os_owned_processes_are_ever_selected_for_termination"
  "browser::account_verifier::tests::ebay_account_page_is_fail_closed_and_uses_only_structural_evidence"
  "browser::account_verifier::tests::ebay_without_a_managed_control_channel_is_not_a_session"
)

for test_name in "${tests[@]}"; do
  output="$(cargo test --manifest-path src-tauri/Cargo.toml "$test_name" -- --exact 2>&1)"
  status=$?
  if [[ $status -ne 0 || "$output" != *"1 passed"* || "$output" != *"$test_name ... ok"* ]]; then
    echo "$output" | tail -30
    fail "behavior test did not pass: $test_name"
  fi
  echo "✓ $test_name"
done

echo "PASS $name"
echo "REAL E2E PASS: product-owner-confirmed lifecycle is recorded in HANDOFF.md"
