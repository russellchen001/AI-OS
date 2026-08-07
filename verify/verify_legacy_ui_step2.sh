#!/bin/bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: legacy ui step2"
  exit 1
}

PAGE_BLOCK="$(sed -n '/export type PageName =/,/export type ThemeMode =/p' src/types/index.ts)"

for page in Dashboard Services OpenClaw Backup Logs MultiLLM "Prompt Library"; do
  if printf '%s\n' "$PAGE_BLOCK" | grep -Fq "\"$page\""; then
    fail "$page 仍存在于 PageName"
  fi
  echo "✅ $page 已退出 PageName"
done

for title in Runtime OpenClaw Backups Logs "Advanced AI" Prompts "System overview"; do
  if grep -Fq "[\"$title\"" src/pages/SettingsPage.tsx; then
    fail "$title 仍存在于 Settings 导航"
  fi
  echo "✅ $title 已退出 Settings 导航"
done

for page in Chat "My AI" Models MCP Artifacts "AI Council" "AI Arena" Agents Settings; do
  if ! printf '%s\n' "$PAGE_BLOCK" | grep -Fq "\"$page\""; then
    fail "$page 被误删"
  fi
done
echo "✅ 新 UI PageName 全部保留"

grep -Fq '["Local models", "Ollama models on this Mac", "Models"]' \
  src/pages/SettingsPage.tsx || fail "Local models 被提前删除"

echo "✅ Models 暂时保留供 My AI 管理本地模型"

npm run build
git diff --check

echo "✅ Frontend build"
echo "✅ Git diff check"
echo "PASS: legacy ui step2"
