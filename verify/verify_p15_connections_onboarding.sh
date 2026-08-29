#!/usr/bin/env bash
set -u

name="P15 connections onboarding"

connections="$(cargo test --manifest-path src-tauri/Cargo.toml connections::tests -- --nocapture 2>&1)"
status=$?

if [[ $status -ne 0 ]]; then
  echo "FAIL $name: connection state-machine behavior failed"
  echo "$connections" | tail -30
  exit 1
fi

required_tests=(
  "connect_all_order_skip_and_failure_preserve_success"
  "browser_opening_never_means_connected"
  "browser_session_serialization_contains_no_credentials"
  "expired_provider_can_enter_waiting_reconnect"
  "local_app_availability_is_recomputed_and_iwork_is_component_accurate"
  "iwork_connection_requires_prior_verification_and_live_probe"
  "iwork_bundle_mapping_is_explicit"
)

for test_name in "${required_tests[@]}"; do
  if [[ "$connections" != *"$test_name ... ok"* ]]; then
    echo "FAIL $name: required behavior test did not pass: $test_name"
    exit 1
  fi
done

oauth="$(cargo test --manifest-path src-tauri/Cargo.toml providers::tests::oauth_ -- --nocapture 2>&1)"
status=$?

if [[ $status -ne 0 ]]; then
  echo "FAIL $name: OAuth callback/session progression failed"
  echo "$oauth" | tail -30
  exit 1
fi

echo "✓ Connect All order and skip behavior"
echo "✓ later failure preserves previous successful connections"
echo "✓ OAuth state is scoped, single-use and callback state is verified"
echo "✓ opening a browser login page never marks an account connected"
echo "✓ expired browser accounts can enter reconnect"
echo "✓ password, cookie and token are absent from browser session state"
echo "✓ runtime app rescan detects Pages, Numbers, Keynote, WPS and Excel independently"
echo "✓ partial iWork installation remains component-accurate"
echo "✓ iWork Connected requires prior successful verification and a live authorization probe"
echo "✓ iWork application-to-bundle authorization mapping is explicit"
echo "PASS $name foundation"
echo "WAITING_FOR_USER Microsoft: application client ID and account consent required"
echo "WAITING_FOR_USER browser providers: authenticated profile verifier required"
