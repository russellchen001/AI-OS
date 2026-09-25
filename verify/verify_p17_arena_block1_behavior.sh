#!/usr/bin/env bash

TMP_DIR="$(mktemp -d /tmp/ai-os-p17-arena-block1-behavior.XXXXXX)"

cleanup() {
  rm -rf "$TMP_DIR"
}

trap cleanup EXIT

echo "============================================================"
echo "P17 BLOCK 1 — BEHAVIOR VERIFIER"
echo "============================================================"

if ! node_modules/.bin/esbuild \
  verify/p17_arena_block1.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP_DIR/p17-arena-block1.mjs" \
  >/dev/null
then
  echo "FAIL — behavior test bundle"
  exit 1
fi

echo "PASS — behavior test bundle"

node "$TMP_DIR/p17-arena-block1.mjs"
STATUS=$?

if [ "$STATUS" -ne 0 ]; then
  echo "FAIL — P17 Block 1 behavior"
  exit "$STATUS"
fi

echo "PASS — P17 Block 1 behavior"
