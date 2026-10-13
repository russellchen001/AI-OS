#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: oMLX-only local stack - $1"
  exit 1
}

run_rust_test() {
  local test_name="$1"
  local label="$2"
  if cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib "$test_name" --quiet >"$TMP/rust.log" 2>&1; then
    echo "✅ $label"
  else
    grep -E "FAILED|^error|failures:" "$TMP/rust.log" | head -20
    fail "$label"
  fi
}

run_rust_test "runtime::tests::registry_has_two_stable_ids" "Runtime registry exposes only supported desktop services"
run_rust_test "providers::tests::provider_adapter_registry_has_stable_order_and_ids" "Provider registry exposes the supported Provider set"
run_rust_test "providers::tests::auto_route_candidates_prefer_omlx_then_cloud" "Auto routing prefers oMLX before cloud"
run_rust_test "providers::tests::parses_omlx_openai_compatible_stream_chunk" "oMLX streaming protocol is operational"

if npm --prefix "$ROOT" run build >"$TMP/frontend.log" 2>&1; then
  echo "✅ Frontend production build"
else
  grep -E "error TS|ERROR|failed" "$TMP/frontend.log" | head -20
  fail "Frontend production build"
fi

echo "PASS: oMLX-only local stack"
exit 0
