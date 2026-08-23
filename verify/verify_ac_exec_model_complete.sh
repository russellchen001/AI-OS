#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "AC-EXEC-MODEL Complete"

run_check() {
  NAME="$1"
  SCRIPT="$2"
  LOG="/tmp/ac-exec-model-complete-last.log"

  if bash "$SCRIPT" >"$LOG" 2>&1; then
    echo "✓ $NAME"
  else
    tail -30 "$LOG"
    echo "✗ $NAME"
    echo "FAIL AC-EXEC-MODEL Complete: $NAME"
    exit 1
  fi
}

run_check \
  "AI Center execution-agent selection" \
  "verify/verify_ac_exec_model_step1.sh"

run_check \
  "Single-agent execution boundary" \
  "verify/verify_ac_exec_model_step3a.sh"

run_check \
  "Retryable file-verification boundary" \
  "verify/verify_ac_exec_model_step3b1.sh"

run_check \
  "Sequential candidate fallback" \
  "verify/verify_ac_exec_model_step3b2.sh"

run_check \
  "Two-attempt fallback policy" \
  "verify/verify_ac_exec_model_step3b3.sh"

echo "PASS AC-EXEC-MODEL Complete"
exit 0
