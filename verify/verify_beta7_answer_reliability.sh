#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
FEATURE=beta7_answer_reliability
LOG=$(mktemp)
trap 'rm -f "$LOG"' EXIT
if cargo test --manifest-path src-tauri/Cargo.toml --lib answer_reliability --quiet >"$LOG" 2>&1; then
  echo '✅ Evidence admission, contradictions, versions, unavailable sources, confident errors and low-risk triage'
  rg '^test result:' "$LOG"
else
  tail -50 "$LOG"
  echo "FAIL: $FEATURE — behavior tests"
  exit 1
fi
if npm run build >"$LOG" 2>&1; then
  echo '✅ Frontend production build'
else
  tail -25 "$LOG"
  echo "FAIL: $FEATURE — frontend build"
  exit 1
fi
echo "PASS: $FEATURE"
echo 'MANUAL: Check guarded advice and ordinary chat in the independent Beta 7 candidate. Fixture evidence does not establish real source availability.'
