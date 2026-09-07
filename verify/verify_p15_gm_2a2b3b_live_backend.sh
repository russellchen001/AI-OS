#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2b-3b Live ComfyUI Backend E2E"
echo "• 首次启动可能需要约 1–2 分钟"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::live_backend_tests::live_backend_starts_reaches_api_and_stops \
  -- --exact --ignored --nocapture \
  >/tmp/gm2a2b3b-live.log 2>&1; then
  echo "✓ Rust discovery → spawn → API health → stop 全链通过"
else
  echo "✗ 真实 ComfyUI backend E2E 失败"
  tail -100 /tmp/gm2a2b3b-live.log
  echo "FAIL GM-2A2b-3b live backend"
  exit 1
fi

if ! grep 'LIVE_BACKEND instance=' /tmp/gm2a2b3b-live.log; then
  echo "✗ 测试没有返回真实 backend 身份信息"
  echo "FAIL GM-2A2b-3b backend identity"
  exit 1
fi

if ./verify/verify_p15_gm_2a2b3a_backend_lifecycle.sh \
  >/tmp/gm2a2b3b-regression.log 2>&1; then
  echo "✓ GM-2A2b-3a lifecycle 回归通过"
else
  echo "✗ GM-2A2b-3a lifecycle 回归失败"
  tail -60 /tmp/gm2a2b3b-regression.log
  echo "FAIL GM-2A2b-3b lifecycle regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2b3b-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2b3b-diff.log
  echo "FAIL GM-2A2b-3b diff"
  exit 1
fi

echo "PASS GM-2A2b-3b Live ComfyUI Backend E2E"
exit 0
