#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW EXISTING INSTALLATION STATE"
echo "============================================================"

echo
echo "===== STATE BEHAVIOR ====="

(
  cd src-tauri &&
  cargo test \
    managed_unconfigured_state_distinguishes_existing_config \
    --lib
)

TEST_RC=$?

if [ "$TEST_RC" -ne 0 ]; then
  echo "FAIL — existing-installation state behavior"
  exit 1
fi

echo "PASS — existing-installation state behavior"

echo
echo "===== LIVE STATUS REGRESSION ====="

(
  cd src-tauri &&
  cargo test \
    managed_live_state_reflects_live_gateway \
    --lib
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
echo "===== SOURCE INTEGRITY ====="

git diff --check
DIFF_RC=$?

if [ "$DIFF_RC" -ne 0 ]; then
  echo "FAIL — git diff check"
  exit 1
fi

echo "PASS — git diff check"

echo
echo "============================================================"
echo "PASS — OPENCLAW EXISTING INSTALLATION STATE"
echo "============================================================"
