#!/bin/bash

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEST_OUTPUT="$(mktemp -t ai-os-mano-cloud-test.XXXXXX)"
E2E_DIR="$(mktemp -d /tmp/ai-os-mano-cloud-e2e.XXXXXX)"
OUTPUT_PATH="$E2E_DIR/AI_OS_MANO_CLOUD_E2E.txt"
MARKER='AI_OS_MANO_CLOUD_E2E_20260913'

cleanup() {
  rm -f "$TEST_OUTPUT" "$OUTPUT_PATH"
  rmdir "$E2E_DIR" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  printf 'FAIL: P15 Mano Cloud E2E — %s\n' "$1"
  exit 1
}

[ "${AI_OS_MANO_MODE:-}" = "cloud" ] \
  || fail 'AI_OS_MANO_MODE is not cloud'
[ "${AI_OS_MANO_CLOUD_AUTHORIZED:-}" = "true" ] \
  || fail 'Mano Cloud is not explicitly authorized'
[ "$(mano-cua config --get disable-bash 2>/dev/null)" = "true" ] \
  || fail 'Mano Cloud bash capability is not disabled'
[ "$(mano-cua config --get save-trajectory 2>/dev/null)" = "false" ] \
  || fail 'trajectory saving is not disabled'

touch "$OUTPUT_PATH" || fail 'could not create the dedicated empty local fixture'
[ ! -s "$OUTPUT_PATH" ] || fail 'the dedicated local fixture was not empty'
open -a TextEdit "$OUTPUT_PATH" || fail 'could not open the local fixture in TextEdit'
sleep 1

if ! (cd "$ROOT_DIR/src-tauri" && \
  AI_OS_MANO_CLOUD_E2E_OUTPUT_PATH="$OUTPUT_PATH" \
  cargo test --lib \
    runtime::plan_runtime_bridge::tests::mp5_real_openclaw_to_mano_cloud_textedit_completes_and_is_verified \
    -- --ignored --exact --nocapture) >"$TEST_OUTPUT" 2>&1; then
  tail -60 "$TEST_OUTPUT"
  fail 'real Runtime to Mano Cloud execution failed'
fi

grep -Eq 'test .*::mp5_real_openclaw_to_mano_cloud_textedit_completes_and_is_verified \.\.\. ok' "$TEST_OUTPUT" \
  || fail 'the exact real Cloud test did not pass'
grep -Eq 'test result: ok\. 1 passed; 0 failed;' "$TEST_OUTPUT" \
  || fail 'the real Cloud test count was not exactly one'
[ -f "$OUTPUT_PATH" ] || fail 'TextEdit did not create the output file'
[ "$(cat "$OUTPUT_PATH")" = "$MARKER" ] \
  || fail 'TextEdit output content did not match the fixed marker'

for log_file in \
  "$HOME/.ai-os/logs/ai-os.log" \
  "$HOME/.ai-os/logs/openclaw.log" \
  "$ROOT_DIR/browser-diagnostics.log" \
  "$ROOT_DIR/browser-launch-stderr.log"; do
  if [ -f "$log_file" ] && grep -Fq "$MARKER" "$log_file"; then
    fail "task marker leaked into $log_file"
  fi
done

printf '✓ selected OpenClaw returned NoViableExecutionPath\n'
printf '✓ Runtime invoked the production Mano Cloud fallback\n'
printf '✓ TextEdit created the dedicated file with exact content\n'
printf '✓ AI-OS returned normalized completed Cloud output\n'
printf '✓ task marker is absent from known AI-OS logs\n'
printf 'PASS: P15 Mano Cloud Real E2E\n'
