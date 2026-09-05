#!/bin/bash
set -u

NAME="P15 Computer Control Phase B (applications, addressed not operated)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-b.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# Listing what is installed, seeing what runs, starting one, asking one to stop.
# Never clicking anything inside one -- that is Computer Use's, and this module
# sends no keystrokes and looks at no windows.

cargo test --manifest-path "$MANIFEST" --lib system::apps::tests \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "Phase B request contract"
  }

grep -q "running 3 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "Phase B contract test count changed"
}

echo "✅ applications are addressed by bundle identifier and never by name"
echo "✅ a localized display name, a path or a name with a space is refused"
echo "✅ a request naming no application fails before anything is started"

have() {
  /usr/bin/osascript -e "tell application \"$1\" to get version" >/dev/null 2>&1
}

if [ "$(uname)" = "Darwin" ]; then
  cargo test --manifest-path "$MANIFEST" --lib \
    system::apps::tests::applications_are_listed_started_and_stopped_real_e2e \
    -- --ignored --exact >"$LOG" 2>&1 || {
      tail -60 "$LOG"
      fail "Phase B real applications"
    }

  grep -q "test result: ok" "$LOG" || fail "Phase B real result marker"

  echo "✅ every listed application carries an identifier that can address it"
  echo "✅ quitting something that is NOT running is refused, not reported as done"
  echo "✅ a second launch says 'already running', which the launch itself cannot"
  echo "✅ an identifier nothing is installed under is refused by its exit status"
  echo "✅ Calculator is started, seen running, asked to stop, and stops"
else
  echo "SKIP $NAME: applications are addressed through macOS here"
fi

echo "PASS $NAME"
