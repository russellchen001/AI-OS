#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW SECURE SETUP LAYER"
echo "============================================================"

echo
echo "===== SECRET ARGUMENT SAFETY ====="

(
  cd src-tauri &&
  cargo test managed_secret_plan_never_places_token_in_argv --lib
)
TEST_RC=$?

if [ "$TEST_RC" -ne 0 ]; then
  echo "FAIL — secret argument safety"
  exit 1
fi

echo "PASS — secret argument safety"

echo
echo "===== KEYCHAIN BRIDGE REGRESSION ====="

(
  cd src-tauri &&
  cargo test ordinary_gateway_tokens_pass_through_unchanged --lib
)
KEYCHAIN_RC=$?

if [ "$KEYCHAIN_RC" -ne 0 ]; then
  echo "FAIL — Keychain bridge regression"
  exit 1
fi

echo "PASS — Keychain bridge regression"

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
echo "PASS — OPENCLAW SECURE SETUP LAYER"
echo "============================================================"
