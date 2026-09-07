#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LIVE_LOG="/tmp/p15-gm2b-live-profile.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL GM-2B Workflow / Model / Node Readiness"
  exit 1
}

echo "GM-2B Workflow / Model / Node Readiness"
echo "• one real ComfyUI cold-start may take up to 150 seconds"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_profile::tests:: \
  --lib >/tmp/p15-gm2b-profile-unit.log 2>&1 || {
    tail -100 /tmp/p15-gm2b-profile-unit.log
    fail "profile workflow/model/integrity contract"
  }

echo "✓ built-in core workflow contract"
echo "✓ missing managed profile remains not configured"
echo "✓ checkpoint must be advertised by ComfyUI"
echo "✓ checkpoint size + SHA-256 integrity"
echo "✓ unsafe managed asset paths fail closed"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_macos::profile_readiness_tests::desktop_model_root_parser_reads_generated_yaml \
  --lib -- --exact >/tmp/p15-gm2b-model-root.log 2>&1 || {
    tail -80 /tmp/p15-gm2b-model-root.log
    fail "Comfy Desktop model-root discovery"
  }

echo "✓ official Comfy Desktop model-root config parsed"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_macos::profile_readiness_tests::live_desktop_profile_readiness_reflects_real_inventory \
  --lib -- --ignored --exact --nocapture >"$LIVE_LOG" 2>&1 || {
    tail -120 "$LIVE_LOG"
    fail "real ComfyUI profile-aware readiness"
  }

grep -q 'LIVE_PROFILE_READINESS instance=' "$LIVE_LOG" || {
  tail -80 "$LIVE_LOG"
  fail "live profile evidence missing"
}

grep -q 'workflow=true' "$LIVE_LOG" || {
  tail -80 "$LIVE_LOG"
  fail "real core workflow is not compatible"
}

grep -q 'custom_nodes=true' "$LIVE_LOG" || {
  tail -80 "$LIVE_LOG"
  fail "core-only profile custom-node contract"
}

grep -q 'ready=false' "$LIVE_LOG" || {
  tail -80 "$LIVE_LOG"
  fail "GM-2B must not claim Ready before smoke/output"
}

grep 'LIVE_PROFILE_READINESS' "$LIVE_LOG" | tail -1

echo "✓ real ComfyUI core workflow compatibility"
echo "✓ real node availability"
echo "✓ real asset/integrity state reported without fake Ready"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media:: \
  --lib >/tmp/p15-gm2b-media-regression.log 2>&1 || {
    tail -120 /tmp/p15-gm2b-media-regression.log
    fail "Generative Media regression"
  }

echo "✓ Generative Media regression"

bash verify/rustfmt_changed.sh >/tmp/p15-gm2b-rustfmt.log 2>&1 || {
  cat /tmp/p15-gm2b-rustfmt.log
  fail "changed Rust formatting"
}

echo "✓ changed Rust formatting"

git diff --check || fail "git diff --check"

echo "✓ git diff --check"
echo "PASS GM-2B Workflow / Model / Node Readiness"
