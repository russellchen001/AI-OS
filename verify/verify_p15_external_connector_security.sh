#!/usr/bin/env bash
set -u
name="P15 External Connector Security"
for test in manifest_and_all_three_layers_reject_sensitive_configuration production_http_and_unapproved_authorization_hosts_are_rejected; do
  output=$(cargo test --manifest-path src-tauri/Cargo.toml "external_connector::tests::$test" -- --exact 2>&1)
  code=$?
  if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: $test"; echo "$output" | tail -20; exit 1; fi
  echo "✓ $test"
done
echo "PASS $name"
