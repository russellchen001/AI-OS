#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
if [ -z "${AI_OS_REMOTE_LINCO_TOKEN:-}" ]; then
  echo "FAIL: AI_OS_REMOTE_LINCO_TOKEN is required"
  exit 1
fi
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if ! node_modules/.bin/esbuild verify/p16_desktop_linco_live.behavior.ts \
  --bundle --platform=node --format=esm --outfile="$TMP_DIR/desktop-linco-live.mjs" >/dev/null; then
  echo "FAIL: P16 Desktop Linco live verifier build"
  exit 1
fi

if node "$TMP_DIR/desktop-linco-live.mjs"; then
  echo "PASS: P16 Desktop Linco live E2E"
else
  echo "FAIL: P16 Desktop Linco live E2E"
  exit 1
fi
