#!/usr/bin/env bash
set -u
name="P15 eBay Connector"
output=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::ebay_capability_approval_and_confirmation_are_independent -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$output" != *"1 passed"* ]]; then echo "FAIL $name: capability policy behavior"; echo "$output" | tail -20; exit 1; fi
echo "✓ Browse and Checkout approval are independent"
confirmation=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::high_risk_capability_cannot_execute_without_confirmation -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$confirmation" != *"1 passed"* ]]; then echo "FAIL $name: high-risk confirmation behavior"; echo "$confirmation" | tail -20; exit 1; fi
echo "✓ checkout.confirm requires User Confirmation"
builtin=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::builtin_ebay_is_single_non_removable_configuration_required_definition -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$builtin" != *"1 passed"* ]]; then echo "FAIL $name: built-in eBay definition behavior"; echo "$builtin" | tail -20; exit 1; fi
migration=$(cargo test --manifest-path src-tauri/Cargo.toml external_connector::tests::legacy_custom_ebay_migrates_once_and_unsafe_reference_requires_migration -- --exact 2>&1)
code=$?
if [ "$code" -ne 0 ] || [[ "$migration" != *"1 passed"* ]]; then echo "FAIL $name: legacy eBay migration behavior"; echo "$migration" | tail -20; exit 1; fi
echo "✓ built-in eBay is a single non-removable Connector definition"
echo "✓ legacy custom eBay migrates without duplicates or unsafe references"
echo "PASS $name"
echo "SKIP Production eBay E2E: DEVELOPER_APPLICATION_AND_BUY_API_APPROVAL_REQUIRED"
