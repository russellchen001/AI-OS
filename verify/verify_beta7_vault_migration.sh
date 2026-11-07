#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_vault_migration"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if cargo test --manifest-path src-tauri/Cargo.toml \
  --test beta7_vault_keychain_live \
  legacy_keychain_migration_is_non_destructive \
  -- --exact > "$LOG" 2>&1; then
  echo "✅ 旧 Keychain 凭据已复制到隔离保险库且原记录保留"
  grep 'test result:' "$LOG" | tail -1 || true
else
  grep -E '^error|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ 非破坏性迁移测试未通过"
  echo "FAIL: $FEATURE — migration"
  exit 1
fi

if cargo test --manifest-path src-tauri/Cargo.toml \
  credential_vault::tests --quiet > "$LOG" 2>&1; then
  echo "✅ 保险库单元回归通过"
else
  grep -E '^error|FAILED|test result:' "$LOG" | head -20 || true
  echo "❌ 保险库单元回归失败"
  echo "FAIL: $FEATURE — unit"
  exit 1
fi

if git diff --check; then
  echo "✅ Git 差异检查通过"
else
  echo "❌ Git 差异检查失败"
  echo "FAIL: $FEATURE — diff"
  exit 1
fi

echo "PASS: $FEATURE"
exit 0
