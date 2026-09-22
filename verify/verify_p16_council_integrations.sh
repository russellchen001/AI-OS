#!/usr/bin/env bash

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
FAIL=0

check() {
  NAME="$1"
  shift

  if "$@"; then
    echo "PASS: $NAME"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $NAME"
    FAIL=$((FAIL + 1))
  fi
}

check \
  "Paperclip adapter exists" \
  test -f src/services/integrations/paperclipAdapter.ts

check \
  "Agency Agents adapter exists" \
  test -f src/services/integrations/agencyAgentsAdapter.ts

check \
  "Linco Bridge adapter exists" \
  test -f src/services/integrations/lincoBridgeAdapter.ts

check \
  "Council integration registry exists" \
  test -f src/services/councilIntegrations.ts

check \
  "Council integration types exist" \
  test -f src/types/councilIntegrations.ts

check \
  "Paperclip source pinned" \
  grep -q \
    'b19307758285e22ef3386cac56031f36ed39815d' \
    src/services/integrations/paperclipAdapter.ts

check \
  "Agency Agents source pinned" \
  grep -q \
    'ad9264e309bd5e5422c04784372d7841b1e5d604' \
    src/services/integrations/agencyAgentsAdapter.ts

check \
  "Linco Bridge source pinned" \
  grep -q \
    '3a7375858eb17de3db45cd9896578d371c6df9be' \
    src/services/integrations/lincoBridgeAdapter.ts

check \
  "Paperclip uses real API health endpoint" \
  grep -q \
    '/api/health' \
    src/services/integrations/paperclipAdapter.ts

check \
  "Paperclip exposes companies integration" \
  grep -q \
    '/api/companies' \
    src/services/integrations/paperclipAdapter.ts

check \
  "Agency Agents loads real divisions catalog" \
  grep -q \
    'divisions.json' \
    src/services/integrations/agencyAgentsAdapter.ts

check \
  "Agency Agents loads real runbooks" \
  grep -q \
    'strategy/runbooks.json' \
    src/services/integrations/agencyAgentsAdapter.ts

check \
  "Linco Bridge uses real demo-config endpoint" \
  grep -q \
    '/api/demo-config' \
    src/services/integrations/lincoBridgeAdapter.ts

check \
  "Linco Bridge exposes OpenClaw status" \
  grep -q \
    '/api/agent-bridges/openclaw/status' \
    src/services/integrations/lincoBridgeAdapter.ts

check \
  "No user NAS fixture leaked into integrations" \
  bash -c \
    '! rg -n "RussellNAS|/Volumes/Movie|russellchen" src/services/integrations src/services/councilIntegrations.ts src/types/councilIntegrations.ts'

echo
echo "PASS=$PASS"
echo "FAIL=$FAIL"

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
