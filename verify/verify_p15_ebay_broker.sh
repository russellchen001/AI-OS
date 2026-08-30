#!/usr/bin/env bash
set -u
name="P15 eBay Broker"
tests=(
  "ebay::tests::browse_and_checkout_approval_are_independent"
  "ebay::tests::browse_adapter_uses_real_http_method_path_query_and_status"
  "storage::tests::oauth_state_is_single_use_and_tokens_are_deleted"
  "storage::tests::encrypted_tokens_survive_restart_without_plaintext_on_disk"
  "ebay::tests::token_exchange_requires_identity_api_before_authorized_record"
  "ebay::tests::expired_access_token_refreshes_then_reverifies_identity"
  "ebay::tests::disconnect_uses_official_refresh_token_revoke_endpoint"
  "tests::wrong_state_and_reused_code_are_rejected_without_token_output"
)
for test in "${tests[@]}"; do
  output=$(cargo test --manifest-path services/external-connector-broker/Cargo.toml "$test" -- --exact 2>&1)
  code=$?
  if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: $test"; echo "$output" | tail -20; exit 1; fi
  echo "✓ $test"
done
echo "PASS $name"
echo "DEFERRED Production eBay OAuth/Broker E2E: OPTIONAL_STRUCTURED_PROVIDER"
