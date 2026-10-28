#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
if node_modules/.bin/esbuild verify/beta6_artifacts_archive.behavior.ts --bundle --platform=node --format=cjs --outfile="$test_dir/archive.cjs" >/dev/null && node "$test_dir/archive.cjs"; then
  echo 'PASS: Beta6 Artifacts project archive'
  exit 0
else
  echo '✗ Artifacts archive behavior'
  echo 'FAIL: Beta6 Artifacts project archive — behavior test failed'
  exit 1
fi
