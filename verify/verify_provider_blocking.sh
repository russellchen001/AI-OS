#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

if node_modules/.bin/esbuild verify/provider_blocking.behavior.ts \
  --bundle --platform=node --format=esm \
  --outfile="$tmp_dir/provider-blocking.mjs" >/dev/null \
  && node "$tmp_dir/provider-blocking.mjs"; then
  echo "✓ Provider quota, credential and external-account blockers are distinct"
else
  echo "✗ Provider blocker classification"
  echo "FAIL: Provider Blocking"
  exit 1
fi

if cargo test --manifest-path src-tauri/Cargo.toml --lib canonical_error_categories_match_existing_semantics >/dev/null; then
  echo "✓ Backend attempt categories preserve quota and account blocks"
else
  echo "✗ Backend Provider categories"
  echo "FAIL: Provider Blocking"
  exit 1
fi

echo "PASS: Provider Blocking"
