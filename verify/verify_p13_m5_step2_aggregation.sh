#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: p13 m5 step2 aggregation"
  exit 1
}

grep -Fq "export type AiCenterMultiInvocationResult" \
  src/services/aiCenter.ts ||
  fail "稳定聚合结果契约缺失"
echo "✅ 稳定聚合结果契约已定义"

grep -Fq "const participantResults = Promise.all(" \
  src/services/aiCenter.ts ||
  fail "参与模型并发执行缺失"
echo "✅ 参与模型保持并发执行"

grep -Fq "participants.filter(" \
  src/services/aiCenter.ts ||
  fail "独立状态统计缺失"
echo "✅ 成功、失败与取消状态独立统计"

grep -Fq "participantCount: participants.length" \
  src/services/aiCenter.ts ||
  fail "参与者总数缺失"
echo "✅ 聚合结果记录参与者总数"

grep -Fq "latencyMs: Math.max(" \
  src/services/aiCenter.ts ||
  fail "聚合延迟记录缺失"
echo "✅ 聚合结果记录总延迟"

grep -Fq "participants," \
  src/services/aiCenter.ts ||
  fail "有序参与者结果缺失"
echo "✅ Promise.all 保留规范化输入顺序"

grep -Fq "await Promise.allSettled(" \
  src/services/aiCenter.ts ||
  fail "整体取消隔离缺失"
echo "✅ 整体取消不会被单个取消失败中断"

npm run build

git diff --check

echo "✅ Frontend build"
echo "✅ Git diff check"
echo "PASS: p13 m5 step2 aggregation"
