#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_grok_account"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
if node verify/beta7_grok_account.behavior.mjs; then
  echo "✅ Grok 账号连接行为通过"
else
  echo "❌ Grok 账号连接行为失败"
  echo "FAIL: $FEATURE — connection behavior"
  exit 1
fi
if cargo test --manifest-path src-tauri/Cargo.toml --lib grok --quiet > "$LOG" 2>&1; then
  grep 'test result:' "$LOG" || true
  echo "✅ Grok 认证方式与 CLI 状态测试通过"
else
  grep -E '^error|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ Grok 认证方式与 CLI 状态测试失败"
  echo "FAIL: $FEATURE — native Grok behavior"
  exit 1
fi
echo "PASS: $FEATURE"
echo "MANUAL: 在未登录 Grok CLI 的机器上打开 My AI > xAI > Sign in with Grok > Verify，应看到 grok login 指引；不代用户登录。"
