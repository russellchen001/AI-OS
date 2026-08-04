#!/usr/bin/env bash
# ============================================================
# 生成"给 GPT 的上下文包"，开新对话时整段粘贴过去
# 用法：./context.sh          （直接看）
#      ./context.sh | pbcopy （Mac 直接复制到剪贴板）
#      ./context.sh | xclip -selection clipboard  （Linux）
# ============================================================

DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== 项目当前状态 ==="
echo ""

echo "--- 最近提交 ---"
git log --oneline -15 2>/dev/null || echo "(未使用 git)"
echo ""

echo "--- 未提交的改动 ---"
git status --short 2>/dev/null || echo "(未使用 git)"
echo ""

echo "--- 目录结构 ---"
if command -v tree > /dev/null 2>&1; then
  tree -L 2 -I 'node_modules|.git|dist|build|__pycache__|venv'
else
  find . -maxdepth 2 \
    -not -path './.git/*' -not -path './node_modules/*' \
    -not -path './dist/*' -not -path './venv/*' | sort
fi
echo ""

echo "--- HANDOFF.md ---"
if [ -f "$DIR/HANDOFF.md" ]; then
  cat "$DIR/HANDOFF.md"
else
  echo "(还没有 HANDOFF.md)"
fi
echo ""

echo "=== 以上是已完成的工作，请勿重复实现 ==="
