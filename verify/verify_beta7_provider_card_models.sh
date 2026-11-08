#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_provider_card_models"
if node verify/beta7_provider_card_models.behavior.mjs; then
  echo "✅ My AI 卡片模型显示行为通过"
else
  echo "❌ My AI 卡片模型显示行为失败"
  echo "FAIL: $FEATURE — card model behavior"
  exit 1
fi
echo "PASS: $FEATURE"
echo "MANUAL: My AI 已连接的卡片显示该连接验证过的模型（如 OpenAI 显示 GPT-6.1-Sol），未连接卡片不显示型号。"
