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

for SPEC in \
  "Pages:com.apple.Pages" \
  "Numbers:com.apple.Numbers" \
  "Keynote:com.apple.Keynote"
do
  APP="${SPEC%%:*}"
  BUNDLE_ID="${SPEC#*:}"

  if ! has_bundle_id "$BUNDLE_ID"; then
    echo "SKIP iWork real E2E: APP_NOT_INSTALLED:$APP"
    exit 0
  fi

  VERSION="$(
    /usr/bin/osascript \
      -e "tell application id \"$BUNDLE_ID\" to get version" \
      2>/dev/null || true
  )"

  if [ -z "$VERSION" ]; then
    echo "FAIL iWork real E2E: AUTOMATION_PERMISSION_OR_APP_FAILURE:$APP"
    exit 1
  fi

  echo "PASS: $APP bundle-id detection"
  echo "PASS: $APP AppleScript automation — version $VERSION"
done

echo "PASS: Apple iWork real E2E"
