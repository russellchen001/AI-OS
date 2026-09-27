#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"

echo "=== P17 FINAL INTEGRATION & ACCEPTANCE ==="

node_modules/.bin/esbuild \
  verify/p17_arena_final.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP/p17-final.mjs" \
  >/dev/null

node "$TMP/p17-final.mjs"

rg -q 'createArenaProductSession' src/pages/AiArenaPage.tsx
rg -q 'ArenaRuntimeControls' src/pages/AiArenaPage.tsx
rg -q 'ArenaOutcomePanel' src/pages/AiArenaPage.tsx
rg -q 'Start Arena' src/components/arena/ArenaRuntimeControls.tsx
rg -q 'Public Arena transcript' src/components/arena/ArenaOutcomePanel.tsx
echo "PASS — Arena product UI is wired to Product Session / Runtime / Outcome"

npm run build >/dev/null
echo "PASS — frontend TypeScript production build"

./verify/verify_p17_arena_block1.sh
./verify/verify_p17_arena_block1_behavior.sh
./verify/verify_p17_arena_block3.sh

git diff --check
echo "PASS — git diff --check"

echo "P17 FINAL INTEGRATION VERIFIER: PASS"
