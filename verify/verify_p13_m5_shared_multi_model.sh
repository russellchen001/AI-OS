#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: P13-M5 shared multi-model invocation"
  exit 1
}

check() {
  grep -Fq "$1" "$2" || fail "$3"
  echo "✅ $4"
}

check "export type AiCenterMultiParticipant" src/services/aiCenter.ts \
  "多模型参与者契约缺失" "多模型参与者契约存在"

check "function normalizeMultiParticipants" src/services/aiCenter.ts \
  "有序去重逻辑缺失" "参与者有序去重存在"

check "seenParticipantIds.has(participantId)" src/services/aiCenter.ts \
  "参与者 ID 去重缺失" "重复参与者 ID 会被过滤"

check "seenModels.has(modelKey)" src/services/aiCenter.ts \
  "模型去重缺失" "重复模型会被过滤"

check "export function invokeMultipleThroughAiCenter(" src/services/aiCenter.ts \
  "共享多模型接口缺失" "共享多模型接口已导出"

check "const participantResults = Promise.all(" src/services/aiCenter.ts \
  "并发执行缺失" "参与模型并发执行"

check "activeStreams.set(operationId, stream)" src/services/aiCenter.ts \
  "独立 operation ID 缺失" "参与者拥有独立 operation ID"

check "response: outcome.response" src/services/aiCenter.ts \
  "M4 metadata 复用缺失" "成功结果复用 M4 metadata"

check "await Promise.allSettled(" src/services/aiCenter.ts \
  "取消隔离缺失" "整体取消具备失败隔离"

check "participantCount: participants.length" src/services/aiCenter.ts \
  "聚合统计缺失" "聚合结果包含参与者统计"

check "**P13-M5:** Completed" \
  docs/Milestones/P13-M5_SHARED_MULTI_MODEL_INVOCATION.md \
  "M5 完成状态缺失" "M5 正式文档标记完成"

check "**P13 AI Center:** Completed" \
  docs/Milestones/P13-M5_SHARED_MULTI_MODEL_INVOCATION.md \
  "P13 完成状态缺失" "P13 正式标记完成"

check "AI-OS v1.0 has one operational execution Agent: OpenClaw." HANDOFF.md \
  "Agent v1.0 边界缺失" "v1.0 Agent 边界已记录"

npm run build
echo "✅ Frontend production build"

cargo test --manifest-path src-tauri/Cargo.toml --lib
echo "✅ Full Rust library tests"

cargo check --manifest-path src-tauri/Cargo.toml
echo "✅ Cargo check"

cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
echo "✅ Rust formatting"

git diff --check
echo "✅ Git diff check"

echo "PASS: P13-M5 shared multi-model invocation"
