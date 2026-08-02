#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
checks=0
grep -Fq 'setName(nextKind === "hermes-api" ? "Hermes Agent" : "Custom Agent")' src/pages/AgentsPage.tsx && echo "✅ Custom Agent naming" && checks=$((checks+1))
grep -Fq 'deleteCustomAgent(agent.id)' src/pages/AgentsPage.tsx && echo "✅ Delete button action" && checks=$((checks+1))
grep -Fq 'export function deleteCustomAgent' src/services/agentRegistry.ts && echo "✅ Registry deletion" && checks=$((checks+1))
npm run build && echo "✅ Frontend build" && checks=$((checks+1))
[ "$checks" -eq 4 ] || { echo "FAIL: agent registry fix"; exit 1; }
echo "PASS: agent registry fix"
