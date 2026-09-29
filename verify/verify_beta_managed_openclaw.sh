#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — MANAGED OPENCLAW BACKEND"
echo "============================================================"

echo
echo "===== STATE MACHINE BEHAVIOR ====="

(
  cd src-tauri &&
  cargo test managed_state_matrix_is_deterministic --lib
)
TEST_RC=$?

if [ "$TEST_RC" -ne 0 ]; then
  echo "FAIL — managed OpenClaw state behavior"
  exit 1
fi

echo "PASS — state behavior"

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
echo "PASS — MANAGED OPENCLAW BACKEND"
echo "============================================================"
