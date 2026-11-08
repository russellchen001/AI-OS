#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
node verify/beta7_setup_layout.server.mjs --verify "${1:?Provide the browser-generated receipt.json path}"
