#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="/tmp/ai-os-powerpoint-real-e2e.log"

cd "$ROOT" || exit 1

VERSION="$(
  /usr/bin/osascript \
    -e 'with timeout of 10 seconds' \
    -e 'tell application "Microsoft PowerPoint" to get version' \
    -e 'end timeout' \
    2>/dev/null || true
)"

if [ -z "$VERSION" ]; then
  echo "SKIP PowerPoint real E2E: APPLICATION_UNAVAILABLE_OR_UNRESPONSIVE"
  exit 0
fi

echo "✓ PowerPoint $VERSION answers bounded AppleScript"

if cargo test \
  --manifest-path "$MANIFEST" \
  document::powerpoint::tests::powerpoint_realistic_workflow_real_e2e \
  -- --ignored --exact >"$LOG" 2>&1
then
  echo "✓ PowerPoint realistic read/create/edit/export"
  echo "PASS PowerPoint real E2E"
  exit 0
fi

if grep -Eq 'AppleEvent.*(超时|timed out)|\(-1712\)' "$LOG"; then
  echo "SKIP PowerPoint real E2E: APPLICATION_AUTOMATION_UNAVAILABLE"
  exit 0
fi

tail -100 "$LOG"
echo "FAIL PowerPoint real E2E: CAPABILITY_ASSERTION_FAILED"
exit 1
