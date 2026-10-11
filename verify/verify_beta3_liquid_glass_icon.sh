#!/bin/bash

set -u

FEATURE="Beta.3 Liquid Glass icon"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT/src-tauri/icons"
FAILED=0

pass() {
  printf '✓ %s\n' "$1"
}

fail() {
  printf '✗ %s\n' "$1"
  FAILED=1
}

check_png() {
  local file="$1"
  local expected_width="$2"
  local expected_height="$3"
  local width
  local height
  local alpha

  if [[ ! -f "$file" ]]; then
    fail "missing $(basename "$file")"
    return
  fi

  width="$(sips -g pixelWidth "$file" 2>/dev/null | tail -1 | tr -cd '0-9')"
  height="$(sips -g pixelHeight "$file" 2>/dev/null | tail -1 | tr -cd '0-9')"
  alpha="$(sips -g hasAlpha "$file" 2>/dev/null | tail -1 | awk '{print $2}')"

  if [[ "$width" == "$expected_width" && "$height" == "$expected_height" && "$alpha" == "yes" ]]; then
    pass "$(basename "$file") is ${expected_width}x${expected_height} with alpha"
  else
    fail "$(basename "$file") expected ${expected_width}x${expected_height} with alpha, got ${width}x${height} alpha=${alpha:-unknown}"
  fi
}

check_png "$ICONS/32x32.png" 32 32
check_png "$ICONS/128x128.png" 128 128
check_png "$ICONS/128x128@2x.png" 256 256
check_png "$ICONS/icon.png" 512 512
check_png "$ICONS/AI-OS-Liquid-Glass-1024.png" 1024 1024

if [[ -s "$ICONS/icon.icns" ]] && file "$ICONS/icon.icns" | grep -q 'Mac OS X icon'; then
  pass "icon.icns is a valid macOS icon family"
else
  fail "icon.icns is missing or invalid"
fi

if [[ -s "$ICONS/icon.ico" ]] && file "$ICONS/icon.ico" | grep -q 'MS Windows icon resource'; then
  pass "icon.ico is a valid Windows icon resource"
else
  fail "icon.ico is missing or invalid"
fi

if [[ "$FAILED" -ne 0 ]]; then
  printf 'FAIL %s\n' "$FEATURE"
  exit 1
fi

printf 'PASS %s\n' "$FEATURE"
exit 0
