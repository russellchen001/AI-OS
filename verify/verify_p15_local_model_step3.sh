#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
 echo "FAIL P15 local model step 3: $1"
 exit 1
}

if npm run build >"$TMP/build.log" 2>&1; then
 echo "✓ My AI local model UI builds"
else
 grep -E "error TS|ERROR" "$TMP/build.log" | head -20
 fail "frontend build"
fi

OMLX_KEY_FILE="$HOME/.openclaw/ai-os-secrets/omlx-api-key"
if [ ! -s "$OMLX_KEY_FILE" ]; then
 fail "oMLX API key is unavailable"
fi

if curl --silent --fail --max-time 5 \
 -H "Authorization: Bearer $(<"$OMLX_KEY_FILE")" \
 http://127.0.0.1:8000/v1/models \
 >/dev/null; then
 echo "✓ oMLX API reachable"
else
 fail "oMLX unavailable"
fi

echo "PASS P15 local model step 3"
