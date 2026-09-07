#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
UNIT_LOG="/tmp/p15-gm2c-unit.log"
LIVE_LOG="/tmp/p15-gm2c-live.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL GM-2C ComfyUI Execution"
  exit 1
}

echo "GM-2C ComfyUI Execution"
echo "• real cancellation + one real 256x256 smoke generation"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_execution::tests:: \
  --lib \
  -- \
  --skip live_managed_profile_execution_reaches_ready \
  >"$UNIT_LOG" 2>&1 || {
    tail -160 "$UNIT_LOG"
    fail "deterministic execution contract"
  }

echo "✓ provider-owned core workflow execution graph"
echo "✓ malformed/error history fails closed"
echo "✓ completed history without image fails closed"
echo "✓ POST /prompt -> /history -> /view HTTP behavior"
echo "✓ running cancellation uses /interrupt"
echo "✓ validation evidence persists atomically"

cargo test \
  --manifest-path "$MANIFEST" \
  generative_media::comfyui_execution::tests::live_managed_profile_execution_reaches_ready \
  --lib -- --ignored --exact --nocapture \
  >"$LIVE_LOG" 2>&1 || {
    tail -220 "$LIVE_LOG"
    fail "real ComfyUI execution"
  }

grep -q 'LIVE_GM2C_EXECUTION' "$LIVE_LOG" || {
  tail -120 "$LIVE_LOG"
  fail "real execution evidence missing"
}

grep -q 'cancelled=true' "$LIVE_LOG" || {
  tail -120 "$LIVE_LOG"
  fail "real cancellation"
}

grep -q 'smoke=true' "$LIVE_LOG" || {
  tail -120 "$LIVE_LOG"
  fail "real smoke generation"
}

grep -q 'output=true' "$LIVE_LOG" || {
  tail -120 "$LIVE_LOG"
  fail "real output retrieval"
}

grep -q 'state=Ready' "$LIVE_LOG" || {
  tail -120 "$LIVE_LOG"
  fail "complete execution evidence does not reach Ready"
}

OUTPUT_BYTES="$(
  grep 'LIVE_GM2C_EXECUTION' "$LIVE_LOG" |
  tail -1 |
  tr ' ' '\n' |
  grep '^output_bytes=' |
  cut -d= -f2
)"

if ! [[ "$OUTPUT_BYTES" =~ ^[0-9]+$ ]] || [ "$OUTPUT_BYTES" -le 8 ]; then
  tail -120 "$LIVE_LOG"
  fail "retrieved image is empty"
fi

grep 'LIVE_GM2C_EXECUTION' "$LIVE_LOG" | tail -1

echo "✓ real prompt submission"
echo "✓ real queue/running observation"
echo "✓ real cancellation"
echo "✓ real completed history"
echo "✓ real SaveImage output metadata"
echo "✓ real /view image bytes"
echo "✓ real execution evidence reaches Ready"

VALIDATION="$HOME/Library/Application Support/AI-OS/generative-media/comfyui/profiles/comfyui-checkpoint-t2i-v1.execution.json"

[ -s "$VALIDATION" ] || fail "durable execution validation record"

grep -q '"cancellationOk": true' "$VALIDATION" ||
  fail "persisted cancellation evidence"

grep -q '"smokeGenerationOk": true' "$VALIDATION" ||
  fail "persisted smoke evidence"

grep -q '"outputRetrievalOk": true' "$VALIDATION" ||
  fail "persisted output evidence"

echo "✓ durable execution validation evidence"

bash verify/rustfmt_changed.sh >/tmp/p15-gm2c-rustfmt.log 2>&1 || {
  cat /tmp/p15-gm2c-rustfmt.log
  fail "changed Rust formatting"
}

echo "✓ changed Rust formatting"

git diff --check || fail "git diff --check"

echo "✓ git diff --check"
echo "PASS GM-2C ComfyUI Execution"
