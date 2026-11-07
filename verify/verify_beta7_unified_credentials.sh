#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_unified_credentials"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

run() {
  local label="$1"
  shift
  if "$@" > "$LOG" 2>&1; then
    echo "✅ $label"
  else
    grep -E '^error|FAILED|FAIL:|panicked|test result:' "$LOG" | head -20 || true
    echo "❌ $label"
    echo "FAIL: $FEATURE — $label"
    exit 1
  fi
}

run "写入失败保护" bash verify/verify_beta7_vault_failure_preservation.sh
run "旧凭据非破坏性迁移" bash verify/verify_beta7_vault_migration.sh
run "Provider 缓存与账号隔离回归" bash verify/verify_macos_keychain_credentials.sh
run "OpenClaw Token 引用回归" cargo test --manifest-path src-tauri/Cargo.toml ordinary_gateway_tokens_pass_through_unchanged --quiet
run "Thunder 凭据模式回归" cargo test --manifest-path src-tauri/Cargo.toml auth_mode_serialization_is_stable --quiet
run "Rust 集成编译" cargo check --manifest-path src-tauri/Cargo.toml

if git diff --check; then
  echo "✅ Git 差异检查"
else
  echo "❌ Git 差异检查"
  echo "FAIL: $FEATURE — diff"
  exit 1
fi

echo "PASS: $FEATURE — isolated and regression coverage"
echo "MANUAL: 真实首次授权与系统拒绝写入场景仍需在签名测试 App 中确认。"
