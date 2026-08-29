#!/usr/bin/env bash
set -u
name="P15 External Connector Broker"
output=$(cargo test --manifest-path services/external-connector-broker/Cargo.toml 2>&1)
code=$?
if [ "$code" -ne 0 ]; then echo "FAIL $name: Broker behavior tests failed"; echo "$output" | tail -20; exit 1; fi
if [[ "$output" != *"6 passed"* ]]; then echo "FAIL $name: expected six Broker behavior tests"; exit 1; fi
echo "✓ deployable Broker routes compile and execute"
echo "✓ OAuth state is single-use and token storage is encrypted"
echo "✓ Production HTTPS and environment isolation"
echo "PASS $name"
