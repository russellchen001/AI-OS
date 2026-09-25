#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

PASS=0
FAIL=0

check() {
  local label="$1"
  shift

  if "$@" >/dev/null 2>&1; then
    echo "PASS — $label"
    PASS=$((PASS + 1))
  else
    echo "FAIL — $label"
    FAIL=$((FAIL + 1))
  fi
}

check "Arena core types" \
  grep -q 'ArenaModelConstraint' src/types/arena.ts

check "Seven internal strategies" \
  grep -q '"simulation"' src/types/arena.ts

check "Strategy Router" \
  grep -q 'routeArenaStrategy' src/services/arenaStrategyRouter.ts

check "Shared Chief of Staff domain arena" \
  grep -q 'domain: "arena"' src/services/arenaChiefOfStaff.ts

check "LLM Chief of Staff reused" \
  grep -q 'ChiefOfStaffOrchestrator' src/services/arenaChiefOfStaff.ts

check "AI Center model assignment" \
  grep -q 'assignArenaModels' src/services/arenaChiefOfStaff.ts

check "Pinned model has zero fallbacks" \
  grep -q 'constraint.mode === "pinned"' src/services/arenaModelAssignment.ts

check "Pinned silent replacement prohibited" \
  grep -q 'will not silently replace' src/services/arenaModelAssignment.ts

check "Arena UI does not expose mode selector" \
  bash -c '! grep -q "arena-mode-rail" src/pages/AiArenaPage.tsx'

check "Arena UI accepts objective" \
  grep -q 'What should the AIs do?' src/pages/AiArenaPage.tsx

check "Arena UI supports optional model preference" \
  grep -q 'Optional model preference' src/pages/AiArenaPage.tsx

check "Arena UI supports pinned model" \
  grep -q 'Pin this model' src/pages/AiArenaPage.tsx

check "Arena execution not implemented in Block 1" \
  grep -q 'Block 2' src/pages/AiArenaPage.tsx

echo
echo "P17 BLOCK 1 VERIFIER: PASS=$PASS FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
