#!/usr/bin/env bash
set -u
name="P15 External Connector Framework"
output=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests -- --nocapture 2>&1)
code=$?
if [ "$code" -ne 0 ]; then echo "FAIL $name: behavior tests failed"; echo "$output" | tail -20; exit 1; fi
if [[ "$output" != *"9 passed"* ]]; then echo "FAIL $name: expected nine framework behavior tests"; exit 1; fi
echo "✓ trusted manifest and public configuration validation"
echo "✓ Provider persistence and restart recovery"
echo "✓ Website login remains unverified without an adapter"
echo "✓ fixed Broker discovery endpoint returns the exact reviewed manifest"
echo "PASS $name"
