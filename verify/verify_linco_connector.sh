#!/usr/bin/env bash
# Built-in Linco connector: AI-OS itself is the linco-connect side of the
# official Linco channel. No separate Linco program or local server.

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0

check() {
  local label="$1"
  shift
  if "$@"; then
    echo "PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $label"
    FAIL=$((FAIL + 1))
  fi
}

check "Connector protocol, pairing and rendering tests" \
  bash -c 'cargo test --manifest-path src-tauri/Cargo.toml --lib linco_connector::tests 2>&1 | grep -Eq "test result: ok\. [1-9][0-9]* passed; 0 failed"'
check "Remote AI-OS dispatch tests" \
  bash -c 'cargo test --manifest-path src-tauri/Cargo.toml --lib remote_linco::tests 2>&1 | grep -Eq "test result: ok\. [1-9][0-9]* passed; 0 failed"'
check "Connector starts with AI-OS and stops on exit" \
  bash -c 'rg -q "linco_connector::start\(app.handle\(\)" src-tauri/src/lib.rs && rg -q "linco_connector::shutdown" src-tauri/src/lib.rs'
check "Official channel over wss only" \
  bash -c 'rg -q "wss://app.lincotalk.com/socket/ai" src-tauri/src/linco_connector.rs && rg -q "must use wss://" src-tauri/src/linco_connector.rs'
check "Pairing token kept in Keychain, not in the settings file" \
  bash -c 'rg -q "store_secret\(KEYCHAIN_ACCOUNT" src-tauri/src/linco_connector.rs && ! rg -q "app_secret" <(sed -n "/struct Settings/,/^}/p" src-tauri/src/linco_connector.rs)'
check "Messages go through Remote AI-OS orchestration only" \
  bash -c 'rg -q "remote_linco::dispatch_inbound" src-tauri/src/linco_connector.rs && ! rg -n "openclaw_gateway|SkillInvocationGateway|Command::new" src-tauri/src/linco_connector.rs'
check "No separate Linco server dependency" \
  bash -c '! rg -n "127\.0\.0\.1:3300|linco-connect start|npm install -g linco-connect" src --glob "!src/components/LincoPhoneAccess.tsx"'
check "Pairing UI present" \
  bash -c 'rg -q "pairLincoConnector" src/components/LincoPhoneAccess.tsx && rg -q "LincoPhoneAccess" src/pages/AccountsServicesPage.tsx'
check "TypeScript" npx tsc --noEmit
check "Git whitespace validation" git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"
if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: Linco connector"
  exit 1
fi
echo "PASS: Linco connector"
