#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2c-1 Readiness Bridge"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::readiness_bridge_tests::runtime_facts_map_to_ready_first_states_without_fake_ready \
  -- --exact >/tmp/gm2a2c1-bridge.log 2>&1; then
  echo "✓ runtime facts 正确映射 Ready First 状态"
else
  echo "✗ readiness bridge 行为测试失败"
  tail -60 /tmp/gm2a2c1-bridge.log
  echo "FAIL GM-2A2c-1 bridge"
  exit 1
fi

if ./verify/verify_p15_gm_2a2b3a_backend_lifecycle.sh \
  >/tmp/gm2a2c1-lifecycle.log 2>&1; then
  echo "✓ GM-2A2b backend lifecycle 回归通过"
else
  echo "✗ GM-2A2b backend lifecycle 回归失败"
  tail -60 /tmp/gm2a2c1-lifecycle.log
  echo "FAIL GM-2A2c-1 lifecycle regression"
  exit 1
fi

if ./verify/verify_p15_gm_2a1_readiness.sh \
  >/tmp/gm2a2c1-provider.log 2>&1; then
  echo "✓ GM-2A1 Ready First 状态机回归通过"
else
  echo "✗ GM-2A1 readiness 回归失败"
  tail -60 /tmp/gm2a2c1-provider.log
  echo "FAIL GM-2A2c-1 readiness regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2c1-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2c1-diff.log
  echo "FAIL GM-2A2c-1 diff"
  exit 1
fi

echo "PASS GM-2A2c-1 Readiness Bridge"
exit 0
