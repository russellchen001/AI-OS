#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_claude_setup"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
if node verify/beta7_claude_setup.behavior.mjs; then
  echo "✅ Claude Code 首次连接行为通过"
else
  echo "❌ Claude Code 首次连接行为失败"
  echo "FAIL: $FEATURE — connection behavior"
  exit 1
fi
if cargo test --manifest-path src-tauri/Cargo.toml --lib claude_setup --quiet > "$LOG" 2>&1; then
  grep 'test result:' "$LOG" || true
  echo "✅ 原生安装授权与校验测试通过"
else
  grep -E '^error|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ 原生安装授权与校验测试失败"
  echo "FAIL: $FEATURE — native install behavior"
  exit 1
fi
echo "PASS: $FEATURE"
echo "MANUAL: 全新 macOS 用户批准安装、在官方页面登录并验证连接；不自动改动真实账号。"
