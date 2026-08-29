#!/usr/bin/env bash
set -u
name="P15 Connection Removal"
output=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::persistence_reload_and_atomic_removal_do_not_affect_other_providers -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: atomic removal behavior"; echo "$output" | tail -20; exit 1; fi
echo "✓ removal is atomically persisted"
echo "✓ deleted Provider does not return after reload"
echo "✓ unrelated Provider remains intact"
echo "PASS $name"
