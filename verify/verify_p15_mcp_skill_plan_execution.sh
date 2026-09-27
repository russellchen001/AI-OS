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


echo "---- Check MCP Agent Skill Transport bridge ----"

if rg -q \
"execute_mcp_runtime_task" \
src-tauri/src/runtime/plan_runtime_bridge.rs
then
    fail "legacy direct Runtime MCP branch still exists"
fi

if rg -n \
"execute_mcp_runtime_task" \
src-tauri/src/runtime/agent_skill_transport.rs \
>"$TMP/bridge.log"
then
    echo "✓ MCP execution is owned by Agent Skill Transport"
else
    fail "Agent Skill Transport MCP branch missing"
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
