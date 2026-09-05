#!/bin/bash
# Computer Control acceptance gate.
#
# Separate from the Office gate because it is a separate capability area, and
# because most of it does not need any application at all -- so it should not
# be held hostage to whether Word is installed.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOGS="${AIOS_RUN_DIR:-$(mktemp -d)}"
mkdir -p "$LOGS"
cd "$ROOT" || exit 1

step() {
  local label="$1"
  shift
  local log="$LOGS/$(echo "$label" | tr ' /' '__').log"

  printf '....  %s\n' "$label"
  if "$@" >"$log" 2>&1; then
    printf 'PASS  %s\n' "$label"
  else
    printf 'FAIL  %s\n' "$label"
    echo "----- last 60 lines of $log -----"
    tail -60 "$log"
    echo "Logs: $LOGS"
    exit 1
  fi
}

step "Phase A reading"    bash verify/verify_p15_computer_control_phase_a.sh
step "Phase B applications" bash verify/verify_p15_computer_control_phase_b.sh
step "Full Rust tests"    cargo test --manifest-path src-tauri/Cargo.toml
step "Verifier file modes" bash verify/verify_verifiers_are_executable.sh
step "Whitespace"         git diff --check

echo
echo "PASS Computer Control phase gate"
echo "Logs: $LOGS"
