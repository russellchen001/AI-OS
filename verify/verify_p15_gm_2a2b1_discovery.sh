#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2b-1 macOS ComfyUI Discovery"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::tests::installed_broken_instance_remains_discoverable \
  -- --exact >/tmp/gm2a2b1-discovery.log 2>&1; then
  echo "✓ 已安装但损坏的实例不会被误判为未安装"
else
  echo "✗ Desktop registry discovery 行为测试失败"
  tail -50 /tmp/gm2a2b1-discovery.log
  echo "FAIL GM-2A2b-1 discovery"
  exit 1
fi

if ./verify/verify_p15_gm_2a2a2_comfyui_http.sh \
  >/tmp/gm2a2b1-api-regression.log 2>&1; then
  echo "✓ GM-2A2a API contract 回归通过"
else
  echo "✗ GM-2A2a API contract 回归失败"
  tail -50 /tmp/gm2a2b1-api-regression.log
  echo "FAIL GM-2A2b-1 API regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2b1-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2b1-diff.log
  echo "FAIL GM-2A2b-1 diff"
  exit 1
fi

echo "PASS GM-2A2b-1 macOS ComfyUI Discovery"
exit 0
