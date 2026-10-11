#!/bin/bash

set -u

NAME="Beta.3 chat experience"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_LOG="$(mktemp -t ai-os-beta3-build.XXXXXX)"
TEST_LOG="$(mktemp -t ai-os-beta3-test.XXXXXX)"
trap 'rm -f "$BUILD_LOG" "$TEST_LOG"' EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: $NAME — $1"
  exit 1
}

cd "$ROOT" || fail "cannot enter repository"

if npm run build >"$BUILD_LOG" 2>&1; then
  echo "✅ Frontend production build"
else
  tail -20 "$BUILD_LOG"
  fail "frontend production build failed"
fi

if (cd src-tauri && cargo test providers::tests:: --lib >"$TEST_LOG" 2>&1); then
  echo "✅ Provider regression tests"
else
  grep -E "FAILED|^error" "$TEST_LOG" | head -20
  fail "Provider regression tests failed"
fi

if git diff --check -- src/pages/ChatPage.tsx src/components/MarkdownRenderer.tsx src/App.css src-tauri/src/providers.rs; then
  echo "✅ Changed-file source integrity"
else
  fail "changed files contain whitespace errors"
fi

echo "PASS: $NAME"
