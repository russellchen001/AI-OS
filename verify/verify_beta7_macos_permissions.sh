#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.." || exit 1
FEATURE="beta7_macos_permissions"

fail() {
  echo "❌ $1"
  echo "FAIL: $FEATURE — $1"
  exit 1
}

echo "===== VERIFICATION ====="

cargo test --manifest-path src-tauri/Cargo.toml --lib system::permissions --quiet \
  >/tmp/ai-os-beta7-permissions-rust.log 2>&1 || {
  tail -20 /tmp/ai-os-beta7-permissions-rust.log
  fail "native permission status tests failed"
}
echo "✅ Native permission status tests"

npm run build >/tmp/ai-os-beta7-permissions-build.log 2>&1 || {
  tail -20 /tmp/ai-os-beta7-permissions-build.log
  fail "frontend production build failed"
}
echo "✅ Permission guidance production build"

echo "PASS: $FEATURE"
echo "MANUAL: Open each permission row and confirm macOS System Settings opens at the matching pane."
echo "NOTE: TCC authorization must still be granted by the user and is not claimed by this script."
