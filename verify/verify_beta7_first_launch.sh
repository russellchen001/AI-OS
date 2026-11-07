#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_first_launch"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

node verify/beta7_diagnostics.behavior.mjs || {
  echo "FAIL: $FEATURE — diagnostics fault behavior"
  exit 1
}

if node verify/beta7_first_launch.runner.mjs; then
  echo "✅ 首次启动三阶段与恢复行为通过"
else
  echo "❌ 首次启动行为测试失败"
  echo "FAIL: $FEATURE — behavior"
  exit 1
fi

if npm run build > "$WORK/build.log" 2>&1; then
  echo "✅ 前端生产构建通过"
else
  grep -E 'error|Error|FAIL' "$WORK/build.log" | head -20 || true
  echo "❌ 前端生产构建失败"
  echo "FAIL: $FEATURE — build"
  exit 1
fi

echo "PASS: $FEATURE — automated"
echo "MANUAL: 用全新本机资料确认连接、权限跳转、诊断和首条真实对话。"
