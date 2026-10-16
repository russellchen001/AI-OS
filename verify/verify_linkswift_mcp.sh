#!/bin/bash

set -u

NAME="LinkSwift MCP managed component"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d -t ai-os-linkswift-mcp.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: $NAME — $1"
  exit 1
}

cd "$ROOT" || fail "cannot enter repository"

if ./node_modules/.bin/esbuild verify/linkswift_mcp.behavior.ts --bundle --platform=node --format=esm --outfile="$TMP_DIR/linkswift-mcp.mjs" >"$TMP_DIR/esbuild.log" 2>&1 \
  && node "$TMP_DIR/linkswift-mcp.mjs" >"$TMP_DIR/behavior.log" 2>&1; then
  echo "✅ Supported-link detection and consent disclosure"
else
  tail -20 "$TMP_DIR/esbuild.log" "$TMP_DIR/behavior.log" 2>/dev/null
  fail "frontend behavior"
fi

if (cd src-tauri && cargo test linkswift_mcp::tests --lib >"$TMP_DIR/cargo-test.log" 2>&1); then
  echo "✅ Decline, verified install, SHA failure, rollback and MCP invocation"
else
  grep -E "FAILED|^error" "$TMP_DIR/cargo-test.log" | head -20
  fail "Rust behavior tests"
fi

if (cd src-tauri && cargo check --lib >"$TMP_DIR/cargo-check.log" 2>&1); then
  echo "✅ Rust compile"
else
  grep -E "^error|error\[" "$TMP_DIR/cargo-check.log" | head -20
  fail "Rust compile"
fi

if npm run build >"$TMP_DIR/frontend-build.log" 2>&1; then
  echo "✅ Frontend production build"
else
  grep -E "error|Error|FAILED" "$TMP_DIR/frontend-build.log" | head -20
  fail "frontend production build"
fi

if git diff --check; then
  echo "✅ Changed-file integrity"
else
  fail "whitespace errors"
fi

echo "PASS: $NAME"
exit 0
