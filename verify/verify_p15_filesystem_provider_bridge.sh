#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"

fail() {
    echo "FAIL P15 Filesystem Provider Bridge: $1"
    exit 1
}


echo "---- Check filesystem bridge ----"

rg -n "OpenClawGatewayExecutionAdapter|OpenClawExecutionRequest|adapter.execute" \
src-tauri/src/filesystem/registry.rs \
>/dev/null \
|| fail "filesystem provider bridge missing"

echo "✓ Provider calls OpenClaw execution adapter"


echo
echo "---- Build Rust runtime ----"

cargo check \
--manifest-path src-tauri/Cargo.toml \
>/tmp/p15-filesystem-bridge.log 2>&1 \
|| {
    grep -E "error:" /tmp/p15-filesystem-bridge.log | head -20
    fail "cargo check"
}

echo "✓ Runtime builds"


echo
echo "PASS P15 Filesystem Provider Bridge"
