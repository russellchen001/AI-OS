#!/usr/bin/env bash
set -u

name="P15 authenticated browser account verification (BROWSER-B)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "FAIL $name: $1"
  exit 1
}

test -f src-tauri/src/browser/account_verifier.rs \
  || fail "account_verifier.rs missing"
test -f src-tauri/src/browser/devtools.rs \
  || fail "devtools.rs missing"

for module in account_verifier devtools; do
  grep -q "$module" src-tauri/src/browser/mod.rs \
    || fail "$module is not registered in the browser module"
done

# Connections must reach CONNECTED only through a real verification result.
grep -q "AccountVerification::Authenticated" src-tauri/src/connections.rs \
  || fail "browser login can become CONNECTED without a real account verifier"
grep -q "verify_managed_account" src-tauri/src/connections.rs \
  || fail "verify_browser_login does not consult the managed browser verifier"
if grep -n "apply_verification(true)" src-tauri/src/connections.rs >/dev/null 2>&1; then
  fail "browser login must never be marked verified without a verifier result"
fi

# The login page must open in the AI-OS managed browser, not the system browser.
grep -q "open_managed_browser" src-tauri/src/connections.rs \
  || fail "begin_browser_login does not open the AI-OS managed browser"
if grep -n "openUrl(capability.loginUrl)" src/components/ConnectionsCenter.tsx >/dev/null 2>&1; then
  fail "Connections still opens browser logins in the user's default browser"
fi

# The verifier reads account state from inside the page, not from the URL.
grep -q "location.origin" src-tauri/src/browser/account_verifier.rs \
  || fail "account state is not read from the live page origin"
grep -q "Runtime.evaluate" src-tauri/src/browser/account_verifier.rs \
  || fail "account state is not read from the live page"

# Verified evidence must stay non-secret.
# Inspect production verifier code only. The #[cfg(test)] module intentionally
# names these forbidden APIs in tests that assert they are absent from probes.
account_code="$(awk '/^#\[cfg\(test\)\]/{exit} {print}' src-tauri/src/browser/account_verifier.rs)"
for forbidden in "document.cookie" "localStorage" "sessionStorage" "Authorization"; do
  if printf '%s\n' "$account_code" | grep -q "$forbidden"; then
    fail "the account verifier must never read $forbidden"
  fi
done

# --- user-added sites ----------------------------------------------------

test -f src-tauri/src/browser/site_registry.rs \
  || fail "there is no registry for sites the user adds"

for command_name in add_browser_site remove_browser_site confirm_browser_login list_browser_sites; do
  grep -q "$command_name" src-tauri/src/lib.rs \
    || fail "user site command is not registered: $command_name"
done

# A site with no account page and nothing learned must never reach Connected.
grep -q "fn site_is_signed_in" src-tauri/src/browser/site_registry.rs \
  || fail "user sites have no verification rule"

# The generic probe must read structure, never content or credentials. Only the
# code is checked: the test module names these strings to assert their absence.
site_code="$(awk '/^#\[cfg\(test\)\]/{exit} {print}' src-tauri/src/browser/site_registry.rs)"
for forbidden in "document.cookie" "localStorage" "sessionStorage" "textContent"; do
  if printf '%s\n' "$site_code" | grep -q "$forbidden"; then
    fail "the generic site probe must never read $forbidden"
  fi
done

sites="$(cargo test --manifest-path src-tauri/Cargo.toml browser::site_registry::tests -- --nocapture 2>&1)"
if [[ $? -ne 0 ]]; then
  echo "FAIL $name: user site behavior tests failed"
  echo "$sites" | tail -30
  exit 1
fi

