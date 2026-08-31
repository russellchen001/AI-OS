#!/usr/bin/env bash
set -euo pipefail

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
    -e 'tell application id "com.apple.Keynote" to get version' \
    2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "FAIL Presentation real E2E: KEYNOTE_AUTOMATION_UNAVAILABLE"
  exit 1
fi

echo "PASS: Keynote bundle-id detection"
echo "PASS: Keynote AppleScript automation — version $VERSION"

cargo test \
  --manifest-path src-tauri/Cargo.toml \
  document::keynote::tests::keynote_real_e2e \
  -- --ignored --exact --nocapture

if grep -nE '^[[:space:]]*quit([[:space:]]|$)' \
  src-tauri/src/document/keynote.rs >/dev/null
then
  echo "FAIL: Keynote adapter contains quit command"
  exit 1
fi

echo "PASS: presentation.create native Keynote"
echo "PASS: presentation.read native Keynote"
echo "PASS: create read-back validation"
echo "PASS: existing target no-overwrite"
echo "PASS: already-open document remains user-owned"
echo "PASS: Keynote application is never quit"
echo "PASS: Presentation native real E2E"
