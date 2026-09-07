#!/usr/bin/env bash

VERIFY_DIR="$(cd "$(dirname "$0")" && pwd)/verify"
CORE_PASS=0
CORE_FAIL=0
CORE_TOTAL=0
EXTERNAL_FAIL=0

if [ ! -d "$VERIFY_DIR" ]; then
  echo "FAIL verify_all: verify directory is missing"
  exit 1
fi

is_external() {
  case "$(basename "$1")" in
    verify_p15_microsoft_graph_real_e2e.sh|\
    verify_p15_google_workspace_real_e2e.sh|\
    verify_p15_wps_real_e2e.sh|\
    verify_p15_iwork_real_e2e.sh|\
    verify_p15_iwork_pages_real_e2e.sh|\
    verify_p15_iwork_numbers_real_e2e.sh|\
    verify_p15_powerpoint_real_e2e.sh|\
    verify_p15_office_conversion_real_e2e.sh|\
    verify_p15_commerce_provider_real_e2e.sh|\
    verify_p15_browser_authenticated_session.sh|\
    verify_p15_browser_authenticated_session_real_e2e.sh|\
    verify_p15_presentation_real_e2e.sh|\
    verify_p15_structured_office_interop_real_e2e.sh|\
    verify_p15_word_real_e2e.sh)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

echo "Core Suite:"

for script in "$VERIFY_DIR"/verify_*.sh; do
  is_external "$script" && continue

  CORE_TOTAL=$((CORE_TOTAL + 1))

  OUTPUT=$(bash "$script" 2>&1)
  CODE=$?

  if [ "$CODE" -eq 0 ]; then
    CORE_PASS=$((CORE_PASS + 1))
    echo "  PASS $(basename "$script" .sh)"
  else
    CORE_FAIL=$((CORE_FAIL + 1))
    echo "  FAIL $(basename "$script" .sh)"
    echo "$OUTPUT" | sed 's/^/     /'
  fi
done

echo "Core Suite: PASS $CORE_PASS/$CORE_TOTAL"

echo "External E2E:"

run_external() {
  LABEL=$1
  SCRIPT=$2

  if [ ! -x "$SCRIPT" ]; then
    echo "  $LABEL SKIP — verification script unavailable"
    return
  fi

  OUTPUT=$(bash "$SCRIPT" 2>&1)
  CODE=$?

  if [ "$CODE" -ne 0 ]; then
    echo "  $LABEL FAIL"
    echo "$OUTPUT" | sed 's/^/    /'
    EXTERNAL_FAIL=$((EXTERNAL_FAIL + 1))
  elif echo "$OUTPUT" | grep -q "SKIP"; then
    REASON=$(echo "$OUTPUT" | grep "SKIP" | tail -1)
    echo "  $LABEL SKIP — $REASON"
  else
    echo "  $LABEL PASS"
  fi
}

run_external "Microsoft" "$VERIFY_DIR/verify_p15_microsoft_graph_real_e2e.sh"
run_external "Google" "$VERIFY_DIR/verify_p15_google_workspace_real_e2e.sh"
run_external "WPS" "$VERIFY_DIR/verify_p15_wps_real_e2e.sh"

run_external "iWork Availability" "$VERIFY_DIR/verify_p15_iwork_real_e2e.sh"
run_external "Pages" "$VERIFY_DIR/verify_p15_iwork_pages_real_e2e.sh"
run_external "Numbers" "$VERIFY_DIR/verify_p15_iwork_numbers_real_e2e.sh"
run_external "Presentation" "$VERIFY_DIR/verify_p15_presentation_real_e2e.sh"
run_external "PowerPoint" "$VERIFY_DIR/verify_p15_powerpoint_real_e2e.sh"
run_external "Office Conversion" "$VERIFY_DIR/verify_p15_office_conversion_real_e2e.sh"
run_external "Office Interop" "$VERIFY_DIR/verify_p15_structured_office_interop_real_e2e.sh"
run_external "Word" "$VERIFY_DIR/verify_p15_word_real_e2e.sh"

run_external "eBay" "$VERIFY_DIR/verify_p15_commerce_provider_real_e2e.sh"
run_external "Amazon" "$VERIFY_DIR/verify_p15_browser_authenticated_session_real_e2e.sh"
run_external "Taobao" "$VERIFY_DIR/verify_p15_browser_authenticated_session_real_e2e.sh"
run_external "JD" "$VERIFY_DIR/verify_p15_browser_authenticated_session_real_e2e.sh"
run_external "Pinduoduo" "$VERIFY_DIR/verify_p15_browser_authenticated_session_real_e2e.sh"

if [ "$CORE_FAIL" -eq 0 ] && [ "$EXTERNAL_FAIL" -eq 0 ]; then
  echo "PASS verify_all: core passed; external PASS/SKIP reported separately"
  exit 0
fi

echo "FAIL verify_all: core failures=$CORE_FAIL external failures=$EXTERNAL_FAIL"
exit 1