required_site_tests=(
  "a_site_with_no_account_page_and_no_learned_evidence_can_never_connect"
  "a_site_is_only_ever_verified_on_its_own_hosts"
  "a_login_surface_is_never_learned_as_proof_of_an_account"
  "learning_keeps_only_what_signing_in_changed"
  "learning_refuses_when_nothing_distinguishable_appeared"
  "an_account_page_answers_only_while_the_site_keeps_us_there"
  "provider_ids_are_derived_safely_and_never_collide_with_built_ins"
  "the_generic_probe_is_built_from_the_battery_and_reads_nothing_personal"
)
for test_name in "${required_site_tests[@]}"; do
  if [[ "$sites" != *"$test_name ... ok"* ]]; then
    echo "FAIL $name: required user site behavior test did not pass: $test_name"
    exit 1
  fi
done

output="$(cargo test --manifest-path src-tauri/Cargo.toml 'browser::' -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: account verification behavior tests failed"
  echo "$output" | tail -30
  exit 1
fi

connections="$(cargo test --manifest-path src-tauri/Cargo.toml connections::tests -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: connections integration behavior tests failed"
  echo "$connections" | tail -30
  exit 1
fi

required_tests=(
  "every_in_scope_browser_provider_has_a_verifier"
  "authentication_requires_both_a_provider_origin_and_an_account_signal"
  "a_sign_in_prompt_is_never_an_authenticated_account"
  "account_marker_is_irreversible_stable_and_carries_no_personal_detail"
  "only_real_provider_pages_are_ever_probed"
  "amazon_verification_is_region_aware"
  "ebay_account_page_is_fail_closed_and_uses_only_structural_evidence"
  "ebay_without_a_managed_control_channel_is_not_a_session"
  "only_loopback_control_channels_are_accepted"
  "devtools_documents_are_parsed_out_of_raw_http_responses"
  "protocol_errors_and_exceptions_are_never_treated_as_results"
)

for test_name in "${required_tests[@]}"; do
  if [[ "$output" != *"$test_name ... ok"* ]]; then
    echo "FAIL $name: required behavior test did not pass: $test_name"
    exit 1
  fi
done

required_connections_tests=(
  "browser_connected_requires_a_real_account_verification"
  "restart_recovery_reports_checking_and_never_invents_connected"
  "only_a_decided_session_writes_the_state_authority"
  "verified_browser_session_carries_only_safe_evidence"
  "browser_opening_never_means_connected"
  "browser_session_serialization_contains_no_credentials"
  "expired_provider_can_enter_waiting_reconnect"
  "consumer_ebay_uses_the_existing_authenticated_browser_runtime"
)

for test_name in "${required_connections_tests[@]}"; do
  if [[ "$connections" != *"$test_name ... ok"* ]]; then
    echo "FAIL $name: required connections behavior test did not pass: $test_name"
    exit 1
  fi
done

echo "✓ every in-scope browser provider has a real account verifier"
echo "✓ login opens in the AI-OS managed browser, not the user's default browser"
echo "✓ verification inspects the authenticated session AI-OS owns"
echo "✓ account state is read from the live page, never inferred from a URL"
echo "✓ a provider origin without an account signal is not authenticated"
echo "✓ an account signal on a non-provider origin is not authenticated"
echo "✓ a sign-in prompt is never an authenticated account"
echo "✓ the account marker is irreversible and carries no personal detail"
echo "✓ no cookie, storage or authorization header is ever read"
echo "✓ Amazon verification is region aware and records the observed origin"
echo "✓ eBay account settings verification is structural and fail-closed"
echo "✓ protocol errors and page exceptions are never treated as verification"
echo "✓ a lost website account expires and enters reconnect"
echo "✓ restart and profile reuse never blindly restore Connected"
echo "✓ users can add their own sites, verified through the same managed browser"
echo "✓ a site with no account page and nothing learned can never be Connected"
echo "✓ a site is only ever verified on its own hosts"
echo "✓ a login surface is never learned as proof of an account"
echo "✓ the generic site probe reads structure only, never content or credentials"
echo "PASS $name"
echo "SKIP $name real account E2E: BROWSER-C real login required"
