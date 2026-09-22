#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

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

check "Desktop inbound behavior" node_modules/.bin/esbuild \
  verify/p16_desktop_linco.behavior.ts --bundle --platform=node --format=esm \
  --outfile="$TMP_DIR/desktop-linco.mjs"
check "Desktop inbound execution" node "$TMP_DIR/desktop-linco.mjs"
check "Desktop bridge Rust tests" cargo test --manifest-path src-tauri/Cargo.toml remote_linco::tests --quiet
check "No direct OpenClaw or Skill gateway bypass" \
  bash -c '! rg -n "OpenClaw|openclaw_gateway|SkillInvocationGateway|SkillInvocationRequest" src/services/desktopLincoInbound.ts src-tauri/src/remote_linco.rs'
check "Loopback-only bind guard" \
  bash -c 'rg -n "starts_with\(\"127\\.0\\.0\\.1:\"\)" src-tauri/src/remote_linco.rs >/dev/null'
check "TypeScript" npx tsc --noEmit
check "Git whitespace validation" git diff --check

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"
if [ "$FAIL" -ne 0 ]; then
  echo "FAIL: P16 Desktop Linco inbound"
  exit 1
fi
echo "PASS: P16 Desktop Linco inbound"
