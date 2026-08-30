#!/usr/bin/env bash
set -u

name="P15 eBay Official OAuth"
broker_manifest="services/external-connector-broker/Cargo.toml"

broker_tests=(
  "ebay::tests::authorization_uses_only_approved_identity_scopes_and_runame"
  "storage::tests::oauth_state_is_single_use_and_tokens_are_deleted"
  "storage::tests::expired_oauth_state_is_consumed_and_fails_closed"
  "tests::wrong_state_and_reused_code_are_rejected_without_token_output"
  "ebay::tests::token_exchange_requires_identity_api_before_authorized_record"
  "ebay::tests::identity_failure_cannot_create_connected_token_record"
  "ebay::tests::account_marker_is_stable_and_does_not_expose_raw_identity"
  "storage::tests::encrypted_tokens_survive_restart_without_plaintext_on_disk"
  "ebay::tests::restart_reverification_does_not_blindly_trust_saved_marker"
  "ebay::tests::expired_access_token_refreshes_then_reverifies_identity"
  "ebay::tests::disconnect_uses_official_refresh_token_revoke_endpoint"
)

for test_name in "${broker_tests[@]}"; do
  output=$(cargo test --manifest-path "$broker_manifest" "$test_name" -- --exact 2>&1)
  code=$?
  if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]] || [[ "$output" != *"$test_name ... ok"* ]]; then
    echo "❌ $test_name"
    echo "$output" | tail -20
    echo "FAIL: $name"
    exit 1
  fi
  echo "✅ $test_name"
done

desktop_tests=(
  "external_connector::tests::builtin_ebay_is_single_non_removable_configuration_required_definition"
  "external_connector::tests::ebay_refresh_failure_requires_reconnect_and_never_restores_connected"
)
for desktop_test in "${desktop_tests[@]}"; do
  output=$(cargo test --manifest-path src-tauri/Cargo.toml "$desktop_test" -- --exact 2>&1)
  code=$?
  if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]] || [[ "$output" != *"$desktop_test ... ok"* ]]; then
    echo "❌ Desktop eBay Provider contract: $desktop_test"
    echo "$output" | tail -20
    echo "FAIL: $name"
    exit 1
  fi
done

echo "✅ Desktop eBay Provider is Official OAuth via Broker with no browser fallback"
echo "PASS: $name"
echo "DEFERRED: Production OAuth/Broker deployment is optional and is not the consumer eBay path"
