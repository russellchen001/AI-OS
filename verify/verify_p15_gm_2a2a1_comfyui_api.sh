#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2a-1 ComfyUI API Contract"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui::tests::non_http_endpoint_is_rejected \
  -- --exact >/tmp/gm2a2a1-test.log 2>&1; then
  echo "✓ 非 HTTP endpoint 会被真实拒绝"
else
  echo "✗ ComfyUI endpoint contract 测试失败"
  tail -40 /tmp/gm2a2a1-test.log
  echo "FAIL GM-2A2a-1 contract"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::provider::tests \
  >/tmp/gm2a2a1-provider.log 2>&1; then
  echo "✓ GM-2A1 readiness 回归通过"
else
  echo "✗ GM-2A1 readiness 回归失败"
  tail -40 /tmp/gm2a2a1-provider.log
  echo "FAIL GM-2A2a-1 provider regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2a1-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2a1-diff.log
  echo "FAIL GM-2A2a-1 diff"
  exit 1
fi

echo "PASS GM-2A2a-1 ComfyUI API Contract"
exit 0
