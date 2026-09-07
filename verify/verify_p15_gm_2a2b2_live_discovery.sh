#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "GM-2A2b-2 Live macOS ComfyUI Discovery"

if cargo test \
  --manifest-path src-tauri/Cargo.toml \
  generative_media::comfyui_macos::live_discovery_tests::live_desktop_registry_discovers_usable_local_instance \
  -- --exact --ignored --nocapture \
  >/tmp/gm2a2b2-live.log 2>&1; then

  echo "✓ Rust adapter 真实读取 Comfy Desktop registry"

  grep '^instance=' /tmp/gm2a2b2-live.log || true
else
  echo "✗ 真实 Desktop registry discovery 失败"
  tail -60 /tmp/gm2a2b2-live.log
  echo "FAIL GM-2A2b-2 live discovery"
  exit 1
fi

if ./verify/verify_p15_gm_2a2b1_discovery.sh \
  >/tmp/gm2a2b2-regression.log 2>&1; then
  echo "✓ GM-2A2b-1 discovery contract 回归通过"
else
  echo "✗ GM-2A2b-1 discovery contract 回归失败"
  tail -60 /tmp/gm2a2b2-regression.log
  echo "FAIL GM-2A2b-2 discovery regression"
  exit 1
fi

if git diff --check >/tmp/gm2a2b2-diff.log 2>&1; then
  echo "✓ git diff --check 通过"
else
  echo "✗ git diff --check 失败"
  cat /tmp/gm2a2b2-diff.log
  echo "FAIL GM-2A2b-2 diff"
  exit 1
fi

echo "PASS GM-2A2b-2 Live macOS ComfyUI Discovery"
exit 0
