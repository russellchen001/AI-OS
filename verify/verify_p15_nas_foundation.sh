#!/bin/zsh

cd "$(dirname "$0")/.." || exit 1

echo "=== P15 NAS FOUNDATION VERIFY ==="

FAIL=0

echo
echo "--- production hardcoding guard ---"

PRODUCTION_PATHS=(
  src-tauri/src/nas
  src-tauri/src/runtime/agent_skill_transport.rs
  src-tauri/src/runtime/skills/registry.rs
  src/types/skill.ts
)

FORBIDDEN_PATTERNS=(
  'RussellNAS'
  '/Volumes/Movie'
  '192\.168\.0\.226'
  'DS925\+'
  'WD161KFGX'
)

for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
  if rg -n "$pattern" "${PRODUCTION_PATHS[@]}" >/tmp/ai-os-nas-hardcode-hit.txt 2>/dev/null; then
    echo "FAIL: production NAS code contains forbidden hardware-specific value: $pattern"
    cat /tmp/ai-os-nas-hardcode-hit.txt
    FAIL=1
  fi
done

if [ "$FAIL" -eq 0 ]; then
  echo "PASS: no owner-hardware values found in production NAS implementation"
fi

echo
echo "--- production capability selector guard ---"

CONTRACT_PRODUCTION_TMP="/tmp/ai-os-nas-contract-production.txt"

awk '
  /#\[cfg\(test\)\]/ { exit }
  { print }
' src-tauri/src/runtime/skills/contracts.rs > "$CONTRACT_PRODUCTION_TMP"

if rg -n '"(device|nas|hostname|my_nas)"' "$CONTRACT_PRODUCTION_TMP"; then
  echo "FAIL: hardware-specific or invented NAS selector found in production contract"
  FAIL=1
else
  echo "PASS: production NAS selector contract is hardware-neutral"
fi

echo
echo "--- capability contract ---"

for capability in \
  nas.discover \
  nas.list \
  nas.status \
  nas.resolve \
  nas.capacity
do
  if rg -q "\"$capability\"" src-tauri/src/runtime/skills/registry.rs; then
    echo "PASS: $capability registered"
  else
    echo "FAIL: $capability missing from registry"
    FAIL=1
  fi
done

echo
echo "--- file CRUD separation ---"

# Only production declarations count here. The NAS unit tests intentionally
# contain strings such as nas.read/nas.write to prove those capabilities are
# forbidden, so scanning the test section would create a false positive.
NAS_PRODUCTION_TMP="/tmp/ai-os-nas-production-contract.txt"

{
  awk '
    /#\[cfg\(test\)\]/ { exit }
    { print }
  ' src-tauri/src/nas/mod.rs

  cat src-tauri/src/runtime/skills/registry.rs
  cat src/types/skill.ts
} > "$NAS_PRODUCTION_TMP"

if rg -q '"nas\.(read|write|copy|move|delete|mkdir)"' "$NAS_PRODUCTION_TMP"; then
  echo "FAIL: NAS duplicated filesystem CRUD in production contract"
  rg -n '"nas\.(read|write|copy|move|delete|mkdir)"' "$NAS_PRODUCTION_TMP" || true
  FAIL=1
else
  echo "PASS: NAS does not duplicate filesystem CRUD"
fi

echo
echo "--- targeted tests ---"

(
  cd src-tauri
  cargo test nas --lib
)
TEST_RC=$?

if [ "$TEST_RC" -ne 0 ]; then
  echo "FAIL: NAS unit tests"
  FAIL=1
else
  echo "PASS: NAS unit tests"
fi

echo
echo "--- cargo check ---"

(
  cd src-tauri
  cargo check
)
CHECK_RC=$?

if [ "$CHECK_RC" -ne 0 ]; then
  echo "FAIL: cargo check"
  FAIL=1
else
  echo "PASS: cargo check"
fi

echo
echo "--- diff check ---"

git diff --check
DIFF_RC=$?

if [ "$DIFF_RC" -ne 0 ]; then
  echo "FAIL: git diff --check"
  FAIL=1
else
  echo "PASS: git diff --check"
fi

echo
echo "=== RESULT ==="

if [ "$FAIL" -eq 0 ]; then
  echo "P15_NAS_FOUNDATION=PASS"
else
  echo "P15_NAS_FOUNDATION=FAIL"
fi

exit "$FAIL"
