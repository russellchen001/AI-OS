#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
echo "===== HANDOFF ====="
grep -nE "Agent|Hermes|OpenClaw|P13" HANDOFF.md || true
echo "===== AGENT FILES ====="
find src src-tauri -type f | grep -E "agent|Agent" || true
echo "===== CREATION TYPE MAPPING ====="
grep -R -nE "createAgent|addAgent|saveAgent|Custom Agent|Hermes|agentType|agent_type" src src-tauri --exclude-dir=target || true
echo "===== MANAGEMENT DELETION ====="
grep -R -nE "deleteAgent|removeAgent|Manage|setup required|requires its adapter" src src-tauri --exclude-dir=target || true
for f in src/pages/AgentsPage.tsx src/services/agentRegistry.ts src/types/agent.ts; do if [ -f "$f" ]; then echo "===== $f ====="; cat "$f"; fi; done
echo "===== STATUS ====="
git status --short
echo "PASS: agent registry context"
