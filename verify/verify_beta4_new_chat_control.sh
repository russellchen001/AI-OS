#!/bin/bash

set -u

NAME="Beta.4 new-chat control"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_LOG="$(mktemp -t ai-os-new-chat-build.XXXXXX)"
TEST_LOG="$(mktemp -t ai-os-new-chat-test.XXXXXX)"
TEST_BUNDLE="${TMPDIR:-/tmp}/ai-os-new-chat-test-$$.cjs"
trap 'rm -f "$BUILD_LOG" "$TEST_LOG" "$TEST_BUNDLE"' EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: $NAME — $1"
  exit 1
}

cd "$ROOT" || fail "cannot enter repository"

if ./node_modules/.bin/esbuild verify/beta4_new_chat_control.behavior.tsx --bundle --platform=node --format=cjs --outfile="$TEST_BUNDLE" >"$TEST_LOG" 2>&1 \
  && node "$TEST_BUNDLE" >>"$TEST_LOG" 2>&1; then
  echo "✅ Rendered control semantics"
else
  tail -20 "$TEST_LOG"
  fail "rendered control behavior failed"
fi

if npm run build >"$BUILD_LOG" 2>&1; then
  echo "✅ Frontend production build"
else
  grep -E "error|Error|FAILED" "$BUILD_LOG" | head -20
  fail "frontend production build failed"
fi

if git diff --check -- src/components/Sidebar.tsx src/App.css verify/beta4_new_chat_control.behavior.tsx verify/verify_beta4_new_chat_control.sh; then
  echo "✅ Changed-file source integrity"
else
  fail "changed files contain whitespace errors"
fi

echo "PASS: $NAME"
