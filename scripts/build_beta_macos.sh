#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
  echo "FAIL Beta macOS build: this release flow requires macOS"
  exit 1
fi

version=$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")
app='src-tauri/target/release/bundle/macos/AI OS.app'
dmg_directory='src-tauri/target/release/bundle/dmg'
dmg="$dmg_directory/AI OS_${version}_aarch64.dmg"
staging=$(mktemp -d "${TMPDIR:-/tmp}/ai-os-beta-dmg.XXXXXX")
temporary_dmg="$dmg_directory/.AI OS_${version}_aarch64.tmp.dmg"

cleanup() {
  if [ -d "$staging" ]; then
    find "$staging" -depth -delete 2>/dev/null || true
  fi
  if [ -f "$temporary_dmg" ]; then
    find "$temporary_dmg" -delete 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "===== Build macOS app ====="
CI=true npm run tauri -- build --bundles app

if [ ! -d "$app" ]; then
  echo "FAIL Beta macOS build: app bundle was not produced"
  exit 1
fi

# The crate has two binaries; the bundle must launch the app, not the MCP.
main_executable=$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$app/Contents/Info.plist")
if [ "$main_executable" != "dashboard" ] || [ ! -x "$app/Contents/MacOS/dashboard" ]; then
  echo "FAIL Beta macOS build: main executable is '$main_executable', expected dashboard"
  exit 1
fi

# Cloud-drive download is a built-in capability: its stdio MCP executable
# ships next to the main binary and is sealed with the app.
echo "===== Bundle built-in Cloud Drive component ====="
(cd src-tauri && cargo build --release --bin ai-os-cloud-drive-mcp)
cloud_drive_mcp='src-tauri/target/release/ai-os-cloud-drive-mcp'
if [ ! -x "$cloud_drive_mcp" ]; then
  echo "FAIL Beta macOS build: Cloud Drive component was not produced"
  exit 1
fi
ditto "$cloud_drive_mcp" "$app/Contents/MacOS/ai-os-cloud-drive-mcp"

echo "===== Seal macOS app bundle ====="
codesign \
  --force \
  --deep \
  --sign - \
  --timestamp=none \
  "$app"

echo "===== Verify macOS app before DMG ====="
codesign \
  --verify \
  --deep \
  --strict \
  --verbose=4 \
  "$app"

signature_details=$(codesign -dvvv "$app" 2>&1)
printf '%s\n' "$signature_details" | grep -qx 'Identifier=com.russellchen.aios'
printf '%s\n' "$signature_details" | grep -qx 'Signature=adhoc'
printf '%s\n' "$signature_details" | grep -Eq '^Info\.plist entries=[1-9][0-9]*$'
printf '%s\n' "$signature_details" | grep -Eq '^Sealed Resources version=2([[:space:]]|$)'

echo "===== Package verified app into DMG ====="
mkdir -p "$dmg_directory"
ditto "$app" "$staging/AI OS.app"
ln -s /Applications "$staging/Applications"
hdiutil create \
  -volname 'AI OS' \
  -srcfolder "$staging" \
  -ov \
  -format UDZO \
  "$temporary_dmg"
mv -f "$temporary_dmg" "$dmg"

echo "===== Verify final artifacts ====="
verify/verify_beta_macos_bundle_signature.sh
verify/verify_beta_package_hygiene.sh

echo "PASS Beta macOS signed release build"
echo "App: $app"
echo "DMG: $dmg"
