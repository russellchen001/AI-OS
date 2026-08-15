#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$ROOT"

fail() {
    echo "FAIL P15 MCP Tool Execution: $1"
    exit 1
}


echo "---- Validate MCP stdio protocol ----"

python3 <<'PY'
import json
import subprocess

proc = subprocess.Popen(
    ["python3", "verify/mcp_mock/mock_mcp_server.py"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

def call(payload):
    proc.stdin.write(json.dumps(payload) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())


response = call({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list",
    "params": {}
})

tools = response["result"]["tools"]

assert any(
    tool["name"] == "echo"
    for tool in tools
), "echo tool missing"


response = call({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
        "name": "echo",
        "arguments": {
            "text": "AI-OS MCP OK"
        }
    }
})

text = response["result"]["content"][0]["text"]

assert text == "AI-OS MCP OK", text


proc.kill()

print("✓ tools/list verified")
print("✓ tools/call verified")
PY


if [ $? -ne 0 ]; then
    fail "MCP protocol behavior"
fi


echo
echo "PASS P15 MCP Tool Execution"
exit 0
