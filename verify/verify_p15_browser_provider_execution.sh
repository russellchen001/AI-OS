#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"


fail() {
    echo "FAIL P15 Browser Provider Execution: $1"
    exit 1
}


echo "---- Check browser dispatcher ----"

test -f src-tauri/src/browser/runtime.rs \
 || fail "browser runtime missing"

echo "✓ Browser dispatcher exists"


echo
echo "---- Check browser routing ----"

rg -n "execute_browser_capability|browser\\." \
src-tauri/src/runtime/plan_runtime_bridge.rs \
>/dev/null \
|| fail "browser routing missing"

echo "✓ Browser capability routed"


echo
echo "---- Build Rust runtime ----"

cargo check --manifest-path src-tauri/Cargo.toml \
 >/tmp/p15-browser-provider-execution.log 2>&1 \
 || {
   grep -E "error:" /tmp/p15-browser-provider-execution.log | head -20
   fail "cargo check"
 }


echo "✓ Runtime builds"


echo
echo "PASS P15 Browser Provider Execution"
