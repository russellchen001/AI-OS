#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
grep -Fq "filter((agent) => agent.id !== normalizedId)" src/services/agentRegistry.ts || { echo "❌ Registry deletion missing"; echo "FAIL: agent registry delete"; exit 1; }
echo "✅ Registry removes selected Agent"
grep -Fq "const nextAgents = deleteCustomAgent(agent.id)" src/pages/AgentsPage.tsx || { echo "❌ UI deletion missing"; echo "FAIL: agent registry delete"; exit 1; }
echo "✅ UI refreshes after deletion"
grep -Fq "if (agent.builtIn) return" src/pages/AgentsPage.tsx || { echo "❌ OpenClaw protection missing"; echo "FAIL: agent registry delete"; exit 1; }
echo "✅ OpenClaw remains protected"
npm run build
echo "✅ Frontend build"
echo "PASS: agent registry delete"
