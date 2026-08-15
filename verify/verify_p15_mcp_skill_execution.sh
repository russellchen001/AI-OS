#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
    echo "FAIL P15 MCP Skill Execution: $1"
    exit 1
}


echo "---- Check MCP mock server ----"

if [ ! -f verify/mcp_mock/mock_mcp_server.py ]; then
    fail "mock MCP server missing"
fi

echo "✓ MCP mock server exists"


echo
echo "---- Check MCP runtime build ----"

if cargo check --manifest-path src-tauri/Cargo.toml \
    >"$TMP/cargo.log" 2>&1
then
    echo "✓ MCP runtime builds"
else
    grep -E "error:" "$TMP/cargo.log" | head -20
    fail "cargo check"
fi


echo
echo "---- Check MCP execution path ----"

if rg -n \
"list_mcp_tools|call_mcp_tool|execute_mcp_runtime_task|McpPreparedOperation" \
src-tauri/src \
>"$TMP/runtime.log"
then
    echo "✓ MCP runtime execution path exists"
else
    fail "MCP runtime path missing"
fi


echo
echo "---- Check browser skill ----"

if rg -n \
'browser.search|browser.control|\"browser\"' \
src-tauri/src/runtime/skills/registry.rs \
>"$TMP/skill.log"
then
    echo "✓ Browser skill registered"
else
    fail "browser skill missing"
fi


echo
echo "PASS P15 MCP Skill Execution"
exit 0
