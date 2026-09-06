#!/bin/bash
set -u

NAME="P15 Computer Control Phase E1 Process Termination"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
RUN_DIR="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}"
UNIT_LOG="$RUN_DIR/computer-control-phase-e1-unit.log"
PERMISSION_LOG="$RUN_DIR/computer-control-phase-e1-permission.log"
ROUTE_LOG="$RUN_DIR/computer-control-phase-e1-route.log"
REAL_LOG="$RUN_DIR/computer-control-phase-e1-real.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

SOURCE="src-tauri/src/system/process_control.rs"
PERMISSION_SOURCE="src-tauri/src/runtime/openclaw_permission.rs"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  system::process_control::tests \
  >"$UNIT_LOG" 2>&1 || {
    tail -140 "$UNIT_LOG"
    fail "process termination unit contracts"
  }

for test_name in \
  termination_request_requires_exact_process_identity \
  special_and_self_process_targets_are_rejected_before_signalling
do
  grep -Fq \
    "test system::process_control::tests::$test_name ... ok" \
    "$UNIT_LOG" ||
    {
      tail -80 "$UNIT_LOG"
      fail "missing passing contract test: $test_name"
    }
done

grep -Fq \
  "system::process_control::tests::disposable_child_is_identity_checked_terminated_and_observed_real_e2e ... ignored" \
  "$UNIT_LOG" ||
  {
    tail -80 "$UNIT_LOG"
    fail "real E2E was not discovered as ignored"
  }

grep -Fq "test result: ok" "$UNIT_LOG" ||
  fail "unit test result was not ok"

echo "✅ pid and expected start time are both mandatory"
echo "✅ extra force/signal/delay parameters are rejected"
echo "✅ pid 0 / 1 and AI-OS self-target are rejected"
echo "✅ real termination E2E is isolated behind ignored test"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  runtime::openclaw_permission::tests::process_termination_always_requires_current_confirmation \
  -- --exact \
  >"$PERMISSION_LOG" 2>&1 || {
    tail -120 "$PERMISSION_LOG"
    fail "always-confirm process termination policy"
  }

echo "✅ unconfirmed process termination -> RequiresApproval"
echo "✅ confirmed process termination -> Allowed"
echo "✅ Trusted Automation cannot bypass current confirmation"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_declared_system_capability_is_dispatched \
  >"$ROUTE_LOG" 2>&1 || {
    tail -120 "$ROUTE_LOG"
    fail "system resolver coverage"
  }

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  every_capability_the_resolver_can_route_is_one_a_person_can_authorise \
  >>"$ROUTE_LOG" 2>&1 || {
    tail -120 "$ROUTE_LOG"
    fail "permission reachability coverage"
  }

echo "✅ system.process.terminate resolves without execution"
echo "✅ routed capability remains authorisable"

PRODUCTION="$(
  sed '/#\[cfg(test)\]/,$d' "$SOURCE"
)"

printf '%s\n' "$PRODUCTION" |
  grep -Fq 'kill_with(Signal::Term)' ||
  fail "production adapter does not use normal termination signal"

if printf '%s\n' "$PRODUCTION" |
  grep -Eq 'Signal::Kill|\.kill\(\)|kill_and_wait|kill_with_and_wait'
then
  fail "production adapter contains a force-kill path"
fi

if printf '%s\n' "$PRODUCTION" |
  grep -Eq 'Command::new|/bin/kill|/usr/bin/kill|pkill|killall'
then
  fail "production adapter shells out for process termination"
fi

grep -Fq 'process.start_time()' "$SOURCE" ||
  fail "PID reuse identity check is missing"

grep -Fq 'expectedStartTimeUnixSeconds' "$SOURCE" ||
  fail "expected process birth time is missing from request contract"

grep -Fq 'std::process::id()' "$SOURCE" ||
  fail "AI-OS self-protection is missing"

grep -Fq 'pid <= 1' "$SOURCE" ||
  fail "special PID protection is missing"

grep -Fq 'AI-OS did not escalate to a force kill' "$SOURCE" ||
  fail "non-escalation contract is missing"

echo "✅ PID reuse protection is enforced before signalling"
echo "✅ only Signal::Term is used"
echo "✅ no force-kill fallback"
echo "✅ no shell / pkill / killall path"
echo "✅ successful result requires post-signal observation"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  system::process_control::tests::disposable_child_is_identity_checked_terminated_and_observed_real_e2e \
  -- --ignored --exact \
  >"$REAL_LOG" 2>&1 || {
    tail -160 "$REAL_LOG"
    fail "disposable child real E2E"
  }

grep -Fq \
  "test system::process_control::tests::disposable_child_is_identity_checked_terminated_and_observed_real_e2e ... ok" \
  "$REAL_LOG" ||
  {
    tail -100 "$REAL_LOG"
    fail "real E2E PASS marker missing"
  }

echo "✅ stale process identity was rejected without signalling"
echo "✅ disposable /bin/sleep child remained alive after stale identity"
echo "✅ exact child identity received normal termination"
echo "✅ original process termination was observed"
echo "✅ disposable child was reaped by its test parent"

echo "PASS $NAME"
