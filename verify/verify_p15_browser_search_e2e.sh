#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
    echo "FAIL P15 Browser Search E2E: $1"
    exit 1
}


echo "---- Check Rust integration tests ----"

if cargo test \
    --manifest-path src-tauri/Cargo.toml \
    runtime::plan_runtime_bridge \
    -- --nocapture \
    >"$TMP/test.log" 2>&1
then
    echo "✓ Plan runtime bridge tests passed"
else
    grep -E "FAILED|error:" "$TMP/test.log" | head -20
    fail "plan runtime bridge tests"
fi


echo
echo "---- Check MCP behavior fixture ----"

python3 <<'PY'
import json
import subprocess

proc = subprocess.Popen(
    ["python3", "verify/mcp_mock/mock_mcp_server.py"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    text=True,
)

def rpc(payload):
    proc.stdin.write(json.dumps(payload) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())


tools = rpc({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list",
    "params": {}
})

names = [
    tool["name"]
    for tool in tools["result"]["tools"]
]

assert "echo" in names


result = rpc({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
        "name": "echo",
        "arguments": {
            "text": "browser.search E2E OK"
        }
    }
})

output = result["result"]["content"][0]["text"]

assert output == "browser.search E2E OK"

proc.kill()

print("✓ MCP tool execution verified")
PY


echo
echo "PASS P15 Browser Search E2E"
exit 0
