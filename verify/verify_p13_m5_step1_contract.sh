#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if grep -Fq "export type AiCenterMultiInvocation" src/services/aiCenter.ts; then
  echo "✅ 多模型调用契约已定义"
else
  echo "❌ 多模型调用契约缺失"
  echo "FAIL: p13 m5 step1 contract"
  exit 1
fi

if grep -Fq "function normalizeMultiParticipants" src/services/aiCenter.ts; then
  echo "✅ 参与者规范化已实现"
else
  echo "❌ 参与者规范化缺失"
  echo "FAIL: p13 m5 step1 contract"
  exit 1
fi

if grep -Fq "seenModels.has(modelKey)" src/services/aiCenter.ts; then
  echo "✅ 重复模型会被过滤"
else
  echo "❌ 模型去重缺失"
  echo "FAIL: p13 m5 step1 contract"
  exit 1
fi

if grep -Fq "seenParticipantIds.has(participantId)" src/services/aiCenter.ts; then
  echo "✅ 重复参与者 ID 会被过滤"
else
  echo "❌ 参与者 ID 去重缺失"
  echo "FAIL: p13 m5 step1 contract"
  exit 1
fi

npm run build

echo "✅ Frontend build"
echo "PASS: p13 m5 step1 contract"
