#!/usr/bin/env bash
set -u

name="P15 Google Workspace real E2E"
account="google-workspace-default"

if ! security find-generic-password \
  -s com.ai-os.provider \
  -a "$account" \
  >/dev/null 2>&1
then
  echo "SKIP $name: USER_LOGIN_OR_OAUTH_CONSENT_REQUIRED"
  exit 0
fi

output="$(
  cargo test \
    --manifest-path src-tauri/Cargo.toml \
    google_workspace::tests::google_workspace_real_e2e \
    -- --ignored --exact --nocapture \
    2>&1
)"
status=$?

if [[ $status -ne 0 ]]; then
  echo "FAIL $name: real Google identity/Drive/Docs/Sheets/Slides validation failed"
  echo "$output" | tail -30
  exit 1
fi

if [[ "$output" != *"test google_workspace::tests::google_workspace_real_e2e ... ok"* ]]; then
  echo "FAIL $name: the intended real E2E test did not run"
  exit 1
fi

echo "✓ secure Google authorization exists"
echo "✓ real Google identity succeeded"
echo "✓ real Google Drive list succeeded"
echo "✓ real Google Docs create/read-back succeeded"
echo "✓ real Google Sheets create/write/read-back succeeded"
echo "✓ real Google Slides create/read-back succeeded"
echo "✓ temporary E2E resources were removed"
echo "✓ authenticated/completed evidence boundary succeeded"
echo "PASS $name"
