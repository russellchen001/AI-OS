#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"

echo "=== P17 ARENA STRUCTURED EVALUATION VERIFY ==="

node_modules/.bin/esbuild \
  verify/p17_arena_evaluation.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP/p17-evaluation.mjs" \
  >/dev/null

node "$TMP/p17-evaluation.mjs"

npm run build >/dev/null
echo "PASS — frontend TypeScript build"

./verify/verify_p17_arena_governance.sh

./verify/verify_p17_arena_block2.sh
./verify/verify_p17_arena_block2_behavior.sh

git diff --check
echo "PASS — git diff --check"

echo "P17 ARENA STRUCTURED EVALUATION VERIFIER: PASS"
