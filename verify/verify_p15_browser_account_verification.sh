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
for forbidden in "document.cookie" "localStorage" "sessionStorage" "Authorization"; do
  if grep -n "$forbidden" src-tauri/src/browser/account_verifier.rs >/dev/null 2>&1; then
    fail "the account verifier must never read $forbidden"
  fi
done

output="$(cargo test --manifest-path src-tauri/Cargo.toml browser::account_verifier::tests browser::devtools::tests -- --nocapture 2>&1)"
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
  "restart_and_profile_reuse_never_blindly_restore_connected"
  "verified_browser_session_carries_only_safe_evidence"
  "browser_opening_never_means_connected"
  "browser_session_serialization_contains_no_credentials"
  "expired_provider_can_enter_waiting_reconnect"
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
echo "✓ protocol errors and page exceptions are never treated as verification"
echo "✓ a lost website account expires and enters reconnect"
echo "✓ restart and profile reuse never blindly restore Connected"
echo "PASS $name"
echo "SKIP $name real account E2E: BROWSER-C real login required"
