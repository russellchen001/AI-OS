#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"


fail() {
    echo "FAIL P15 Browser Complete: $1"
    exit 1
}


run_check() {
    local name="$1"
    local script="$2"

    echo
    echo "---- $name ----"

    if ./"$script" >/tmp/browser-check.log 2>&1
    then
        cat /tmp/browser-check.log
        echo "✓ $name passed"
    else
        cat /tmp/browser-check.log
        fail "$name"
    fi
}


run_check \
"Browser Provider Registry" \
"verify/verify_p15_browser_provider_registry.sh"


run_check \
"Browser Provider Execution" \
"verify/verify_p15_browser_provider_execution.sh"


run_check \
"Browser Provider MCP Bridge" \
"verify/verify_p15_browser_provider_mcp_bridge.sh"


run_check \
"Browser Search E2E" \
"verify/verify_p15_browser_search_e2e.sh"


echo
echo "PASS P15 Browser Complete"
