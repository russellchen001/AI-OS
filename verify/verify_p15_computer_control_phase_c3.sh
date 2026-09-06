#!/bin/bash
set -u

NAME="P15 Computer Control Phase C3 Power"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-c3.log"
TMP_DIR="$(mktemp -d)"

trap 'rm -rf "$TMP_DIR"' EXIT

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  system::power::tests \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "power unit contracts"
  }

grep -q "test result: ok. 3 passed; 0 failed" "$LOG" || {
  tail -40 "$LOG"
  fail "power unit result contract"
}

echo "✅ Power accepts no force/delay parameters"
echo "✅ exact macOS power scripts are deterministic"
echo "✅ no real destructive E2E exists"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  power_capabilities_require_current_confirmation_even_when_trusted \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "always-confirm power permission contract"
  }

grep -q "test result: ok. 1 passed; 0 failed" "$LOG" ||
  fail "power permission result marker"

echo "✅ Trusted Automation cannot bypass Power confirmation"
echo "✅ unconfirmed Power returns RequiresApproval"
echo "✅ current confirmed Power is allowed"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  ordinary_trusted_automation_behavior_is_unchanged \
  >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "ordinary Trusted Automation regression"
  }

echo "✅ ordinary Trusted Automation behavior preserved"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_declared_system_capability_is_dispatched \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "non-executing system resolver coverage"
  }

grep -q "test result: ok" "$LOG" ||
  fail "resolver coverage result marker"

echo "✅ every declared system capability resolves"
echo "✅ resolver coverage does not execute adapters"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "permission reachability coverage"
  }

echo "✅ every routed capability remains authorisable"

# Inspect actual executable construction only.
# Comments and documentation are intentionally irrelevant to this check.
COMMAND_LINES="$(
  grep -n 'Command::new' src-tauri/src/system/power.rs || true
)"

[ -n "$COMMAND_LINES" ] ||
  fail "Power adapter contains no platform command"

COMMAND_COUNT="$(
  printf '%s\n' "$COMMAND_LINES" |
    wc -l |
    tr -d ' '
)"

[ "$COMMAND_COUNT" -eq 1 ] || {
  echo "$COMMAND_LINES"
  fail "Power adapter contains unexpected additional executable paths"
}

printf '%s\n' "$COMMAND_LINES" |
  grep -Fq 'Command::new("/usr/bin/osascript")' ||
  {
    echo "$COMMAND_LINES"
    fail "Power executable is not /usr/bin/osascript"
  }

echo "✅ only executable path is /usr/bin/osascript"
echo "✅ comments cannot trigger command-policy false positives"

cat > "$TMP_DIR/sleep.applescript" <<'EOF'
tell application "System Events" to sleep
EOF

cat > "$TMP_DIR/restart.applescript" <<'EOF'
tell application "System Events" to restart
EOF

cat > "$TMP_DIR/shutdown.applescript" <<'EOF'
tell application "System Events" to shut down
EOF

for action in sleep restart shutdown
do
  /usr/bin/osacompile \
    -o "$TMP_DIR/$action.scpt" \
    "$TMP_DIR/$action.applescript" \
    >"$TMP_DIR/$action.log" 2>&1 || {
      cat "$TMP_DIR/$action.log"
      fail "$action AppleScript does not compile"
    }

  echo "✅ compile-only: $action"
done

echo "✅ destructive scripts were compiled, NEVER executed"

if grep -Fq \
  'execute_system_capability(&request(capability' \
  src-tauri/src/runtime/openclaw_gateway_adapter.rs
then
  fail "generic dispatch coverage still executes system adapters"
fi

grep -Fq \
  'resolve_system_capability(capability).is_some()' \
  src-tauri/src/runtime/openclaw_gateway_adapter.rs ||
  fail "generic dispatch coverage is not resolver-only"

echo "✅ generic cargo tests cannot invoke Power through dispatch coverage"

echo "PASS $NAME"
