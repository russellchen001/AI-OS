#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
 echo "FAIL P15 local model step 2: $1"
 exit 1
}

if npm run build >"$TMP/build.log" 2>&1; then
 echo "✓ Chat UI build"
else
 grep -E "error TS|ERROR" "$TMP/build.log" | head -20
 fail "frontend build"
fi

if ./verify/verify_p15_local_model_core_skill.sh >"$TMP/core.log" 2>&1; then
 echo "✓ Local model runtime"
else
 tail -20 "$TMP/core.log"
 fail "runtime regression"
fi

echo "PASS P15 local model step 2"
