#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if grep -Fq '!agent.builtIn && (' src/pages/AgentsPage.tsx; then
  echo "✅ 非内置 Agent 显示 Delete 按钮"
else
  echo "❌ Delete 按钮显示条件缺失"
  echo "FAIL: agent registry delete visible"
  exit 1
fi

if grep -Fq 'onClick={() => deleteAgent(agent)}' src/pages/AgentsPage.tsx; then
  echo "✅ Delete 按钮连接真实删除逻辑"
else
  echo "❌ Delete 按钮未连接删除逻辑"
  echo "FAIL: agent registry delete visible"
  exit 1
fi

if grep -Fq 'if (agent.builtIn) return' src/pages/AgentsPage.tsx; then
  echo "✅ OpenClaw 保持删除保护"
else
  echo "❌ OpenClaw 删除保护缺失"
  echo "FAIL: agent registry delete visible"
  exit 1
fi

if grep -Fq '.agent-record-actions {' src/App.css; then
  echo "✅ Agent 操作按钮样式已添加"
else
  echo "❌ Agent 操作按钮样式缺失"
  echo "FAIL: agent registry delete visible"
  exit 1
fi

npm run build

echo "✅ Frontend build"
echo "PASS: agent registry delete visible"
