#!/bin/bash

set -e

echo "Checking P13 AI Center migration..."

PASS=0
FAIL=0

check() {
  if eval "$2"; then
    echo "✓ $1"
    PASS=$((PASS+1))
  else
    echo "✗ $1"
    FAIL=$((FAIL+1))
  fi
}

check "MultiLLM service removed" \
"test ! -f src/services/multillm.ts"

check "AiCouncil no longer uses MultiLLM" \
"! rg -n 'MultiLlm|MultiLLM|multillm' src/pages/AiCouncilPage.tsx"

check "Provider Registry exists" \
"test -f src/services/providers.ts"

check "Council uses ProviderId" \
"rg -n 'ProviderId' src/pages/AiCouncilPage.tsx src/types/council.ts"

check "Artifact no longer uses MultiLLM" \
"! rg -n 'MultiLLM' src/types/artifact.ts src/services/artifacts.ts src/components/MarkdownRenderer.tsx"

if [ $FAIL -eq 0 ]; then
  echo ""
  echo "PASS P13 AI Center migration"
  exit 0
else
  echo ""
  echo "FAIL P13 AI Center migration"
  exit 1
fi
