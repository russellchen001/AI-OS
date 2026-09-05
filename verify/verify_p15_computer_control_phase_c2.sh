#!/bin/bash
set -u

NAME="P15 Computer Control Phase C2 Audio"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/computer-control-phase-c2.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

audio_state() {
  /usr/bin/osascript -e '
set currentSettings to get volume settings
return ((output volume of currentSettings) as text) & "|" & ((output muted of currentSettings) as text)
'
}

echo "$NAME"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  system::audio::tests \
  >"$LOG" 2>&1 || {
    tail -100 "$LOG"
    fail "audio request contract"
  }

# Four tests are discovered on macOS:
#   3 normal contract tests + 1 ignored real state-changing E2E.
# The ignored test is deliberately run separately below.
grep -q "test result: ok. 3 passed; 0 failed; 1 ignored" "$LOG" ||
  {
    tail -40 "$LOG"
    fail "audio contract/ignored-E2E discovery contract changed"
  }

echo "✅ 3 audio contract tests PASS"
echo "✅ real audio E2E discovered and intentionally ignored here"
echo "✅ volume requires integer 0 through 100"
echo "✅ mute requires boolean"
echo "✅ malformed platform state fails closed"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_declared_system_capability_is_dispatched \
  >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "system dispatch coverage"
  }

grep -q "test result: ok" "$LOG" ||
  fail "dispatch result marker"

echo "✅ declared capability dispatch coverage"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >"$LOG" 2>&1 || {
    tail -80 "$LOG"
    fail "permission coverage"
  }

grep -q "test result: ok" "$LOG" ||
  fail "permission result marker"

echo "✅ permission gate coverage"

COMMAND_LINES="$(
  grep 'Command::new' src-tauri/src/system/audio.rs || true
)"

[ -n "$COMMAND_LINES" ] ||
  fail "audio module contains no platform command"

if printf '%s\n' "$COMMAND_LINES" |
  grep -v 'Command::new("/usr/bin/osascript")' >/dev/null
then
  echo "$COMMAND_LINES"
  fail "audio module invokes a command other than osascript"
fi

if grep -Eq \
  'tell application "System Events"|keystroke|CGEvent|cliclick' \
  src-tauri/src/system/audio.rs
then
  fail "audio adapter crossed into GUI automation"
fi

echo "✅ deterministic system adapter only"
echo "✅ no screen or keystroke automation"

if [ "$(uname)" = "Darwin" ]; then
  BEFORE="$(audio_state)" ||
    fail "could not read pre-test audio state"

  cargo test \
    --manifest-path "$MANIFEST" \
    --lib \
    system::audio::tests::audio_state_changes_and_returns_to_exact_original_real_e2e \
    -- --ignored --exact \
    >"$LOG" 2>&1 || {
      AFTER="$(audio_state 2>/dev/null || true)"
      echo "Before: $BEFORE"
      echo "After failure: $AFTER"
      tail -120 "$LOG"
      fail "real macOS audio E2E"
    }

  grep -q "test result: ok. 1 passed; 0 failed" "$LOG" ||
    {
      tail -40 "$LOG"
      fail "real audio E2E result marker"
    }

  AFTER="$(audio_state)" ||
    fail "could not read post-test audio state"

  [ "$BEFORE" = "$AFTER" ] || {
    echo "Before: $BEFORE"
    echo "After:  $AFTER"
    fail "audio state was not exactly restored"
  }

  echo "✅ output volume changed and read back"
  echo "✅ mute false and true both observed"
  echo "✅ exact original volume/mute restored"
fi

echo "PASS $NAME"
