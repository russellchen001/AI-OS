#!/usr/bin/env bash
FAILED=0
ok()  { echo "  ✅ $1"; }
bad() { echo "  ❌ $1"; FAILED=1; }

EXPECTED_MODEL="ollama/qwen3:8b"
if [ "$(uname -m)" = "arm64" ]; then
  EXPECTED_MODEL="omlx/Qwen3.5-9B-4bit"
fi
RESOLVED_MODEL=$(openclaw models status --agent ai-os-exec-standard --json 2>/dev/null | jq -r '.resolvedDefault // empty')
if [ "$RESOLVED_MODEL" = "$EXPECTED_MODEL" ]; then
  ok "ai-os-exec-standard 使用 $EXPECTED_MODEL"
else
  bad "ai-os-exec-standard 实际使用 ${RESOLVED_MODEL:-未知}，应为 $EXPECTED_MODEL"
fi

VISIBLE=$(openclaw skills check --agent ai-os-exec-standard 2>/dev/null | grep "Visible to model" | grep -o '[0-9]*')
if [ "$VISIBLE" = "1" ]; then
  ok "执行 agent 仅可见 1 个 Skill"
else
  bad "执行 agent 可见 ${VISIBLE:-未知} 个 Skill（应为 1）"
fi

FILESYSTEM_AGENT=$(jq -r '.agents.list[] | select(.id == "ai-os-files") | .id' "$HOME/.openclaw/openclaw.json")
if [ "$FILESYSTEM_AGENT" = "ai-os-files" ]; then
  ok "文件操作仍使用 ai-os-files"
else
  bad "FILESYSTEM_AGENT_ID 被改动"
fi

if (cd src-tauri && cargo test 2>&1 | grep -q "^test result: ok"); then
  ok "Rust 测试通过"
else
  bad "Rust 测试失败"
fi

if [ $FAILED -eq 0 ]; then echo "PASS: p15 download exec agent"; exit 0
else echo "FAIL: p15 download exec agent"; exit 1; fi
