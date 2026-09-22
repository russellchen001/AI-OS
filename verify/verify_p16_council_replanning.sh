#!/usr/bin/env bash

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || return 1

PASS=0
FAIL=0

ok() {
  echo "✓ $1"
  PASS=$((PASS + 1))
}

bad() {
  echo "✗ $1"
  FAIL=$((FAIL + 1))
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if npx --yes esbuild \
  verify/p16_council_replanning.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP/replanning.mjs" >/dev/null; then
  if node "$TMP/replanning.mjs"; then
    ok "Blocker triggers targeted Council replanning"
  else
    bad "Replanning behavior"
  fi
else
  bad "Replanning behavior build"
fi

if rg -q \
  'requiresUserApproval: true' \
  src/services/councilReplanning.ts; then
  ok "Revised recommendation requires user approval"
else
  bad "Approval boundary"
fi

if rg -q \
  'execution-feedback:blocker' \
  src/services/councilReplanning.ts; then
  ok "Execution blocker provenance retained"
else
  bad "Blocker provenance"
fi

if rg -q \
  'maxRounds.*2|Math\.min\(' \
  src/services/councilReplanning.ts; then
  ok "Replanning Council remains bounded"
else
  bad "Bounded replanning"
fi

if rg -q \
  'runtime\.run' \
  src/services/councilReplanning.ts; then
  ok "Existing Council Runtime reused"
else
  bad "Council Runtime reuse"
fi

if ! rg -q \
  'submitChatTask|executeChatWorkTask|SkillInvocationGateway' \
  src/services/councilReplanning.ts; then
  ok "Replanning cannot execute tasks or Skills directly"
else
  bad "Replanning execution boundary"
fi

if npx tsc --noEmit; then
  ok "TypeScript"
else
  bad "TypeScript"
fi

if git diff --check; then
  ok "Git whitespace validation"
else
  bad "Git whitespace validation"
fi

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Council Replanning Core"
  return 1 2>/dev/null || false
fi

echo "PASS: P16 Council Replanning Core"
