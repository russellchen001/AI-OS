#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

check() {
  local name="$1"
  shift
  if "$@"; then
    echo "✓ $name"
    PASS=$((PASS + 1))
  else
    echo "✗ $name"
    FAIL=$((FAIL + 1))
  fi
}

check \
  "Dynamic assembly, Agency roles, model assignment, deliberation, recommendation and fallbacks" \
  bash -c 'node_modules/.bin/esbuild verify/p16_dynamic_council.behavior.ts --bundle --platform=node --format=esm --outfile="$1/dynamic.mjs" >/dev/null && node "$1/dynamic.mjs"' _ "$TMP_DIR"

check "TypeScript" npx tsc --noEmit
check "Frontend production build" npm run build
check "P16-1 integrations remain green" verify/verify_p16_council_integrations.sh
check "P16-2 runtime remains green" verify/verify_p16_council_runtime.sh
check "Git whitespace validation" git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Dynamic Council Core"
  exit 1
fi

echo "PASS: P16 Dynamic Council Core"
