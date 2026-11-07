#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

FEATURE="beta7_chat_jump"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if npm run build > "$LOG" 2>&1; then
  echo "✅ 聊天界面生产构建通过"
else
  grep -E 'error|Error|FAIL' "$LOG" | head -20 || true
  echo "❌ 聊天界面构建失败"
  echo "FAIL: $FEATURE — build"
  exit 1
fi

if git diff --check; then
  echo "✅ Git 差异检查通过"
else
  echo "❌ Git 差异检查失败"
  echo "FAIL: $FEATURE — diff"
  exit 1
fi

echo "PASS: $FEATURE — automated build"
echo "MANUAL: 长对话中确认顶部/底部按钮可达；向上阅读时流式输出不抢走位置；跳到底部后恢复跟随。"
exit 0
