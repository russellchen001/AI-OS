#!/usr/bin/env bash
set -u

name="P15 persistent authenticated browser runtime (BROWSER-A)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "FAIL $name: $1"
  exit 1
}

test -f src-tauri/src/browser/authenticated_runtime.rs \
  || fail "authenticated_runtime.rs missing"

grep -q "authenticated_runtime" src-tauri/src/browser/mod.rs \
  || fail "authenticated_runtime is not registered in the browser module"

for command_name in \
  open_authenticated_browser \
  inspect_authenticated_browser \
  close_authenticated_browser
do
  grep -q "$command_name" src-tauri/src/lib.rs \
    || fail "runtime command is not registered: $command_name"
done

# BROWSER-A must not promote any browser-backed provider to CONNECTED.
if grep -n "UnifiedConnectionState::Connected" src-tauri/src/browser/authenticated_runtime.rs >/dev/null 2>&1; then
  fail "the managed browser runtime must not decide connection state"
fi

grep -q "AccountVerification::Authenticated" src-tauri/src/connections.rs \
  || fail "browser login must only become CONNECTED through a real account verifier"

# Amazon must not be pinned to one regional site.
grep -q "www.amazon.com\"" src-tauri/src/browser/authenticated_runtime.rs \
  || fail "Amazon region handling must accept more than one regional origin"
grep -q "www.amazon.co.jp" src-tauri/src/browser/authenticated_runtime.rs \
  || fail "Amazon region handling must accept more than one regional origin"

output="$(cargo test --manifest-path src-tauri/Cargo.toml browser::authenticated_runtime::tests -- --nocapture 2>&1)"
status=$?

if [[ $status -ne 0 ]]; then
  echo "FAIL $name: managed browser runtime behavior tests failed"
  echo "$output" | tail -30
  exit 1
fi

required_tests=(
  "browser_discovery_order_is_deterministic"
  "managed_profile_lives_under_app_data_and_never_in_the_user_browser_profile"
  "session_reference_is_opaque_and_carries_no_credentials_or_raw_path"
  "running_or_reused_managed_browser_is_never_authenticated"
  "control_channel_is_loopback_only_and_readiness_requires_a_real_answer"
  "launch_arguments_pin_the_managed_profile_and_a_loopback_debug_channel"
  "only_ai_os_owned_processes_are_ever_selected_for_termination"
  "amazon_is_region_aware_and_origin_validation_is_fail_closed"
)

for test_name in "${required_tests[@]}"; do
  if [[ "$output" != *"$test_name ... ok"* ]]; then
    echo "FAIL $name: required behavior test did not pass: $test_name"
    exit 1
  fi
done

echo "✓ supported Chromium-family browser discovery order is deterministic"
echo "✓ managed profile lives beneath AI-OS application data"
echo "✓ the user's own Chrome/Safari profile is never adopted as AI-OS owned"
echo "✓ session metadata exposes only an opaque profile reference"
echo "✓ no password, cookie, token or raw profile path leaves the runtime"
echo "✓ browser control/debug channel is loopback only"
echo "✓ readiness requires a real answer from the managed browser"
echo "✓ launch arguments pin the managed profile and a loopback debug address"
echo "✓ only AI-OS-owned browser processes can be terminated"
echo "✓ opening, running or reusing a managed profile never equals authenticated"
echo "✓ Amazon is region aware and is not pinned to amazon.com.au"
echo "✓ provider/origin validation is fail-closed"
echo "PASS $name"
echo "WAITING_FOR_USER browser providers: BROWSER-B account verifier required before CONNECTED"
