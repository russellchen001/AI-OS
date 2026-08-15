#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
 echo "FAIL P15 local model step 4: $1"
 exit 1
}

if rg -n "window\\.prompt|window\\.confirm" src/pages/MyAiPage.tsx >/dev/null; then
 echo "Found browser native dialogs"
 fail "native dialog remains"
else
 echo "✓ Browser dialogs removed"
fi

if npm run build >"$TMP/build.log" 2>&1; then
 echo "✓ My AI dialog UI builds"
else
 grep -E "error TS|ERROR" "$TMP/build.log" | head -20
 fail "frontend build"
fi

echo "PASS P15 local model step 4"
exit 0
