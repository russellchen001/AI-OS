#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_openclaw_startup"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if npm run build > "$LOG" 2>&1; then
  echo "✓ 前端构建通过"
else
  echo "✗ 前端构建失败"
  grep -E 'error|Error|FAIL' "$LOG" | head -20 || true
  echo "FAIL: $FEATURE — build"
  exit 1
fi

if git diff --check; then
  echo "✓ Git 差异完整性检查通过"
else
  echo "✗ Git 差异检查失败"
  echo "FAIL: $FEATURE — diff"
  exit 1
fi

echo "PASS: $FEATURE — 自动化构建检查"
echo "注意：实际 Keychain 弹窗行为仍需在 Beta 7 测试版中验收"
exit 0
