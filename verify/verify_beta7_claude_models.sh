#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_claude_models"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
if node verify/beta7_claude_models.behavior.mjs; then
  echo "✅ Claude 模型选择行为通过"
else
  echo "❌ Claude 模型选择行为失败"; echo "FAIL: $FEATURE — model list behavior"; exit 1
fi
if cargo test --manifest-path src-tauri/Cargo.toml --lib claude_code --quiet > "$LOG" 2>&1; then
  grep 'test result:' "$LOG" || true
  echo "✅ 别名解析与模型参数防注入测试通过"
else
  grep -E '^error|FAILED|panicked|test result:' "$LOG" | head -20 || true
  echo "❌ Claude Code 原生测试失败"; echo "FAIL: $FEATURE — native behavior"; exit 1
fi
echo "PASS: $FEATURE"
echo "MANUAL: My AI > Anthropic > Manage connection > Refresh model catalog 后可选 Fable/Opus/Sonnet，逐个 Verify 后出现在聊天模型菜单。"
