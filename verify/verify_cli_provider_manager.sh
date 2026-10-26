#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

if node_modules/.bin/esbuild verify/cli_provider_manager.behavior.ts \
  --bundle --platform=node --format=esm \
  --outfile="$tmp_dir/cli-provider-manager.mjs" >/dev/null \
  && node "$tmp_dir/cli-provider-manager.mjs"; then
  echo "✓ CLI adapter registry, login guidance and model confirmation"
else
  echo "✗ CLI Provider Manager behavior"
  echo "FAIL: CLI Provider Manager"
  exit 1
fi

if npm run build >/dev/null; then
  echo "✓ Frontend production build"
  echo "PASS: CLI Provider Manager"
else
  echo "✗ Frontend production build"
  echo "FAIL: CLI Provider Manager"
  exit 1
fi
