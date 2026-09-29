#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW KEYCHAIN TOKEN BRIDGE"
echo "============================================================"

echo
echo "===== TOKEN RESOLUTION BEHAVIOR ====="

(
  cd src-tauri &&
  cargo test ordinary_gateway_tokens_pass_through_unchanged --lib
)
TEST_RC=$?

if [ "$TEST_RC" -ne 0 ]; then
  echo "FAIL — token resolution behavior"
  exit 1
fi

echo "PASS — token resolution behavior"

echo
echo "===== EXISTING OPENCLAW REGRESSION ====="

(
  cd src-tauri &&
  cargo test classifies_localhost_and_loopback_addresses_as_local --lib
)
REGRESSION_RC=$?

if [ "$REGRESSION_RC" -ne 0 ]; then
  echo "FAIL — existing OpenClaw regression"
  exit 1
fi

echo "PASS — existing OpenClaw regression"

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
echo "PASS — OPENCLAW KEYCHAIN TOKEN BRIDGE"
echo "============================================================"
