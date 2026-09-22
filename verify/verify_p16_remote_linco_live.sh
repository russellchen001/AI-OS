#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if ! node_modules/.bin/esbuild verify/p16_remote_linco_live.behavior.ts \
  --bundle --platform=node --format=esm --outfile="$TMP_DIR/linco-live.mjs" >/dev/null; then
  echo "FAIL: P16 Remote Linco live verifier build"
  exit 1
fi

node "$TMP_DIR/linco-live.mjs"
RC=$?

if [ "$RC" -eq 0 ]; then
  echo "PASS: P16 Remote Linco live transport"
elif [ "$RC" -eq 2 ]; then
  echo "SKIP: P16 Remote Linco live transport"
else
  echo "FAIL: P16 Remote Linco live transport"
fi

exit "$RC"
