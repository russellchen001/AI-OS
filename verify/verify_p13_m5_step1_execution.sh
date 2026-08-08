#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

check() {
  local pattern="$1"
  local message="$2"

  if grep -Fq "$pattern" src/services/aiCenter.ts; then
    echo "✅ $message"
  else
    echo "❌ $message"
    echo "FAIL: p13 m5 step1 execution"
    exit 1
  fi
}

check "export function invokeMultipleThroughAiCenter(" \
  "共享多模型调用接口已导出"

check "Promise.all(" \
  "所有参与模型并发启动"

check "await Promise.allSettled(" \
  "整体取消隔离单个取消失败"

check "activeStreams.set(operationId, stream)" \
  "每个参与者拥有独立 operationId"

check "participants: AiCenterMultiParticipant[]" \
  "调用接收显式参与模型"

check "response: outcome.response" \
  "成功结果复用 M4 invocation metadata"

npm run build

echo "✅ Frontend build"
echo "PASS: p13 m5 step1 execution"
