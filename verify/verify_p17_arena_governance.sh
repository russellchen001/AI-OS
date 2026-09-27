#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"

echo "=== P17 ARENA GOVERNANCE VERIFY ==="

test -f src/types/arenaGovernance.ts
echo "PASS — governance contract exists"

test -f src/services/arenaGovernance.ts
echo "PASS — governance aggregator exists"

if rg -n \
  'taskEngine|openclaw|SkillInvocation|streamThroughAiCenter|answerThroughAiCenter' \
  src/services/arenaGovernance.ts
then
  echo "FAIL — governance owns execution authority"
  exit 1
fi

echo "PASS — governance has no execution authority"

node_modules/.bin/esbuild \
  verify/p17_arena_governance.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP/p17-governance.mjs" \
  >/dev/null

node "$TMP/p17-governance.mjs"

npm run build >/dev/null
echo "PASS — frontend TypeScript build"

git diff --check
echo "PASS — git diff --check"

echo "P17 ARENA GOVERNANCE VERIFIER: PASS"
