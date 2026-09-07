#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2b-3a ComfyUI Backend Lifecycle"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::backend_lifecycle_tests::missing_runtime_is_rejected_before_process_spawn \
  -- --exact >/tmp/gm2a2b3a-lifecycle.log 2>&1; then
  echo "✓ 缺失 runtime 会在 spawn 前被拒绝"
else
  echo "✗ backend lifecycle contract 测试失败"
  tail -60 /tmp/gm2a2b3a-lifecycle.log
  echo "FAIL GM-2A2b-3a lifecycle"
  exit 1
fi

if ./verify/verify_p15_gm_2a2b2_live_discovery.sh \
  >/tmp/gm2a2b3a-discovery.log 2>&1; then
  echo "✓ GM-2A2b-2 real discovery 回归通过"
else
  echo "✗ GM-2A2b-2 discovery 回归失败"
  tail -60 /tmp/gm2a2b3a-discovery.log
  echo "FAIL GM-2A2b-3a discovery regression"
  exit 1
fi

if ./verify/verify_p15_gm_2a2a2_comfyui_http.sh \
  >/tmp/gm2a2b3a-api.log 2>&1; then
  echo "✓ GM-2A2a Local API contract 回归通过"
else
  echo "✗ GM-2A2a Local API 回归失败"
  tail -60 /tmp/gm2a2b3a-api.log
  echo "FAIL GM-2A2b-3a API regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2b3a-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2b3a-diff.log
  echo "FAIL GM-2A2b-3a diff"
  exit 1
fi

echo "PASS GM-2A2b-3a ComfyUI Backend Lifecycle"
exit 0
