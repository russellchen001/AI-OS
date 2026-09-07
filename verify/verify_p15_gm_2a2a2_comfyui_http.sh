#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2a-2 ComfyUI HTTP Behavior"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui::http_behavior_tests::real_comfyui_shape_is_healthy \
  -- --exact >/tmp/gm2a2a2-valid.log 2>&1; then
  echo "✓ 真实 ComfyUI API 形状被识别"
else
  echo "✗ ComfyUI API 正例失败"
  tail -40 /tmp/gm2a2a2-valid.log
  echo "FAIL GM-2A2a-2 valid API"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui::http_behavior_tests::ordinary_json_service_is_not_comfyui \
  -- --exact >/tmp/gm2a2a2-fake.log 2>&1; then
  echo "✓ 普通 JSON HTTP 服务不会被误认成 ComfyUI"
else
  echo "✗ 非 ComfyUI 服务识别失败"
  tail -40 /tmp/gm2a2a2-fake.log
  echo "FAIL GM-2A2a-2 false positive"
  exit 1
fi

if curl -fsS \
  --connect-timeout 1 \
  --max-time 3 \
  http://127.0.0.1:8188/system_stats \
  >/dev/null 2>&1; then

  if AI_OS_COMFYUI_TEST_ENDPOINT="http://127.0.0.1:8188" \
    cargo test \
      --manifest-path src-tauri/Cargo.toml \
      generative_media::comfyui::http_behavior_tests::live_comfyui_endpoint_is_healthy_when_requested \
      -- --exact --ignored >/tmp/gm2a2a2-live.log 2>&1; then
    echo "✓ Rust probe 通过当前真实 127.0.0.1:8188"
  else
    echo "✗ Rust probe 无法识别当前真实 ComfyUI"
    tail -40 /tmp/gm2a2a2-live.log
    echo "FAIL GM-2A2a-2 live API"
    exit 1
  fi
else
  echo "○ SKIP live endpoint：ComfyUI 当前未运行"
  echo "  macOS discovery/start 属于 GM-2A2b，不由 API verifier 启动"
fi

if ./verify/verify_p15_gm_2a1_readiness.sh \
  >/tmp/gm2a2a2-a1.log 2>&1; then
  echo "✓ GM-2A1 readiness 回归通过"
else
  echo "✗ GM-2A1 readiness 回归失败"
  tail -40 /tmp/gm2a2a2-a1.log
  echo "FAIL GM-2A2a-2 readiness regression"
  exit 1
fi

if ./verify/verify_p15_gm_2a2a1_comfyui_api.sh \
  >/tmp/gm2a2a2-a1api.log 2>&1; then
  echo "✓ GM-2A2a-1 API contract 回归通过"
else
  echo "✗ GM-2A2a-1 API contract 回归失败"
  tail -40 /tmp/gm2a2a2-a1api.log
  echo "FAIL GM-2A2a-2 API contract regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2a2-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2a2-diff.log
  echo "FAIL GM-2A2a-2 diff"
  exit 1
fi

echo "PASS GM-2A2a-2 ComfyUI HTTP Behavior"
exit 0
