#!/bin/zsh

cd "$(dirname "$0")/.." || exit 1

FAIL=0

echo "=== AIR-1B COMPOSER INTERACTION VERIFY ==="

echo
echo "--- attachment picker ---"

if rg -q 'function openAttachmentPicker' src/pages/ChatPage.tsx &&
   rg -q 'showPicker' src/pages/ChatPage.tsx &&
   rg -q 'onClick=\{openAttachmentPicker\}' src/pages/ChatPage.tsx
then
  echo "PASS: attachment picker uses direct user gesture with fallback"
else
  echo "FAIL: attachment picker repair missing"
  FAIL=1
fi

echo
echo "--- Enter / IME handling ---"

if rg -q 'nativeEvent\.isComposing' src/pages/ChatPage.tsx &&
   rg -q 'requestSubmit\(\)' src/pages/ChatPage.tsx
then
  echo "PASS: Enter submit handles IME composition"
else
  echo "FAIL: Enter/IME handling missing"
  FAIL=1
fi

echo
echo "--- immediate submit feedback ---"

python3 <<'PY'
from pathlib import Path

text = Path("src/pages/ChatPage.tsx").read_text()

submit = text.index("async function submit")
classifier = text.index("classifyAutomaticIntent", submit)
busy = text.index("setIsSubmitting(true)", submit)

if busy < classifier:
    print("PASS: isSubmitting is set before intent classification")
else:
    print("FAIL: classification still starts before submit feedback")
    raise SystemExit(1)
PY

if [ "$?" -ne 0 ]; then
  FAIL=1
fi

echo
echo "--- classifier timeout ---"

if rg -q 'INTENT_ROUTER_TIMEOUT_MS = 8000' \
  src/services/intentRouting.ts &&
   rg -q 'withIntentRouterTimeout' \
  src/services/intentRouting.ts
then
  echo "PASS: classifier has bounded wait"
else
  echo "FAIL: classifier timeout missing"
  FAIL=1
fi

echo
echo "--- no capability routing regression ---"

if rg -n \
  'filesystem\.|download\.start|models\.(list|show|pull|delete)|nas\.|email\.|calendar\.|media\.' \
  src/services/intentRouting.ts
then
  echo "FAIL: intent router hard-codes Skill capability"
  FAIL=1
else
  echo "PASS: intent router remains Skill-neutral"
fi

echo
echo "--- frontend build ---"

npm run build
BUILD_RC=$?

if [ "$BUILD_RC" -eq 0 ]; then
  echo "PASS: frontend build"
else
  echo "FAIL: frontend build"
  FAIL=1
fi

echo
echo "--- diff check ---"

git diff --check
DIFF_RC=$?

if [ "$DIFF_RC" -eq 0 ]; then
  echo "PASS: git diff --check"
else
  echo "FAIL: git diff --check"
  FAIL=1
fi

echo
echo "=== RESULT ==="

if [ "$FAIL" -eq 0 ]; then
  echo "P15_AIR_1B_COMPOSER=PASS"
else
  echo "P15_AIR_1B_COMPOSER=FAIL"
fi

exit "$FAIL"
