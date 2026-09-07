#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A Local ComfyUI Ready Detection"
echo "• 最终验收会真实冷启动一次本地 ComfyUI，最长等待 150 秒"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::provider::tests::readiness_evidence_classifies_all_stable_states \
  -- --exact \
  >/tmp/gm2a-final-provider.log 2>&1; then
  echo "✓ 四态 Ready First 分类契约通过"
else
  echo "✗ Ready First 分类契约失败"
  tail -80 /tmp/gm2a-final-provider.log
  echo "FAIL GM-2A provider readiness"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui::http_behavior_tests::real_comfyui_shape_is_healthy \
  -- --exact \
  >/tmp/gm2a-final-api-valid.log 2>&1; then
  echo "✓ ComfyUI API 正例识别通过"
else
  echo "✗ ComfyUI API 正例失败"
  tail -80 /tmp/gm2a-final-api-valid.log
  echo "FAIL GM-2A API positive"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui::http_behavior_tests::ordinary_json_service_is_not_comfyui \
  -- --exact \
  >/tmp/gm2a-final-api-negative.log 2>&1; then
  echo "✓ 普通 JSON 服务不会被误认成 ComfyUI"
else
  echo "✗ ComfyUI API false-positive 防护失败"
  tail -80 /tmp/gm2a-final-api-negative.log
  echo "FAIL GM-2A API negative"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::final_readiness_tests::incomplete_runtime_is_installed_broken_without_becoming_ready \
  -- --exact \
  >/tmp/gm2a-final-broken.log 2>&1; then
  echo "✓ 已安装但 runtime 损坏 → InstalledBroken"
else
  echo "✗ broken runtime 状态映射失败"
  tail -80 /tmp/gm2a-final-broken.log
  echo "FAIL GM-2A broken runtime"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::live_discovery_tests::live_desktop_registry_discovers_usable_local_instance \
  -- --exact --ignored --nocapture \
  >/tmp/gm2a-final-discovery.log 2>&1; then
  echo "✓ 真实 Comfy Desktop standalone installation discovery 通过"
else
  echo "✗ 真实 Comfy Desktop discovery 失败"
  tail -100 /tmp/gm2a-final-discovery.log
  echo "FAIL GM-2A live discovery"
  exit 1
fi

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::final_readiness_tests::live_desktop_runtime_is_healthy_but_not_ready_before_profile_checks \
  -- --exact --ignored --nocapture \
  >/tmp/gm2a-final-live.log 2>&1; then
  echo "✓ 真实 backend readiness E2E 通过"
else
  echo "✗ 真实 backend readiness E2E 失败"
  tail -120 /tmp/gm2a-final-live.log
  echo "FAIL GM-2A live readiness"
  exit 1
fi

if grep 'LIVE_READINESS instance=' /tmp/gm2a-final-live.log; then
  :
else
  echo "✗ 没有取得真实 readiness identity"
  echo "FAIL GM-2A live identity"
  exit 1
fi

if grep -q 'state=InstalledNotConfigured' /tmp/gm2a-final-live.log \
  && grep -q 'installed=true startable=true api=true ready=false' \
       /tmp/gm2a-final-live.log; then
  echo "✓ API healthy 仍不会被提前标记 Ready"
else
  echo "✗ Ready First 最终状态不符合契约"
  tail -80 /tmp/gm2a-final-live.log
  echo "FAIL GM-2A premature ready"
  exit 1
fi

if git diff --check >/tmp/gm2a-final-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a-final-diff.log
  echo "FAIL GM-2A diff"
  exit 1
fi

echo
echo "PASS GM-2A Local ComfyUI Ready Detection"
exit 0
