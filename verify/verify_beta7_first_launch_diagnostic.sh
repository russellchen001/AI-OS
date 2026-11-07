#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_first_launch"
APP="/Users/Shared/AI-OS-Beta7-Test.app"
LOG="/tmp/aios-beta7-first-launch.log"

fail() {
  echo "❌ $1"
  echo "FAIL: $FEATURE — $1"
  exit 1
}

echo "===== AUTOMATED REGRESSION ====="

bash verify/verify_beta7_openclaw_startup.sh || fail "OpenClaw regression"
echo "✅ OpenClaw build regression"

bash verify/verify_beta_clean_profile.sh > "$LOG" 2>&1 ||
  fail "Clean profile regression"
echo "✅ Clean profile regression"

bash verify/verify_macos_keychain_credentials.sh >> "$LOG" 2>&1 ||
  fail "Keychain regression"
echo "✅ Keychain regression"

echo "===== APP CHECK ====="

[ -x "$APP/Contents/MacOS/dashboard" ] ||
  fail "Beta 7 test app missing"

codesign --verify --deep --strict "$APP" ||
  fail "App signature invalid"

echo "✅ Test App signature"

if pgrep -x dashboard >/dev/null 2>&1; then
  fail "AI-OS is already running; close it before testing"
fi

echo "===== LAUNCH ====="

open -n "$APP" || fail "App launch request failed"
sleep 8

if pgrep -x dashboard >/dev/null 2>&1; then
  echo "✅ Application process started"
else
  fail "Application process not detected"
fi

echo "===== SYSTEM DIAGNOSTICS ====="

log show --last 2m --style compact \
  --predicate 'process == "dashboard" AND (eventMessage CONTAINS[c] "Keychain" OR eventMessage CONTAINS[c] "OpenClaw")' \
  2>/dev/null |
  grep -Ei 'error|denied|authorization|failed' |
  head -10 || true

echo
echo "PASS: $FEATURE — automated checks"
echo "MANUAL: Check whether a macOS password dialog appeared."
echo "MANUAL: Check Chat and Agents navigation."
echo "NOTE: Real clean-user and password-count acceptance remains pending."
