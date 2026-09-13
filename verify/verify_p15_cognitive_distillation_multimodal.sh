#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="${TMPDIR:-/tmp}/ai-os-cognitive-distillation-multimodal.log"
PASS=0
FAIL=0

ok() {
  printf '✓ %s\n' "$1"
  PASS=$((PASS + 1))
}

bad() {
  printf '✗ %s\n' "$1"
  tail -60 "$LOG" 2>/dev/null || true
  FAIL=$((FAIL + 1))
}

run_test_group() {
  local label="$1"
  local filter="$2"
  local witness="$3"
  if cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib "$filter" >"$LOG" 2>&1 \
    && grep -F "$witness" "$LOG" >/dev/null; then
    ok "$label"
  else
    bad "$label"
  fi
}

printf '%s\n' 'P15 Cognitive Distillation multimodal foundation'

run_test_group \
  'text, image, audio, video and document evidence retain modality provenance' \
  'cognitive_distillation::evidence::tests' \
  'test cognitive_distillation::evidence::tests::text_image_audio_and_video_ingestion_preserve_modality_context ... ok'

run_test_group \
  'automatic routing, private/public isolation and optional-adapter fallback' \
  'cognitive_distillation::router::tests' \
  'test cognitive_distillation::router::tests::unavailable_specialized_adapter_falls_back_to_valid_primary ... ok'

run_test_group \
  'AI-OS-only activation and raw-media-free persona packaging' \
  'cognitive_distillation::profile::tests' \
  'test cognitive_distillation::profile::tests::runnable_skill_never_bundles_raw_multimodal_files ... ok'

run_test_group \
  'capability-based creator probes and license gates' \
  'cognitive_distillation::adapters::tests' \
  'test cognitive_distillation::adapters::tests::distilly_compatibility_uses_capabilities_not_version ... ok'

run_test_group \
  'single product capability is exposed without implementation names' \
  'runtime::skills::registry::tests::cognitive_distillation_exposes_one_platform_routed_capability' \
  'test runtime::skills::registry::tests::cognitive_distillation_exposes_one_platform_routed_capability ... ok'

if command -v ffmpeg >/dev/null 2>&1 && ffmpeg -version >"$LOG" 2>&1; then
  ok 'real local FFmpeg demux capability probe'
else
  bad 'real local FFmpeg demux capability probe'
fi

if command -v whisper-cli >/dev/null 2>&1 || command -v mlx_whisper >/dev/null 2>&1; then
  ok 'local timestamped ASR adapter is available'
else
  printf '%s\n' '○ local ASR adapter unavailable; no runtime/model was installed automatically'
fi

printf 'PASS=%s FAIL=%s\n' "$PASS" "$FAIL"
if [[ "$FAIL" -ne 0 ]]; then
  printf '%s\n' 'FAIL P15 Cognitive Distillation multimodal foundation'
  exit 1
fi

printf '%s\n' 'PASS P15 Cognitive Distillation multimodal foundation'
exit 0
