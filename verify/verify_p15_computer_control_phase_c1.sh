#!/bin/bash
set -u

NAME="P15 Computer Control Phase C1 Clipboard"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-c1.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

cargo test --manifest-path "$MANIFEST" --lib \
  system::clipboard::tests >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "clipboard request contract"
  }

grep -q "running 2 tests" "$LOG" ||
  fail "clipboard contract test count changed"

echo "✅ text-only clipboard contract"
echo "✅ 65536-byte bound"
echo "✅ invalid write input fails closed"

cargo test --manifest-path "$MANIFEST" --lib \
  every_declared_system_capability_is_dispatched >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "declared capability dispatch coverage"
  }

grep -q "test result: ok" "$LOG" ||
  fail "dispatch coverage result marker"

echo "✅ declared capability dispatch coverage"

cargo test --manifest-path "$MANIFEST" --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "permission gate regression"
  }

grep -q "test result: ok" "$LOG" ||
  fail "permission regression result marker"

echo "✅ permission gate regression"

grep -q 'Command::new("/usr/bin/pbpaste")' \
  src-tauri/src/system/clipboard.rs ||
  fail "clipboard read is not using the deterministic platform adapter"

grep -q 'Command::new("/usr/bin/pbcopy")' \
  src-tauri/src/system/clipboard.rs ||
  fail "clipboard write is not using the deterministic platform adapter"

if grep -Eq 'osascript|keystroke|System Events|CGEvent|cliclick' \
  src-tauri/src/system/clipboard.rs; then
  fail "clipboard adapter crossed into Computer Use"
fi

echo "✅ no screen, keystroke or GUI automation"
echo "PASS $NAME"
