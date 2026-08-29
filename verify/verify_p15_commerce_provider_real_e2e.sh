#!/usr/bin/env bash
set -u

name="P15 commerce provider real E2E"
output="$(cargo test --manifest-path src-tauri/Cargo.toml commerce_provider::tests -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: capability boundary tests failed"
  echo "$output" | tail -20
  exit 1
fi
echo "eBay: SKIP — developer application and production approval required"
echo "Amazon: BROWSER_REQUIRED — ordinary consumer cart/checkout API unavailable"
echo "Taobao: BROWSER_REQUIRED — approved ordinary consumer checkout API unavailable"
echo "JD: BROWSER_REQUIRED — approved ordinary consumer checkout API unavailable"
echo "Pinduoduo: BROWSER_REQUIRED — approved ordinary consumer checkout API unavailable"
echo "PASS P15 commerce provider capability boundaries"
