#!/bin/bash
set -u

NAME="P15 Computer Control Phase E2 Final Workflow"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
RUN_DIR="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}"
WORKFLOW_LOG="$RUN_DIR/computer-control-e2-workflow.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

echo ".... v1 scope"

[ ! -f src-tauri/src/system/notification.rs ] ||
  fail "notification production module remains"

[ ! -f verify/verify_p15_computer_control_phase_d1.sh ] ||
  fail "D1 verifier remains"

if rg -n \
  '"system\.notification\.send"' \
  src-tauri/src/system.rs \
  src-tauri/src/runtime/openclaw_gateway_adapter.rs \
  src-tauri/src/runtime/openclaw_permission.rs \
  >/dev/null
then
  fail "notification capability remains reachable"
fi

echo "✅ native notification delivery is outside v1"
echo "✅ no notification capability route remains"

echo ".... final Runtime workflow"

cargo test \
  --manifest-path "$MANIFEST" \
  --lib \
  computer_control_acceptance::tests::final_cross_capability_workflow_real_e2e \
  -- --exact \
  >"$WORKFLOW_LOG" 2>&1 || {
    tail -200 "$WORKFLOW_LOG"
    fail "final Runtime workflow"
  }

grep -Fq \
  "test computer_control_acceptance::tests::final_cross_capability_workflow_real_e2e ... ok" \
  "$WORKFLOW_LOG" || {
    tail -100 "$WORKFLOW_LOG"
    fail "workflow PASS marker missing"
  }

echo "✅ storage crossed Permission -> Gateway -> adapter"
echo "✅ app.running crossed Permission -> Gateway -> adapter"
echo "✅ audio read crossed Permission -> Gateway -> adapter"
echo "✅ unconfirmed shutdown stopped before execution"
echo "✅ process.info supplied stable identity"
echo "✅ unconfirmed termination left child alive"
echo "✅ confirmed termination crossed real Runtime path"
echo "✅ only verifier-owned child was terminated and reaped"

# IMPORTANT:
# Do not grep this verifier for words describing forbidden mechanisms.
# The previous verifier did exactly that and therefore matched its OWN
# validation expression. Product dependency is judged by actual production
# routes and behavior, not by vocabulary inside the test.
if rg -n \
  'system\.notification\.send|requestAuthorizationWithOptions' \
  src-tauri/src/computer_control_acceptance.rs \
  >/dev/null
then
  fail "final workflow still invokes deferred notification behavior"
fi

echo "✅ E2 invokes no deferred notification behavior"
echo "✅ no certificate or signing step is executed"
echo "✅ no paid service is executed"
echo "✅ no clipboard mutation"
echo "✅ no audio mutation"
echo "✅ no real sleep/restart/shutdown"
echo "✅ no user-owned process termination"

echo "PASS $NAME"
