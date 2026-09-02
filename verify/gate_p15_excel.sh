#!/bin/bash
# Excel Office phase acceptance gate. Runs the whole handoff checklist in one pass
# and prints one line per step, so a failure is identifiable without reading
# every log. Full logs are kept and their paths are printed at the end.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOGS="$(mktemp -d)"
cd "$ROOT" || exit 1

step() {
  local label="$1"
  shift
  local log="$LOGS/$(echo "$label" | tr ' /' '__').log"
  if "$@" >"$log" 2>&1; then
    echo "PASS  $label"
  else
    echo "FAIL  $label"
    echo "----- last 60 lines of $log -----"
    tail -60 "$log"
    echo "Logs: $LOGS"
    exit 1
  fi
}

step "Phase E sort and filter"   bash verify/verify_p15_excel_phase_e_sort_filter.sh
step "Spreadsheet Create"        bash verify/verify_p15_spreadsheet_create.sh
step "Spreadsheet Read"          bash verify/verify_p15_spreadsheet_read.sh
step "Full Rust tests"           cargo test --manifest-path src-tauri/Cargo.toml
step "Frontend build"            npm run build
step "Whitespace"                git diff --check

echo
echo "PASS Excel phase gate"
echo "Logs: $LOGS"
