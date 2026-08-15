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

if curl --silent --fail --max-time 5 \
 http://127.0.0.1:11434/api/tags \
 >/dev/null; then
 echo "✓ Ollama API reachable"
else
 fail "Ollama unavailable"
fi

echo "PASS P15 local model step 3"
