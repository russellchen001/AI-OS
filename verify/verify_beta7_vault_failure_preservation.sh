#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_vault_failure_preservation"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if cargo test --manifest-path src-tauri/Cargo.toml \
  --test beta7_vault_keychain_live \
  rejected_vault_writes_preserve_existing_keychain_value \
  -- --exact > "$LOG" 2>&1; then
  echo "✅ 无效保险库写入被拒绝，原 Keychain 记录保持不变"
  grep 'test result:' "$LOG" | tail -1 || true
else
  grep -E '^error|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ 写入失败保护测试未通过"
  echo "FAIL: $FEATURE — preservation"
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
