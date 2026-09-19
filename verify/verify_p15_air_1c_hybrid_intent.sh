#!/bin/zsh

cd "$(dirname "$0")/.." || exit 1

FAIL=0

echo "=== AIR-1C HYBRID INTENT VERIFY ==="

echo
echo "--- hybrid architecture ---"

if rg -q \
  'export function classifySemanticIntent' \
  src/services/intentSemantic.ts &&
   rg -q \
  'source: "semantic-rule"' \
  src/services/intentRouting.ts
then
  echo "PASS: semantic routing layer exists"
else
  echo "FAIL: semantic routing layer missing"
  FAIL=1
fi

if rg -q \
  'answerThroughAiCenter' \
  src/services/intentRouting.ts &&
   rg -q \
  'source: "safe-fallback"' \
  src/services/intentRouting.ts
then
  echo "PASS: AI fallback and safe fallback remain available"
else
  echo "FAIL: fallback routing path missing"
  FAIL=1
fi

echo
echo "--- capability-neutral guard ---"

if rg -n \
  '"filesystem\.|"nas\.|"email\.|"calendar\.|"download\.start|"models\.|"media\.' \
  src/services/intentRouting.ts \
  src/services/intentSemantic.ts
then
  echo "FAIL: router contains concrete Skill capability"
  FAIL=1
else
  echo "PASS: router remains capability-neutral"
fi

echo
echo "--- canonical behavior cases ---"

node --experimental-strip-types \
  verify/.air_intent_semantic_test.ts \
  2>&1 | tee /tmp/ai-os-air-intent-test.log

TEST_RC=$pipestatus[1]

if [ "$TEST_RC" -eq 0 ]; then
  echo "PASS: canonical ASK/DO behavior"
else
  echo "FAIL: canonical ASK/DO behavior"
  FAIL=1
fi

echo
echo "--- frontend build ---"

npm run build \
  2>&1 | tee /tmp/ai-os-air-1c-build.log

BUILD_RC=$pipestatus[1]

if [ "$BUILD_RC" -eq 0 ]; then
  echo "PASS: frontend build"
else
  echo "FAIL: frontend build"
  FAIL=1
fi

echo
echo "--- existing AIR checks ---"

if [ -x verify/verify_p15_automatic_intent_routing.sh ]; then
  ./verify/verify_p15_automatic_intent_routing.sh \
    2>&1 | tee /tmp/ai-os-air-1c-existing-air.log

  EXISTING_AIR_RC=$pipestatus[1]

  if [ "$EXISTING_AIR_RC" -eq 0 ]; then
    echo "PASS: existing AIR verification"
  else
    echo "FAIL: existing AIR verification"
    FAIL=1
  fi
else
  echo "FAIL: existing AIR verifier missing"
  EXISTING_AIR_RC=1
  FAIL=1
fi

echo
echo "--- composer interaction regression ---"

if [ -x verify/verify_p15_air_1b_composer.sh ]; then
  ./verify/verify_p15_air_1b_composer.sh \
    2>&1 | tee /tmp/ai-os-air-1c-composer.log

  COMPOSER_RC=$pipestatus[1]

  if [ "$COMPOSER_RC" -eq 0 ]; then
    echo "PASS: AIR-1B composer behavior preserved"
  else
    echo "FAIL: AIR-1B composer regression"
    FAIL=1
  fi
else
  echo "FAIL: AIR-1B verifier missing"
  COMPOSER_RC=1
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
  echo "P15_AIR_1C_HYBRID_INTENT=PASS"
else
  echo "P15_AIR_1C_HYBRID_INTENT=FAIL"
fi

exit "$FAIL"
