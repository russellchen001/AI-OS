#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

check() {
  NAME="$1"
  shift
  if "$@"; then
    echo "✓ $NAME"
    PASS=$((PASS + 1))
  else
    echo "✗ $NAME"
    FAIL=$((FAIL + 1))
  fi
}

check \
  "Council runtime behavior, failover, synthesis, context and cancellation" \
  bash -c 'node_modules/.bin/esbuild verify/p16_council_runtime.behavior.ts --bundle --platform=node --format=esm --outfile="$1/runtime.mjs" >/dev/null && node "$1/runtime.mjs"' _ "$TMP_DIR"

check \
  "Frontend production build" \
  npm run build

check \
  "P16-1 integration verifier remains green" \
  verify/verify_p16_council_integrations.sh

check \
  "Council page delegates execution to runtime" \
  bash -c '! rg -n "streamThroughAiCenter|runMemberWithFailover|SkillInvocationGateway" src/pages/AiCouncilPage.tsx'

check \
  "Council services cannot bypass Task Engine" \
  bash -c '! rg -n "SkillInvocationGateway|executeChatWorkTask|openclaw" src/services/councilRuntime.ts src/services/councilMemberContext.ts src/services/councilTaskHandoff.ts'

check \
  "Git whitespace validation" \
  git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Council Runtime"
  exit 1
fi

echo "PASS: P16 Council Runtime"
