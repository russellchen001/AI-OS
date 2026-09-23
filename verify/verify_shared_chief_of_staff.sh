#!/usr/bin/env bash

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if ! node_modules/.bin/esbuild \
  verify/shared_chief_of_staff.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP_DIR/shared-chief-of-staff.mjs" >/dev/null; then
  echo "FAIL: Shared Chief of Staff verifier build"
  exit 1
fi

node "$TMP_DIR/shared-chief-of-staff.mjs"
