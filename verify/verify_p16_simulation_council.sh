#!/usr/bin/env bash

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

PASS=0
FAIL=0

pass() {
  PASS=$((PASS + 1))
  printf 'PASS: %s\n' "$1"
}

fail() {
  FAIL=$((FAIL + 1))
  printf 'FAIL: %s\n' "$1"
}

if npx tsx verify/p16_simulation_council.behavior.ts; then
  pass "Simulation Council behavior"
else
  fail "Simulation Council behavior"
fi

if grep -q '"simulation" | "legacy-fallback"' src/types/councilAssembly.ts; then
  pass "Simulation is a first-class assembly mode"
else
  fail "Simulation assembly mode missing"
fi

if grep -q 'distilledPersonaToCouncilContext' src/services/councilSimulation.ts; then
  pass "P15 distilled persona adapter reused"
else
  fail "P15 distilled persona adapter not reused"
fi

if grep -q 'buildPersonPersonaSkill' src/services/councilSimulation.ts; then
  pass "Existing P15 persona skill reused"
else
  fail "P15 persona skill not reused"
fi

if grep -q 'humanReviewed: true' src/services/councilSimulation.ts; then
  pass "Human-reviewed boundary retained"
else
  fail "Human-reviewed boundary missing"
fi

if grep -q 'evidenceThatWouldChangeForecast' src/services/councilRuntime.ts; then
  pass "Forecast report preserves falsifiability"
else
  fail "Forecast report structure missing"
fi

if grep -q 'request.assemblyPlan.mode !== "legacy-fallback"' src/services/councilRuntime.ts; then
  pass "Simulation uses existing dynamic runtime"
else
  fail "Simulation dynamic runtime dispatch missing"
fi

printf '\nPASS=%s FAIL=%s\n' "$PASS" "$FAIL"
test "$FAIL" -eq 0
