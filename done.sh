#!/usr/bin/env bash
# ============================================================
# 收尾脚本：验收 → 通过就提交 → 记入 HANDOFF.md
# 用法：./done.sh "完成了用户登录功能"
# 验收不通过会停下，不会提交
# ============================================================

DIR="$(cd "$(dirname "$0")" && pwd)"
MSG="$1"
HANDOFF="$DIR/HANDOFF.md"

if [ -z "$MSG" ]; then
  echo "❌ 请写一句改动说明，例如："
  echo "   ./done.sh \"完成了用户登录功能\""
  exit 1
fi

# ---- 第一步：跑验收 --------------------------------------
bash "$DIR/verify_all.sh"
if [ $? -ne 0 ]; then
  echo ""
  echo "🛑 验收未通过，已停止，不会提交。"
  echo "   把上面的失败信息整段复制给 GPT 即可。"
  exit 1
fi

# ---- 第二步：确认有东西要提交 ----------------------------
if [ -z "$(git status --porcelain 2>/dev/null)" ]; then
  echo ""
  echo "ℹ️  没有检测到代码改动，无需提交。"
  exit 0
fi

# ---- 第三步：写入 HANDOFF.md 变更日志 --------------------
NOW=$(date '+%Y-%m-%d %H:%M')

if [ ! -f "$HANDOFF" ]; then
  printf '# 项目状态\n\n## 已完成\n\n## 进行中\n\n## 已否决方案\n\n## 变更日志\n' > "$HANDOFF"
fi

if ! grep -q '^## 变更日志' "$HANDOFF"; then
  printf '\n## 变更日志\n' >> "$HANDOFF"
fi

printf -- '- %s  %s\n' "$NOW" "$MSG" >> "$HANDOFF"
echo ""
echo "📝 已记入 HANDOFF.md"

# ---- 第四步：提交 ----------------------------------------
git add -A
git commit -q -m "$MSG"

echo "✅ 已提交：$MSG"
echo ""
git log --oneline -5
