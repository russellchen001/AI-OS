#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_keychain_prompt_regression"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
run() {
  local label="$1"
  shift
  if "$@" > "$LOG" 2>&1; then
    grep 'test result:' "$LOG" || true
    echo "✅ $label"
  else
    grep -E '^error|FAILED|FAIL:|panicked|test result:' "$LOG" | head -20 || true
    echo "❌ $label"
    echo "FAIL: $FEATURE — $label"
    exit 1
  fi
}
run "整库共享、重启冷读、拒绝/失败不缓存、跨版本失效" cargo test --manifest-path src-tauri/Cargo.toml --lib credential_vault --quiet
run "独立 Keychain：240 次跨路径读取只发生一次系统读取；未变内容零写入；旧值保护" cargo test --manifest-path src-tauri/Cargo.toml --test beta7_vault_keychain_live --quiet
run "跨进程锁与更新版本号" cargo test --manifest-path src-tauri/Cargo.toml --test beta7_vault_lock_contention --quiet
run "第三步不被可选诊断挂起" node verify/beta7_diagnostics.behavior.mjs
run "完整向导、挂起和迟到结果" node verify/beta7_first_launch.runner.mjs
run "Git 差异检查" git diff --check
echo "PASS: $FEATURE — isolated behavior and native test-item access counts"
echo "MANUAL: 框架访问次数不是弹窗次数。全新用户实际连接、聊天、重启及稳定签名授权必须本人复验。"
