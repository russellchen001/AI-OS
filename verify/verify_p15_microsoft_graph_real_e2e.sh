#!/usr/bin/env bash
set -u

name="P15 Microsoft Graph real E2E"
account="microsoft-graph-default"

if ! security find-generic-password -s com.ai-os.provider -a "$account" >/dev/null 2>&1; then
  echo "SKIP $name: USER_AUTHORIZATION_REQUIRED"
  exit 0
fi

for variable in AI_OS_GRAPH_E2E_DRIVE_ID AI_OS_GRAPH_E2E_ITEM_ID AI_OS_GRAPH_E2E_WORKSHEET AI_OS_GRAPH_E2E_RANGE; do
  if [[ -z "${!variable:-}" ]]; then
    echo "BLOCKED $name: $variable is required to select a non-destructive workbook range"
    exit 2
  fi
done

output="$(cargo test --manifest-path src-tauri/Cargo.toml microsoft_graph::tests::microsoft_graph_real_e2e -- --ignored --exact --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: real Graph identity/workbook/write/read-back failed"
  echo "$output" | tail -20
  exit 1
fi
if [[ "$output" != *"test microsoft_graph::tests::microsoft_graph_real_e2e ... ok"* ]]; then
  echo "FAIL $name: the intended real E2E test did not run"
  exit 1
fi
echo "✓ secure authorization exists"
echo "✓ real /me identity succeeded"
echo "✓ real workbook read succeeded"
echo "✓ real write and read-back validation succeeded"
echo "✓ authenticated/completed evidence boundary succeeded"
echo "PASS $name"
