#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
    echo "FAIL P15 MCP Skill Plan Execution: $1"
    exit 1
}


echo "---- Check MCP runtime bridge ----"

if rg -n \
"execute_mcp_runtime_task|executor_kind\\.as_str\\(\\)|\"mcp\"" \
src-tauri/src/runtime/plan_runtime_bridge.rs \
>"$TMP/bridge.log"
then
    echo "✓ Plan runtime MCP executor path exists"
else
    fail "plan runtime MCP branch missing"
fi


echo
echo "---- Check MCP executor ----"

if rg -n \
"McpPreparedOperation|list_mcp_tools|call_mcp_tool|execute_mcp_runtime_task" \
src-tauri/src/runtime \
src-tauri/src/mcp_runtime.rs \
>"$TMP/executor.log"
then
    echo "✓ MCP executor implementation exists"
else
    fail "MCP executor missing"
fi


echo
echo "---- Check browser skill contract ----"

if rg -n \
"browser.search|browser.control|executor.*mcp|handler.*browser" \
src-tauri/src/runtime/skills/registry.rs \
>"$TMP/skill.log"
then
    echo "✓ Browser MCP skill registered"
else
    fail "browser MCP skill missing"
fi


echo
echo "---- Build Rust runtime ----"

if cargo check --manifest-path src-tauri/Cargo.toml \
    >"$TMP/cargo.log" 2>&1
then
    echo "✓ Runtime builds"
else
    grep -E "error:" "$TMP/cargo.log" | head -20
    fail "cargo check"
fi


echo
echo "PASS P15 MCP Skill Plan Execution"
exit 0
