#!/bin/bash
set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT_DIR/src-tauri/Cargo.toml"
pass() {
  printf '✅ %s\n' "$1"
}

fail() {
  printf '❌ %s\n' "$1"
  printf 'FAIL: macOS Keychain Provider credentials\n'
  exit 1
}

run_test() {
  local name="$1"
  local description="$2"
  if cargo test --manifest-path "$MANIFEST" "$name" -- --exact >/dev/null 2>&1; then
    pass "$description"
  else
    fail "$description"
  fi
}

run_test \
  'providers::tests::provider_keychain_namespace_stays_compatible_with_existing_items' \
  'stable service and per-Provider account namespace'
run_test \
  'providers::tests::provider_credential_cache_reads_each_account_once_per_process' \
  'repeated Provider operations load Keychain once per account and process'
run_test \
  'providers::tests::lazy_provider_operations_read_only_accounts_that_are_used' \
  'startup/navigation stay lazy and only used Provider accounts are read'
run_test \
  'providers::tests::provider_commands_share_one_process_global_credential_authority' \
  'all Provider commands share one process-global credential authority'
run_test \
  'providers::tests::recovery_status_and_connection_test_share_one_underlying_read' \
  'recovery, status polling and connection testing share one underlying read'
run_test \
  'providers::tests::provider_credential_cache_keeps_provider_accounts_isolated' \
  'multiple Provider credentials do not collide'
run_test \
  'providers::tests::provider_credential_cache_update_replaces_memory_without_reloading' \
  'credential updates replace the cached value without another read'
run_test \
  'providers::tests::provider_credential_cache_delete_is_the_only_explicit_invalidation' \
  'credential deletion invalidates only its own cached account'
run_test \
  'providers::tests::missing_provider_credential_is_not_cached' \
  'a missing item can appear later without restarting AI-OS'
run_test \
  'providers::tests::a_new_process_cache_loads_an_existing_keychain_value_once' \
  'restart recovery preserves and loads an existing Keychain credential'
run_test \
  'providers::tests::provider_instance_round_trips_without_secret_material' \
  'Provider persistence and frontend contracts exclude secret material'
run_test \
  'providers::tests::provider_instance_rejects_secret_fields_in_migration_input' \
  'migration input cannot introduce plaintext credentials'

printf 'PASS: macOS Keychain Provider credentials\n'
