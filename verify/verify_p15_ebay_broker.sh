#!/usr/bin/env bash
set -u
name="P15 eBay Broker"
tests=(
  "ebay::tests::browse_and_checkout_approval_are_independent"
  "ebay::tests::browse_adapter_uses_real_http_method_path_query_and_status"
  "storage::tests::oauth_state_is_single_use_and_tokens_are_deleted"
  "tests::wrong_state_and_reused_code_are_rejected_without_token_output"
)
for test in "${tests[@]}"; do
  output=$(cargo test --manifest-path services/external-connector-broker/Cargo.toml "$test" -- --exact 2>&1)
  code=$?
  if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: $test"; echo "$output" | tail -20; exit 1; fi
  echo "✓ $test"
done
echo "PASS $name"
echo "SKIP Production eBay E2E: DEVELOPER_APPLICATION_REAL_ACCOUNT_AND_BUY_API_APPROVAL_REQUIRED"
