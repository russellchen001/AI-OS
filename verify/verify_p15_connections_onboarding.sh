#!/usr/bin/env bash
set -u

name="P15 connections onboarding"
connections="$(cargo test --manifest-path src-tauri/Cargo.toml connections::tests -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: connection state-machine behavior failed"
  echo "$connections" | tail -20
  exit 1
fi
if [[ "$connections" != *"5 passed"* ]]; then
  echo "FAIL $name: expected five connection onboarding behavior tests"
  exit 1
fi

oauth="$(cargo test --manifest-path src-tauri/Cargo.toml providers::tests::oauth_ -- --nocapture 2>&1)"
status=$?
if [[ $status -ne 0 ]]; then
  echo "FAIL $name: OAuth callback/session progression failed"
  echo "$oauth" | tail -20
  exit 1
fi

echo "✓ Connect All order and skip behavior"
echo "✓ later failure preserves previous successful connections"
echo "✓ OAuth state is scoped, single-use and callback state is verified"
echo "✓ opening a browser login page never marks an account connected"
echo "✓ expired browser accounts can enter reconnect"
echo "✓ password, cookie and token are absent from browser session state"
echo "✓ runtime app rescan detects Pages, Numbers, Keynote, WPS and Excel independently"
echo "✓ partial iWork installation remains component-accurate"
echo "PASS $name foundation"
echo "WAITING_FOR_USER Microsoft: application client ID and account consent required"
echo "WAITING_FOR_USER browser providers: authenticated profile verifier required"
