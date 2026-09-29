#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

echo "============================================================"
echo "BETA — OPENCLAW FIRST-RUN UI"
echo "============================================================"

echo
echo "===== BACKEND BEHAVIOR ====="

(
  cd src-tauri &&
  cargo test managed_state_matrix_is_deterministic --lib
)
BACKEND_RC=$?

if [ "$BACKEND_RC" -ne 0 ]; then
  echo "FAIL — managed backend behavior"
  exit 1
fi

echo "PASS — managed backend behavior"

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
echo "PASS — OPENCLAW FIRST-RUN UI"
echo "============================================================"
