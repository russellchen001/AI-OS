#!/bin/bash

set -u

NAME="Beta 6 Provider stability"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d -t ai-os-beta6-provider.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: $NAME — $1"
  exit 1
}

cd "$ROOT" || fail "cannot enter repository"

if ./node_modules/.bin/esbuild verify/beta6_provider_stability.behavior.ts \
  --bundle --platform=node --format=esm --outfile="$TMP_DIR/provider.mjs" >/dev/null 2>&1 \
  && node "$TMP_DIR/provider.mjs" >/dev/null 2>&1; then
  echo "✅ Chat model catalog excludes non-chat and disabled models"
else
  fail "model catalog behavior failed"
fi

if cargo test --manifest-path src-tauri/Cargo.toml claude_code --lib 2>&1 \
  | grep -q "test result: ok"; then
  echo "✅ Claude Code bridge tests"
else
  fail "Claude Code bridge tests failed"
fi

if cargo test --manifest-path src-tauri/Cargo.toml oauth_loopback_parser --lib 2>&1 \
  | grep -q "test result: ok"; then
  echo "✅ OAuth callback error-path tests"
else
  fail "OAuth callback tests failed"
fi

if npm run build >/dev/null 2>&1; then
  echo "✅ Frontend production build"
else
  fail "frontend production build failed"
fi

if git diff --check; then
  echo "✅ Changed-file source integrity"
else
  fail "changed files contain whitespace errors"
fi

echo "PASS: $NAME"
