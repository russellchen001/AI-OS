#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

APP='src-tauri/target/release/bundle/macos/AI OS.app'
DMG='src-tauri/target/release/bundle/dmg/AI OS_1.0.1-beta.2_aarch64.dmg'
failures=0

verify_app() {
  label="$1"
  app_path="$2"

  if [ ! -d "$app_path" ]; then
    echo "✗ $label is missing: $app_path"
    failures=$((failures + 1))
    return
  fi

  verify_output=$(codesign --verify --deep --strict --verbose=4 "$app_path" 2>&1)
  verify_code=$?
  if [ "$verify_code" -ne 0 ]; then
    echo "✗ $label codesign verification failed"
    printf '%s\n' "$verify_output"
    failures=$((failures + 1))
    return
  fi

  printf '%s\n' "$verify_output"
  details=$(codesign -dvvv "$app_path" 2>&1)

  if printf '%s\n' "$details" | grep -qx 'Identifier=com.russellchen.aios'; then
    echo "✓ $label Identifier=com.russellchen.aios"
  else
    echo "✗ $label identifier is not com.russellchen.aios"
    failures=$((failures + 1))
  fi

  if printf '%s\n' "$details" | grep -qx 'Signature=adhoc'; then
    echo "✓ $label Signature=adhoc"
  else
    echo "✗ $label signature is not ad-hoc"
    failures=$((failures + 1))
  fi

  if printf '%s\n' "$details" | grep -Eq '^Info\.plist entries=[1-9][0-9]*$' &&
     ! printf '%s\n' "$details" | grep -q '^Info\.plist=not bound$'; then
    echo "✓ $label Info.plist is bound"
  else
    echo "✗ $label Info.plist is not bound"
    failures=$((failures + 1))
  fi

  if printf '%s\n' "$details" | grep -Eq '^Sealed Resources version=2([[:space:]]|$)'; then
    echo "✓ $label sealed resources are present"
  else
    echo "✗ $label sealed resources are missing"
    failures=$((failures + 1))
  fi
}

verify_app 'built app' "$APP"

if [ ! -f "$DMG" ]; then
  echo "✗ Beta DMG is missing: $DMG"
  failures=$((failures + 1))
else
  mount_dir=$(mktemp -d "${TMPDIR:-/tmp}/ai-os-beta-signature.XXXXXX")
  mounted=0
  cleanup() {
    if [ "$mounted" -eq 1 ]; then
      hdiutil detach "$mount_dir" >/dev/null 2>&1 || true
    fi
    rmdir "$mount_dir" >/dev/null 2>&1 || true
  }
  trap cleanup EXIT

  if hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$DMG" >/dev/null; then
    mounted=1
    verify_app 'DMG app' "$mount_dir/AI OS.app"
  else
    echo "✗ Beta DMG could not be mounted read-only"
    failures=$((failures + 1))
  fi
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta macOS bundle signature: $failures checks failed"
  exit 1
fi

echo "PASS Beta macOS bundle signature"
