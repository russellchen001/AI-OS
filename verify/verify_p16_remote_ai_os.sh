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
  "Session continuity, ASK/DO, Council, confirmation, replanning, simulation and cancellation" \
  bash -c 'node_modules/.bin/esbuild verify/p16_remote_ai_os.behavior.ts --bundle --platform=node --format=esm --outfile="$1/remote.mjs" >/dev/null && node "$1/remote.mjs"' _ "$TMP_DIR"

check \
  "Linco is absent from Council reasoning context" \
  bash -c '! rg -n "loadLincoContext|linco-bridge:connectivity" src/services/councilMemberContext.ts'

check \
  "Remote gateway has no direct OpenClaw transport dependency" \
  bash -c '! rg -n "OpenClaw|openclaw_gateway|AgentExecutionAdapter" src/services/remoteAiOs.ts src/services/integrations/lincoRemoteTransportAdapter.ts'

check \
  "Remote gateway has no SkillInvocationGateway shortcut" \
  bash -c '! rg -n "SkillInvocationGateway|SkillInvocationRequest" src/services/remoteAiOs.ts src/services/integrations/lincoRemoteTransportAdapter.ts'

check "TypeScript" npx tsc --noEmit
check "Git whitespace validation" git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Remote AI-OS / Linco"
  exit 1
fi

echo "PASS: P16 Remote AI-OS / Linco"
