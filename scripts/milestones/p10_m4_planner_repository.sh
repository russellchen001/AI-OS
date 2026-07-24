#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR/src-tauri"

echo
echo "== P10-M4: cargo fmt check =="
cargo fmt --all -- --check

echo
echo "== P10-M4: cargo check =="
cargo check

echo
echo "== P10-M4: planner tests =="
cargo test planner::

echo
echo "== P10-M4: full Rust tests =="
cargo test

echo
echo "== P10-M4: documentation tests =="
cargo test --doc
