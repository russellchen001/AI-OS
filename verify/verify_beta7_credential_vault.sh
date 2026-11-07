#!/bin/bash
set -u
cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_credential_vault"

if cargo test --manifest-path src-tauri/Cargo.toml \
  credential_vault::tests::vault_round_trip_preserves_separate_accounts \
  -- --exact; then
  echo "✅ 保险库数据往返与账号隔离测试"
else
  echo "❌ 保险库数据测试失败"
  echo "FAIL: $FEATURE"
  exit 1
fi

if git diff --check; then
  echo "✅ Git 差异完整性检查"
else
  echo "❌ Git 差异检查失败"
  echo "FAIL: $FEATURE"
  exit 1
fi

echo "PASS: $FEATURE"
exit 0
