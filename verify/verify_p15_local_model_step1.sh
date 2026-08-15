#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 local model step 1: $1"
  exit 1
}

cd "$ROOT"

if npm run build >"$TMP/build.log" 2>&1; then
  echo "✓ Chat local model result UI builds"
else
  grep -E "error TS|ERROR|Build failed|failed to build" "$TMP/build.log" | head -20
  fail "frontend build"
fi

if ./verify/verify_p15_local_model_core_skill.sh >"$TMP/local-model.log" 2>&1; then
  echo "✓ Local Model Runtime contract remains valid"
else
  tail -20 "$TMP/local-model.log"
  fail "Local Model Runtime regression"
fi

echo "PASS P15 local model step 1"
exit 0
