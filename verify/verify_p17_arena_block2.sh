#!/usr/bin/env bash

PASS=0
FAIL=0

check() {
  local name="$1"
  shift

  if "$@"; then
    echo "PASS — $name"
    PASS=$((PASS + 1))
  else
    echo "FAIL — $name"
    FAIL=$((FAIL + 1))
  fi
}

check "shared Arena Runtime exists" \
  grep -q 'export class ArenaRuntime' \
  src/services/arenaRuntime.ts

check "single shared strategy driver interface" \
  grep -q 'export function buildArenaStrategyDriver' \
  src/services/arenaStrategyDrivers.ts

for strategy in \
  compare \
  debate \
  collaboration \
  competition \
  role-play \
  game \
  simulation
do
  check "strategy driver: $strategy" \
    grep -q "\"$strategy\"" \
    src/services/arenaStrategyDrivers.ts
done

check "public context projection" \
  grep -q 'case "public"' \
  src/services/arenaContextStore.ts

check "participant-private projection" \
  grep -q 'case "participant-private"' \
  src/services/arenaContextStore.ts

check "team-private projection" \
  grep -q 'case "team-private"' \
  src/services/arenaContextStore.ts

check "runtime-private isolation" \
  grep -q 'case "runtime-private"' \
  src/services/arenaContextStore.ts

check "pause support" \
  grep -q 'pause(): void' \
  src/services/arenaRuntime.ts

check "resume support" \
  grep -q 'resume(): void' \
  src/services/arenaRuntime.ts

check "stop support" \
  grep -q 'async stop()' \
  src/services/arenaRuntime.ts

check "cancel support" \
  grep -q 'async cancel()' \
  src/services/arenaRuntime.ts

check "runtime events" \
  grep -q 'ArenaRuntimeEvent' \
  src/services/arenaRuntime.ts

check "latency metrics" \
  grep -q 'durationMs' \
  src/services/arenaRuntime.ts

check "seed hook" \
  grep -q 'config.seed' \
  src/services/arenaRuntime.ts

check "AI Center participant invocation" \
  grep -q 'streamThroughAiCenter' \
  src/services/arenaRuntime.ts

check "no Task Engine import" \
  bash -c '! rg -n "^import .*task|from .*task" src/services/arenaRuntime.ts src/services/arenaStrategyDrivers.ts'

check "no OpenClaw import" \
  bash -c '! rg -n "^import .*openclaw|from .*openclaw" src/services/arenaRuntime.ts src/services/arenaStrategyDrivers.ts'

check "no Skill execution import" \
  bash -c '! rg -n "^import .*skill|from .*skill" src/services/arenaRuntime.ts src/services/arenaStrategyDrivers.ts'

echo
echo "P17 BLOCK 2 VERIFIER: PASS=$PASS FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
