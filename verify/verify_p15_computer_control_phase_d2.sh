#!/bin/bash
set -u

NAME="P15 Computer Control Phase D2 Permissions"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-d2.log"

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
  system::permissions::tests \
  >"$LOG" 2>&1 || {
    tail -140 "$LOG"
    fail "D2 permission unit contracts"
  }

grep -q "test result: ok. 4 passed; 0 failed" "$LOG" || {
  tail -50 "$LOG"
  fail "D2 unit result contract"
}

echo "✅ list accepts only empty input"
echo "✅ Settings handoff targets are allowlisted"
echo "✅ notification native states remain distinct"
echo "✅ camera/microphone native states remain distinct"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_declared_system_capability_is_dispatched \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "system resolver coverage"
  }

echo "✅ D2 capabilities resolve without execution"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "permission reachability"
  }

echo "✅ D2 capabilities are authorisable"

SOURCE="src-tauri/src/system/permissions.rs"

grep -Fq 'AXIsProcessTrusted()' "$SOURCE" ||
  fail "native Accessibility preflight missing"

grep -Fq 'CGPreflightScreenCaptureAccess()' "$SOURCE" ||
  fail "native Screen Recording preflight missing"

grep -Fq 'authorizationStatusForMediaType' "$SOURCE" ||
  fail "native AVFoundation permission read missing"

grep -Fq 'getNotificationSettingsWithCompletionHandler' "$SOURCE" ||
  fail "native notification permission read missing"

grep -Fq 'NSWorkspace::sharedWorkspace()' "$SOURCE" ||
  fail "native System Settings handoff missing"

grep -Fq 'openURL(&url)' "$SOURCE" ||
  fail "NSWorkspace URL handoff missing"

echo "✅ native macOS permission APIs are used in-process"
echo "✅ System Settings handoff uses NSWorkspace"

if grep -Eq \
  'Command::new|std::process|osascript|System Events|tccutil|sqlite3' \
  "$SOURCE"
then
  fail "D2 crossed into subprocess/TCC manipulation"
fi

if grep -Eq \
  'CGRequestScreenCaptureAccess|requestAccessForMediaType|AXIsProcessTrustedWithOptions|requestAuthorizationWithOptions_completionHandler' \
  "$SOURCE"
then
  fail "D2 contains an automatic permission-request path"
fi

echo "✅ no shell / AppleScript"
echo "✅ no TCC database manipulation"
echo "✅ no automatic permission-request API"

for dependency in \
  'objc2-app-kit = "0.3.2"' \
  'objc2-application-services = "0.3.2"' \
  'objc2-av-foundation = "0.3.2"' \
  'objc2-core-graphics = "0.3.2"'
do
  grep -Fq "$dependency" src-tauri/Cargo.toml ||
    fail "missing native dependency: $dependency"
done

echo "✅ native framework dependencies are direct"

for capability in \
  system.permission.list \
  system.permission.open_settings
do
  grep -Fq "\"$capability\"" src-tauri/src/system.rs ||
    fail "$capability missing from System registry"

  grep -Fq "\"$capability\"" \
    src-tauri/src/runtime/openclaw_permission.rs ||
    fail "$capability missing from Runtime permission registry"
done

echo "✅ registry and permission contracts present"

echo "NOTE: verifier does NOT open System Settings."
echo "NOTE: verifier does NOT request or change any macOS permission."
echo "PASS $NAME"
