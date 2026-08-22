#!/usr/bin/env bash
FAILED=0
ok()  { echo "  ✅ $1"; }
bad() { echo "  ❌ $1"; FAILED=1; }

if openclaw agent --agent ai-os-exec-standard -m "只回答模型名" 2>/dev/null | grep -q "qwen3:8b"; then
  ok "ai-os-exec-standard 使用 qwen3:8b"
else
  bad "ai-os-exec-standard 未使用 qwen3:8b 或不可达"
fi

VISIBLE=$(openclaw skills check --agent ai-os-exec-standard 2>/dev/null | grep "Visible to model" | grep -o '[0-9]*')
if [ "$VISIBLE" = "1" ]; then
  ok "执行 agent 仅可见 1 个 Skill"
else
  bad "执行 agent 可见 ${VISIBLE:-未知} 个 Skill（应为 1）"
fi

if grep -q 'const FILESYSTEM_AGENT_ID: &str = "ai-os-files"' src-tauri/src/runtime/openclaw_gateway_adapter.rs; then
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
