#!/usr/bin/env bash

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

node_modules/.bin/esbuild \
  verify/p17_arena_block2.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP_DIR/p17-arena-block2.mjs" \
  >/dev/null || exit 1

node "$TMP_DIR/p17-arena-block2.mjs"
