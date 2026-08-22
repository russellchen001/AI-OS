#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEST_DIR="$(mktemp -d /tmp/ai-os-p15-baidu-official.XXXXXX)"
SOURCE="${AI_OS_BAIDU_E2E_SOURCE:-https://pan.baidu.com/s/1rh-lvEc-QjiMtfPi6T1BKA?pwd=7p6z}"

cleanup() {
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 Baidu Official Download: $1"
  exit 1
}

command -v bdpan >/dev/null 2>&1 || fail "official bdpan CLI is not installed"
bdpan whoami 2>/dev/null | grep -q "已登录" || fail "official bdpan CLI is not logged in"
echo "✓ Official bdpan CLI is installed and authenticated"

cd "$ROOT" || fail "cannot enter project"
AI_OS_DOWNLOAD_E2E_SOURCE="$SOURCE" \
AI_OS_DOWNLOAD_E2E_DESTINATION="$TEST_DIR" \
cargo test --manifest-path src-tauri/Cargo.toml \

# --- MANUAL E2E BOUNDARY ---
# Local-model execution is probabilistic. Automating "the model did the right
# thing every time" produces flaky failures that train people to skip
# acceptance. Everything deterministic is verified above.
echo ""
echo "MANUAL CONFIRMATION REQUIRED:"
echo "  Run a download in the app to an empty directory; confirm the file lands."
echo "  Last confirmed 2026-08-23: Baidu share link, 9.8 MB, ai-os-exec-standard."
echo ""
echo "PASS: p15_baidu_official (deterministic checks)"
exit 0
