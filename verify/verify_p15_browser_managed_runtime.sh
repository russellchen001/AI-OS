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

# --- managed browser shutdown lifecycle ---------------------------------

grep -q "fn close_all_managed_browsers" src-tauri/src/browser/authenticated_runtime.rs \
  || fail "there is no shutdown primitive for AI-OS-owned browsers"

grep -q "close_all_managed_browsers" src-tauri/src/lib.rs \
  || fail "managed browser shutdown is not wired into the application lifecycle"

for lifecycle_event in "RunEvent::ExitRequested" "RunEvent::Exit"; do
  grep -q "$lifecycle_event" src-tauri/src/lib.rs \
    || fail "shutdown is not wired to $lifecycle_event"
done

# A killed Chromium never releases its profile Singleton lock, so the browser
# must be asked to close before it is killed.
terminate_body="$(awk '/fn terminate_owned_process/,/^}/' src-tauri/src/browser/authenticated_runtime.rs)"
graceful_line="$(printf '%s\n' "$terminate_body" | grep -n 'Browser.close' | head -1 | cut -d: -f1)"
kill_line="$(printf '%s\n' "$terminate_body" | grep -n 'child.kill()' | head -1 | cut -d: -f1)"
if [[ -z "$graceful_line" || -z "$kill_line" ]]; then
  fail "managed browser termination has no graceful close followed by a kill fallback"
fi
if [[ "$graceful_line" -ge "$kill_line" ]]; then
  fail "the managed browser must be asked to close before it is killed"
fi

# Ownership boundary: AI-OS must never hunt for browser processes.
for scan in "pkill" "killall" "pgrep"; do
  if grep -n "$scan" src-tauri/src/browser/*.rs >/dev/null 2>&1; then
    fail "the runtime must never scan for or kill processes it does not own: $scan"
  fi
done

# Launching must be single-flight per provider and must not hold the registry
# lock, or one slow launch blocks capability listing and freezes Connections.
grep -q "fn begin_launch" src-tauri/src/browser/authenticated_runtime.rs \
  || fail "concurrent launches for one provider are not guarded"

launch_body="$(awk '/pub\(crate\) fn open_managed_browser/,/^}/' src-tauri/src/browser/authenticated_runtime.rs)"
if printf '%s\n' "$launch_body" | grep -q "wait_until_ready"; then
  registry_locks="$(printf '%s\n' "$launch_body" | grep -c 'registry()')"
  if [[ "$registry_locks" -lt 2 ]]; then
    fail "the launch must take the registry lock in short sections, not hold it across readiness"
  fi
fi

# A click must never look dead while a launch is in flight.
awk '/async function connect\(providerId/,/^  }/' src/components/ConnectionsCenter.tsx \
  | grep -q "setMessage" \
  || fail "connect() can return without telling the user anything"

# A profile another browser still owns is refused, never adopted or killed.
grep -q "fn profile_is_owned_by_live_browser" src-tauri/src/browser/authenticated_runtime.rs \
  || fail "a profile still owned by a live browser is not detected before launch"

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
  "shutdown_drains_the_owned_registry_and_repeats_safely"
  "readiness_stops_as_soon_as_the_owned_browser_exits"
  "a_live_profile_owner_is_recognised_from_the_published_port"
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
echo "✓ shutdown is wired to the real application lifecycle"
echo "✓ an owned browser is asked to close before it is killed"
echo "✓ shutdown drains the owned registry and repeats safely"
echo "✓ the runtime never scans for browser processes it does not own"
echo "✓ a profile a live browser still owns is refused, never adopted"
echo "✓ a handed-off launch fails fast instead of waiting out readiness"
echo "✓ a slow launch never holds the registry lock or blocks Connections"
echo "✓ concurrent launches for one provider are refused, not stacked"
echo "✓ a click while a launch is in flight always reports back"
echo "✓ Amazon is region aware and is not pinned to amazon.com.au"
echo "✓ provider/origin validation is fail-closed"
echo "PASS $name"
echo "WAITING_FOR_USER browser providers: BROWSER-B account verifier required before CONNECTED"
