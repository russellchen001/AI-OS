#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_vault_lock_contention"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if cargo test --manifest-path src-tauri/Cargo.toml \
  --test beta7_vault_lock_contention \
  -- --test-threads=1 > "$LOG" 2>&1; then
  echo "✅ 双进程锁竞争测试通过"
  grep 'test result:' "$LOG" | tail -1
else
  grep -E 'error:|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ 双进程锁竞争测试失败"
  echo "FAIL: $FEATURE — contention"
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
