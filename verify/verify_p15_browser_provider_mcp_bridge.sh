#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"


fail() {
    echo "FAIL P15 Browser Provider MCP Bridge: $1"
    exit 1
}


echo "---- Check provider MCP bridge ----"

rg -n "call_mcp_tool" \
src-tauri/src/browser/registry.rs \
>/dev/null \
|| fail "provider does not call MCP runtime"

echo "✓ Browser provider connected to MCP runtime"


echo
echo "---- Build Rust runtime ----"

cargo check --manifest-path src-tauri/Cargo.toml \
 >/tmp/p15-browser-provider-mcp.log 2>&1 \
 || {
   grep -E "error:" /tmp/p15-browser-provider-mcp.log | head -20
   fail "cargo check"
 }

echo "✓ Runtime builds"


echo
echo "PASS P15 Browser Provider MCP Bridge"
