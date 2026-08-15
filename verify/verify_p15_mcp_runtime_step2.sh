#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT


fail() {
    echo "FAIL P15 MCP runtime step 2: $1"
    exit 1
}


if python3 verify/mcp_mock/mock_mcp_server.py \
    </dev/null \
    >/dev/null 2>&1
then
    echo "✓ MCP mock server executable"
else
    fail "mock server"
fi


if cargo check --manifest-path src-tauri/Cargo.toml \
    >"$TMP/build.log" 2>&1
then
    echo "✓ MCP runtime builds"
else
    grep -E "error:" "$TMP/build.log" | head -20
    fail "cargo check"
fi


echo "PASS P15 MCP runtime step 2"
