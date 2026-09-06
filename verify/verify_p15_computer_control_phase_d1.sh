#!/bin/bash
set -u

NAME="P15 Computer Control Phase D1 Notification"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-d1.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  system::notification::tests \
  >"$LOG" 2>&1 || {
    tail -120 "$LOG"
    fail "notification unit contract"
  }

grep -q "test result: ok. 3 passed; 0 failed" "$LOG" || {
  tail -50 "$LOG"
  fail "notification unit result contract"
}

echo "✅ notification payload is bounded"
echo "✅ authorization states remain distinct"
echo "✅ only Apple-authorized states permit delivery"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_declared_system_capability_is_dispatched \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "system resolver coverage"
  }

echo "✅ system.notification.send resolves without execution"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "permission reachability"
  }

echo "✅ notification capability is authorisable"

grep -Fq \
  'UNUserNotificationCenter::currentNotificationCenter()' \
  src-tauri/src/system/notification.rs ||
  fail "native UserNotifications center missing"

grep -Fq \
  'getNotificationSettingsWithCompletionHandler' \
  src-tauri/src/system/notification.rs ||
  fail "native notification status read missing"

grep -Fq \
  'requestAuthorizationWithOptions_completionHandler' \
  src-tauri/src/system/notification.rs ||
  fail "native first-use authorization path missing"

grep -Fq \
  'addNotificationRequest_withCompletionHandler' \
  src-tauri/src/system/notification.rs ||
  fail "native notification submission missing"

echo "✅ macOS UserNotifications is used in-process"

if grep -Eq \
  'Command::new|std::process|osascript|System Events|keystroke|notify_rust|notify-rust' \
  src-tauri/src/system/notification.rs
then
  fail "notification adapter crossed into subprocess or GUI automation"
fi

echo "✅ no shell / AppleScript / GUI automation"
echo "✅ no alternate application identity shim"

grep -Fq \
  'objc2-user-notifications = "0.3.2"' \
  src-tauri/Cargo.toml ||
  fail "direct native notification dependency missing"

grep -Fq \
  '"system.notification.send"' \
  src-tauri/src/system.rs ||
  fail "system registry missing notification"

grep -Fq \
  '"system.notification.send"' \
  src-tauri/src/runtime/openclaw_permission.rs ||
  fail "permission registry missing notification"

echo "✅ registry and permission contracts present"

echo "NOTE: verifier deliberately does not post a notification."
echo "NOTE: product execution is exercised later from the real AI-OS app identity."

echo "PASS $NAME"
