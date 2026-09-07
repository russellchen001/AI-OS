#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Final Gate"

FMT_LOG="/tmp/ac-exec-final-fmt.log"
SHARED_LOG="/tmp/ac-exec-final-shared.log"
E2E_LOG="/tmp/ac-exec-final-e2e.log"
COMPLETE_LOG="/tmp/ac-exec-final-complete.log"

if bash verify/rustfmt_changed.sh >"$FMT_LOG" 2>&1; then
  echo "✓ Changed Rust formatting passed"
else
  tail -40 "$FMT_LOG"
  echo "✗ Rust formatting failed"
  echo "FAIL AC-EXEC-MODEL Final Gate: rustfmt"
  exit 1
fi

if bash verify/verify_p13_m5_shared_multi_model.sh >"$SHARED_LOG" 2>&1; then
  echo "✓ P13 shared multi-model regression passed"
else
  tail -50 "$SHARED_LOG"
  echo "✗ P13 shared multi-model regression failed"
  echo "FAIL AC-EXEC-MODEL Final Gate: shared multi-model"
  exit 1
fi

if bash verify/verify_ai_center_e2e_qa.sh >"$E2E_LOG" 2>&1; then
  echo "✓ AI Center E2E regression passed"
else
  tail -50 "$E2E_LOG"
  echo "✗ AI Center E2E regression failed"
  echo "FAIL AC-EXEC-MODEL Final Gate: AI Center E2E"
  exit 1
fi

if bash verify/verify_ac_exec_model_complete.sh >"$COMPLETE_LOG" 2>&1; then
  echo "✓ AC-EXEC-MODEL complete verification passed"
else
  tail -50 "$COMPLETE_LOG"
  echo "✗ AC-EXEC-MODEL complete verification failed"
  echo "FAIL AC-EXEC-MODEL Final Gate: complete verification"
  exit 1
fi

echo "PASS AC-EXEC-MODEL Final Gate"
exit 0
