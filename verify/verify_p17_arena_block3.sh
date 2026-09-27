#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"

echo "=== P17 BLOCK 3 — ARENA GOVERNANCE & EVALUATION ==="

node_modules/.bin/esbuild \
  verify/p17_arena_block3.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP/p17-block3.mjs" \
  >/dev/null

node "$TMP/p17-block3.mjs"

./verify/verify_p17_arena_governance.sh
./verify/verify_p17_arena_evaluation.sh

echo "===== BLOCK 2 REGRESSION ====="
./verify/verify_p17_arena_block2.sh
./verify/verify_p17_arena_block2_behavior.sh

npm run build >/dev/null
echo "PASS — frontend TypeScript build"

git diff --check
echo "PASS — git diff --check"

echo "P17 BLOCK 3 VERIFIER: PASS"
