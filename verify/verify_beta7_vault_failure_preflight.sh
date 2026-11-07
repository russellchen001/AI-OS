#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_vault_failure_preflight"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if cargo test --manifest-path src-tauri/Cargo.toml \
  credential_vault::tests --quiet > "$LOG" 2>&1; then
  echo "✅ 保险库单元测试通过"
  grep 'test result:' "$LOG" | head -1 || true
else
  grep -E '^error|FAILED|test result:' "$LOG" | head -20 || true
  echo "❌ 保险库测试失败"
  echo "FAIL: $FEATURE"
  exit 1
fi

if cargo test --manifest-path src-tauri/Cargo.toml \
  --test beta7_vault_keychain_live --no-run > "$LOG" 2>&1; then
  echo "✅ Keychain 集成测试编译通过"
else
  grep -E '^error|FAILED' "$LOG" | head -20 || true
  echo "❌ Keychain 集成测试编译失败"
  echo "FAIL: $FEATURE"
  exit 1
fi

if git diff --check; then
  echo "✅ Git 差异检查通过"
else
  echo "❌ Git 差异检查失败"
  echo "FAIL: $FEATURE"
  exit 1
fi

echo "PASS: $FEATURE"
