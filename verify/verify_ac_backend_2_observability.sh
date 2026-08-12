#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "FAIL: AC-BACKEND-2 Observability - $1"
  exit 1
}

trap 'fail "command failed at line $LINENO"' ERR

cargo test --quiet --manifest-path src-tauri/Cargo.toml canonical_
echo "✅ Rust canonical metadata behavior"

cargo test --quiet --manifest-path src-tauri/Cargo.toml route_selection_allows_only_auto_pre_output_fallback
echo "✅ Streaming/cancellation fallback boundary"

npm run build >/dev/null
echo "✅ Frontend canonical metadata and pricing contract"

git diff --check
echo "✅ Git diff check"

echo "PASS: AC-BACKEND-2 Observability"
