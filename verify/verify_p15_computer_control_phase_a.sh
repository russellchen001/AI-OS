#!/bin/bash
set -u

NAME="P15 Computer Control Phase A (reading the machine, on any system)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-a.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# Computer Control is defined by its boundary: deterministic, system-level,
# directly callable. Not a second Agent, no planning, and nothing that reads
# the screen -- that is Computer Use's.
#
# Phase A is the reading half, and it is portable on purpose. The numbers come
# from one pure-Rust crate that supports macOS, Windows and Linux, so this
# capability area starts on the right side of the rule the owner set for a
# release that must not require a particular operating system.

cargo test --manifest-path "$MANIFEST" --lib system::inspect::tests \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "Phase A reading contract"
  }

grep -q "running 3 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "Phase A contract test count changed"
}

echo "✅ what is read holds together on whatever machine reads it"
echo "✅ a volume that reported no size carries no numbers it never had"
echo "✅ volumes are per mount point and the answer says not to add them up"
echo "✅ processor use is measured across two samples, not since boot"
echo "✅ the process list is bounded, ordered as claimed, and says what it omitted"
echo "✅ a pid nothing is running under is a request problem, not a failure"

cargo test --manifest-path "$MANIFEST" --lib \
  runtime::openclaw_gateway_adapter::tests::every_declared_system_capability_is_dispatched \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "system capability dispatch"
  }

echo "✅ every capability the module declares is one the dispatcher answers"

cargo test --manifest-path "$MANIFEST" --lib \
  runtime::openclaw_permission::tests::every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "system capability authorisation"
  }

echo "✅ and one a person can actually authorise -- declared, dispatched, permitted"

echo "PASS $NAME"
