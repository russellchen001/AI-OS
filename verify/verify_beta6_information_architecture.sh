#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

if node_modules/.bin/esbuild verify/beta6_information_architecture.behavior.tsx \
  --bundle --platform=node --format=cjs --outfile="$tmp_dir/information-architecture.cjs" >/dev/null \
  && node "$tmp_dir/information-architecture.cjs"; then
  echo "✓ Five primary navigation entries and legacy-route activation"
else
  echo "✗ Navigation behavior"
  echo "FAIL: Beta 6 information architecture"
  exit 1
fi

if npm run build >/dev/null; then
  echo "✓ Frontend production build"
else
  echo "✗ Frontend production build"
  echo "FAIL: Beta 6 information architecture"
  exit 1
fi

echo "PASS: Beta 6 information architecture"
