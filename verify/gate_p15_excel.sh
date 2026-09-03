#!/bin/bash
# Excel acceptance gate. Runs every phase once, in order, and prints one line
# per step so a failure is identifiable without reading every log.
#
# Phase verifiers deliberately do NOT run each other. When they did, the chain
# was quadratic -- Phase F pulled in E, which pulled in D, which pulled in C --
# and one gate cost 32 real Excel cycles.
#
# Logs go to $AIOS_RUN_DIR when the runner provides one, so they land inside the
# repository where they can actually be read, and to a temp directory otherwise.
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

step "Phase A"                   bash verify/verify_p15_excel_phase_a.sh
step "Phase B mutation"          bash verify/verify_p15_excel_phase_b_mutation.sh
step "Phase B read"              bash verify/verify_p15_excel_phase_b_read.sh
step "Phase C structural"        bash verify/verify_p15_excel_phase_c_structural.sh
step "Phase D formatting"        bash verify/verify_p15_excel_phase_d_formatting.sh
step "Phase E sort and filter"   bash verify/verify_p15_excel_phase_e_sort_filter.sh
step "Phase F charts"            bash verify/verify_p15_excel_phase_f_charts.sh
step "Phase G formula read"      bash verify/verify_p15_excel_phase_g_formula_read.sh
step "Spreadsheet Create"        bash verify/verify_p15_spreadsheet_create.sh
step "Spreadsheet Read"          bash verify/verify_p15_spreadsheet_read.sh
step "Full Rust tests"           cargo test --manifest-path src-tauri/Cargo.toml
step "Frontend build"            npm run build
step "Whitespace"                git diff --check

echo
echo "PASS Excel phase gate"
echo "Logs: $LOGS"
