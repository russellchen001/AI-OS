#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW START ACTION"
echo "============================================================"

echo
echo "===== START POLICY ====="

(
  cd src-tauri &&
  cargo test start_policy_is_bounded --lib
)
POLICY_RC=$?

if [ "$POLICY_RC" -ne 0 ]; then
  echo "FAIL — start policy"
  exit 1
fi

echo "PASS — start policy"

echo
echo "===== LIVE STATUS REGRESSION ====="

(
  cd src-tauri &&
  cargo test managed_live_state_reflects_live_gateway --lib
)
LIVE_RC=$?

if [ "$LIVE_RC" -ne 0 ]; then
  echo "FAIL — live status regression"
  exit 1
fi

echo "PASS — live status regression"

echo
echo "===== RUST CHECK ====="

(
  cd src-tauri &&
  cargo check
)
RUST_RC=$?

if [ "$RUST_RC" -ne 0 ]; then
  echo "FAIL — Rust check"
  exit 1
fi

echo "PASS — Rust check"

echo
echo "===== FRONTEND BUILD ====="

npm run build
BUILD_RC=$?

if [ "$BUILD_RC" -ne 0 ]; then
  echo "FAIL — frontend build"
  exit 1
fi

echo "PASS — frontend build"

echo
echo "===== DIFF CHECK ====="

git diff --check
DIFF_RC=$?

if [ "$DIFF_RC" -ne 0 ]; then
  echo "FAIL — git diff check"
  exit 1
fi

echo "PASS — git diff check"

echo
echo "============================================================"
echo "PASS — OPENCLAW START ACTION"
echo "============================================================"
