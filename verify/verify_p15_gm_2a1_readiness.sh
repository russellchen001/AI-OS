#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A1 Ready Detection"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::provider::tests::readiness_evidence_classifies_all_stable_states \
  -- --exact >/tmp/gm2a1-state.log 2>&1; then
  echo "✓ Ready First 四态分类行为正确"
else
  echo "✗ Ready First 四态分类行为失败"
  tail -30 /tmp/gm2a1-state.log
  echo "FAIL GM-2A1 Ready Detection"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::provider::tests \
  >/tmp/gm2a1-provider.log 2>&1; then
  echo "✓ Generative Media Provider 回归测试通过"
else
  echo "✗ Generative Media Provider 回归测试失败"
  tail -30 /tmp/gm2a1-provider.log
  echo "FAIL GM-2A1 Provider Regression"
  exit 1
fi

if git diff --check >/tmp/gm2a1-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a1-diff.log
  echo "FAIL GM-2A1 Diff Check"
  exit 1
fi

echo "PASS GM-2A1 Ready Detection"
exit 0
