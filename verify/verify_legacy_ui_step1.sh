#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

OLD_PAGES=(
  BackupPage
  DashboardPage
  LogsPage
  MultiLlmPage
  PromptLibraryPage
  OpenClawPage
  ServicesPage
)

for page in "${OLD_PAGES[@]}"; do
  if grep -Fq "$page" src/App.tsx; then
    echo "❌ $page 仍存在于 App.tsx"
    echo "FAIL: legacy ui step1"
    exit 1
  fi
  echo "✅ $page 已退出 App 入口"
done

if grep -Fq 'ModelsPage' src/App.tsx; then
  echo "✅ Models 暂时保留，避免破坏 My AI 本地模型管理"
else
  echo "❌ Models 被提前删除"
  echo "FAIL: legacy ui step1"
  exit 1
fi

if grep -Fq 'McpPage' src/App.tsx; then
  echo "✅ Skills / MCP 新 UI 入口保留"
else
  echo "❌ MCP 被误删"
  echo "FAIL: legacy ui step1"
  exit 1
fi

npm run build

git diff --check

echo "✅ Frontend build"
echo "✅ Git diff check"
echo "PASS: legacy ui step1"
