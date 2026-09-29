#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW COMPLETE SETUP"
echo "============================================================"

echo
echo "===== COMPLETE POLICY ====="

(
  cd src-tauri &&
  cargo test complete_policy_is_bounded --lib
)

POLICY_RC=$?

if [ "$POLICY_RC" -ne 0 ]; then
  echo "FAIL — Complete Setup policy"
  exit 1
fi

echo "PASS — Complete Setup policy"

echo
echo "===== SECURE SETUP REGRESSION ====="

(
  cd src-tauri &&
  cargo test managed_secret_plan_never_places_token_in_argv --lib
)

SECRET_RC=$?

if [ "$SECRET_RC" -ne 0 ]; then
  echo "FAIL — secure setup regression"
  exit 1
fi

echo "PASS — secure setup regression"

echo
echo "===== START ACTION REGRESSION ====="

(
  cd src-tauri &&
  cargo test start_policy_is_bounded --lib
)

START_RC=$?

if [ "$START_RC" -ne 0 ]; then
  echo "FAIL — start action regression"
  exit 1
fi

echo "PASS — start action regression"

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
echo "PASS — OPENCLAW COMPLETE SETUP"
echo "============================================================"
