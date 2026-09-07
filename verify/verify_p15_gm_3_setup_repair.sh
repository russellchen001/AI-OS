#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
UNIT_LOG="/tmp/p15-gm3-unit.log"
LIVE_LOG="/tmp/p15-gm3-live.log"
GM2B_LOG="/tmp/p15-gm3-gm2b.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL GM-3 One-click Setup / Repair"
  exit 1
}

echo "GM-3 One-click Setup / Repair"
echo "• first real run may download approximately 2.13 GB"
echo "• interrupted .part downloads resume on the next run"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_setup::tests:: \
  --lib \
  -- \
  --skip live_setup_or_repair_configures_real_profile \
  >"$UNIT_LOG" 2>&1 || {
    tail -120 "$UNIT_LOG"
    fail "deterministic Setup / Repair contract"
  }

echo "✓ explicit exact-file adoption"
echo "✓ unknown conflicting user model is never overwritten"
echo "✓ managed corruption is repairable"
echo "✓ manifest persistence is atomic"
echo "✓ interrupted HTTP download resumes with Range"
echo "✓ configured-but-not-yet-smoke-tested is not falsely Broken"

npm run build >/tmp/p15-gm3-frontend.log 2>&1 || {
  tail -100 /tmp/p15-gm3-frontend.log
  fail "frontend Setup / Repair build"
}

echo "✓ frontend one-click Setup / Repair build"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_setup::tests::live_setup_or_repair_configures_real_profile \
  --lib -- --ignored --exact --nocapture \
  >"$LIVE_LOG" 2>&1 || {
    tail -160 "$LIVE_LOG"
    fail "real managed checkpoint Setup / Repair"
  }

grep -q 'LIVE_GM3_SETUP action=' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "live Setup / Repair evidence missing"
}

grep -q 'workflow=true' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "workflow readiness after setup"
}

grep -q 'assets=true' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "asset readiness after setup"
}

grep -q 'custom_nodes=true' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "node readiness after setup"
}

grep -q 'integrity=true' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "integrity readiness after setup"
}

grep -q 'state=InstalledNotConfigured' "$LIVE_LOG" || {
  tail -100 "$LIVE_LOG"
  fail "pending GM-2C execution must remain InstalledNotConfigured"
}

grep 'LIVE_GM3_SETUP' "$LIVE_LOG" | tail -1

echo "✓ real managed checkpoint installed/adopted/repaired"
echo "✓ real SHA-256 integrity"
echo "✓ fresh ComfyUI advertises managed checkpoint"
echo "✓ profile is configured without fabricating smoke/output readiness"

bash verify/verify_p15_gm_2b_profile_readiness.sh \
  >"$GM2B_LOG" 2>&1 || {
    tail -160 "$GM2B_LOG"
    fail "GM-2B profile regression"
  }

grep -q 'assets=true' "$GM2B_LOG" || {
  tail -100 "$GM2B_LOG"
  fail "GM-2B does not observe installed asset"
}

grep -q 'integrity=true' "$GM2B_LOG" || {
  tail -100 "$GM2B_LOG"
  fail "GM-2B does not observe managed integrity"
}

grep 'LIVE_PROFILE_READINESS' "$GM2B_LOG" | tail -1

echo "✓ GM-2B now observes assets=true / integrity=true"

bash verify/rustfmt_changed.sh >/tmp/p15-gm3-rustfmt.log 2>&1 || {
  cat /tmp/p15-gm3-rustfmt.log
  fail "changed Rust formatting"
}

echo "✓ changed Rust formatting"

git diff --check || fail "git diff --check"

echo "✓ git diff --check"
echo "PASS GM-3 One-click Setup / Repair"
