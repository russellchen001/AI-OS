#!/usr/bin/env bash
# ============================================================
# 总验收：依次运行 verify/ 目录下所有 verify_*.sh
# 用法：./verify_all.sh
# 结果：最后两行告诉你通过几项、哪几项失败
# ============================================================

VERIFY_DIR="$(cd "$(dirname "$0")" && pwd)/verify"
PASS_COUNT=0
FAIL_COUNT=0
FAIL_LIST=""

if [ ! -d "$VERIFY_DIR" ]; then
  echo "❌ 找不到 verify/ 目录，请先创建：mkdir verify"
  exit 1
fi

SCRIPTS=$(find "$VERIFY_DIR" -maxdepth 1 -name 'verify_*.sh' | sort)

if [ -z "$SCRIPTS" ]; then
  echo "⚠️  verify/ 目录里还没有验收脚本"
  exit 1
fi

echo "════════════════════════════════════════"
echo " 开始验收  $(date '+%Y-%m-%d %H:%M')"
echo "════════════════════════════════════════"

for script in $SCRIPTS; do
  NAME=$(basename "$script" .sh | sed 's/^verify_//')
  OUTPUT=$(bash "$script" 2>&1)
  CODE=$?

  if [ $CODE -eq 0 ]; then
    PASS_COUNT=$((PASS_COUNT + 1))
    echo "✅ $NAME"
  else
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAIL_LIST="$FAIL_LIST $NAME"
    echo "❌ $NAME"
    # 失败时才打印细节，方便直接复制给 GPT
    echo "$OUTPUT" | sed 's/^/     /'
  fi
done

TOTAL=$((PASS_COUNT + FAIL_COUNT))
echo "════════════════════════════════════════"
echo " 通过 $PASS_COUNT/$TOTAL"

if [ $FAIL_COUNT -eq 0 ]; then
  echo " 结论：ALL PASS ✅"
  exit 0
else
  echo " 失败项：$FAIL_LIST"
  echo " 结论：FAIL ❌"
  exit 1
fi
