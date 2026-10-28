#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
if node_modules/.bin/esbuild verify/beta6_render_security.behavior.tsx --bundle --platform=node --format=cjs --loader:.css=empty --outfile="$test_dir/render.cjs" >/dev/null && node "$test_dir/render.cjs"; then
  echo 'PASS: Beta6 production render security'
  exit 0
else
  echo '✗ production renderer behavior'
  echo 'FAIL: Beta6 production render security — renderer behavior failed'
  exit 1
fi
