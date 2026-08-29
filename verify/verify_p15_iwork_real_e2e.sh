#!/usr/bin/env bash
set -u

has_bundle_id() {
  local expected="$1"
  local directory app actual
  for directory in /Applications "$HOME/Applications"; do
    [ -d "$directory" ] || continue
    for app in "$directory"/*.app; do
      [ -d "$app" ] || continue
      actual="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist" 2>/dev/null || true)"
      [ "$actual" = "$expected" ] && return 0
    done
  done
  return 1
}

MISSING=""
for SPEC in "Pages:com.apple.Pages" "Numbers:com.apple.Numbers" "Keynote:com.apple.Keynote"; do
  APP="${SPEC%%:*}"
  BUNDLE_ID="${SPEC#*:}"
  if ! has_bundle_id "$BUNDLE_ID" && [ ! -d "/Applications/$APP.app" ]; then
    MISSING="$MISSING $APP"
  fi
done
if [ -n "$MISSING" ]; then
  echo "SKIP iWork real E2E: APP_NOT_INSTALLED:$MISSING"
  exit 0
fi
echo "SKIP iWork real E2E: NATIVE_AUTOMATION_PERMISSION_REQUIRED"
exit 0
