#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."
failures=0

unexpected_data=$(
  find . \
    -path './node_modules' -prune -o \
    -path './dist' -prune -o \
    -path './src-tauri/target' -prune -o \
    -type f \( -name 'conversations.sqlite3' -o -name 'openclaw-servers.json' \) \
    -print
)

if [ -n "$unexpected_data" ]; then
  echo "✗ repository contains runtime user data"
  printf '%s\n' "$unexpected_data"
  failures=$((failures + 1))
else
  echo "✓ repository contains no conversation DB or OpenClaw server record"
fi

APP='src-tauri/target/release/bundle/macos/AI OS.app'
if [ -d "$APP" ]; then
  bundled_data=$(
    find "$APP" -type f \
      \( -name 'conversations.sqlite3' -o -name 'openclaw-servers.json' \) \
      -print
  )
  if [ -n "$bundled_data" ]; then
    echo "✗ app bundle contains user runtime data"
    printf '%s\n' "$bundled_data"
    failures=$((failures + 1))
  else
    echo "✓ app bundle contains no user runtime data"
  fi
else
  echo "✓ app bundle hygiene deferred until Beta artifact build"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta package/data hygiene: $failures checks failed"
  exit 1
fi

echo "PASS Beta package/data hygiene"
