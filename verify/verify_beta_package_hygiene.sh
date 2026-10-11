#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."
failures=0

unexpected_data=$(
  find . \
    -path './node_modules' -prune -o \
    -path './dist' -prune -o \
    -path './src-tauri/target' -prune -o \
    -type f \( -name 'conversations.sqlite3' -o -name 'memory.sqlite3' -o -name 'openclaw-servers.json' \) \
    -print
)

if [ -n "$unexpected_data" ]; then
  echo "✗ repository contains runtime user data"
  printf '%s\n' "$unexpected_data"
  failures=$((failures + 1))
else
  echo "✓ repository contains no conversation, Memory/knowledge DB or OpenClaw server record"
fi

APP='src-tauri/target/release/bundle/macos/AI OS.app'
version=$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")
DMG="src-tauri/target/release/bundle/dmg/AI OS_${version}_aarch64.dmg"
if [ -d "$APP" ]; then
  bundled_data=$(
    find "$APP" -type f \
      \( -name 'conversations.sqlite3' -o -name 'memory.sqlite3' -o -name 'openclaw-servers.json' \) \
      -print
  )
  if [ -n "$bundled_data" ]; then
    echo "✗ app bundle contains user runtime data"
    printf '%s\n' "$bundled_data"
    failures=$((failures + 1))
  else
    echo "✓ app bundle contains no user conversation, Memory/knowledge or runtime data"
  fi
else
  echo "✓ app bundle hygiene deferred until Beta artifact build"
fi

if [ -f "$DMG" ]; then
  mount_dir=$(mktemp -d "${TMPDIR:-/tmp}/ai-os-beta-hygiene.XXXXXX")
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
    dmg_runtime_data=$(
      find "$mount_dir" -type f \
        \( -name 'conversations.sqlite3' -o \
           -name 'memory.sqlite3' -o \
           -name 'openclaw-servers.json' -o \
           -name 'mcp-servers.json' -o \
           -name '*.keychain-db' \) \
        -print
    )
    if [ -n "$dmg_runtime_data" ]; then
      echo "✗ Beta DMG contains user runtime data"
      printf '%s\n' "$dmg_runtime_data"
      failures=$((failures + 1))
    else
      echo "✓ Beta DMG contains no user chats, runtime config or keychain database"
    fi
  else
    echo "✗ Beta DMG could not be mounted for distribution hygiene verification"
    failures=$((failures + 1))
  fi
else
  echo "✗ Beta DMG is missing: $DMG"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta package/data hygiene: $failures checks failed"
  exit 1
fi

echo "PASS Beta package/data hygiene"
