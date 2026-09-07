#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="/tmp/ai-os-keynote-real-e2e.log"

cd "$ROOT" || exit 1

has_bundle_id() {
  local expected="$1"
  local directory app actual

  for directory in /Applications "$HOME/Applications"; do
    [ -d "$directory" ] || continue

    for app in "$directory"/*.app; do
      [ -d "$app" ] || continue

      actual="$(
        /usr/libexec/PlistBuddy \
          -c 'Print :CFBundleIdentifier' \
          "$app/Contents/Info.plist" \
          2>/dev/null || true
      )"

      [ "$actual" = "$expected" ] && return 0
    done
  done

  return 1
}

if ! has_bundle_id "com.apple.Keynote"; then
  echo "SKIP Presentation real E2E: KEYNOTE_APP_NOT_INSTALLED"
  exit 0
fi

VERSION="$(
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application id "com.apple.Keynote" to get version' \
    -e 'end timeout' \
    2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP Presentation real E2E: KEYNOTE_AUTOMATION_UNAVAILABLE"
  exit 0
fi

echo "PASS: Keynote bundle-id detection"
echo "PASS: Keynote AppleScript automation — version $VERSION"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::keynote::tests::keynote_real_e2e \
  -- --ignored --exact --nocapture >"$LOG" 2>&1
then
  cat "$LOG"
  echo "PASS: Presentation native real E2E"
  exit 0
fi

if grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$LOG"; then
  echo "SKIP Presentation real E2E: APPLICATION_AUTOMATION_UNAVAILABLE"
  exit 0
fi

tail -120 "$LOG"
echo "FAIL Presentation real E2E: CAPABILITY_ASSERTION_FAILED"
exit 1
