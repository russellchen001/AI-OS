#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
if ! node verify/beta7_chat_local_routing.behavior.mjs; then
  echo 'FAIL: beta7_chat_local_routing — behavior regression'
  exit 1
fi
echo 'PASS: beta7_chat_local_routing'
