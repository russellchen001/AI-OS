#!/bin/bash

set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Step 1 compile verification"

if cargo check --manifest-path src-tauri/Cargo.toml >/tmp/ai-os-ac-exec-check.log 2>&1; then
  echo "✓ AI Center execution-agent candidate API compiles"
  echo "PASS AC-EXEC-MODEL Step 1 compile"
  exit 0
fi

echo "✗ Rust compilation failed"
tail -20 /tmp/ai-os-ac-exec-check.log
echo "FAIL AC-EXEC-MODEL Step 1 compile"
exit 1
