#!/usr/bin/env bash
set -u
name="P15 Add Other Provider"
output=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::persistence_reload_and_atomic_removal_do_not_affect_other_providers -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: persistence/reload behavior"; echo "$output" | tail -20; exit 1; fi
npm run build >/tmp/ai-os-add-provider-build.log 2>&1
code=$?
if [ "$code" -ne 0 ]; then echo "FAIL $name: frontend build"; tail -20 /tmp/ai-os-add-provider-build.log; exit 1; fi
echo "✓ custom Provider persists across reload"
echo "✓ removing one Provider preserves other Providers"
echo "✓ executable Add Other Provider UI build"
echo "PASS $name"
