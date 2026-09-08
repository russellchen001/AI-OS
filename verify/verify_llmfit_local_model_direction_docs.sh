#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "FAIL $1"
  exit 1
}

pass() {
  echo "PASS $1"
}

echo "LLMFIT LOCAL MODEL DIRECTION DOCS"

grep -Fq '### Local Model Optimization — llmfit primary' HANDOFF.md \
  || fail "HANDOFF llmfit primary decision missing"

grep -Fq '`AlexsJones/llmfit` is the preferred v1.0 reusable foundation' HANDOFF.md \
  || fail "HANDOFF llmfit role missing"

grep -Fq 'oMLX and Ollama remain the current v1.0 Local Model execution Providers.' HANDOFF.md \
  || fail "HANDOFF execution-provider boundary missing"

grep -Fq 'Magnitude is superseded as the planned v1.0 Local Model optimization' HANDOFF.md \
  || fail "HANDOFF Magnitude supersession missing"

grep -Fq '2. llmfit Local Model Optimization / Recommendation integration' HANDOFF.md \
  || fail "HANDOFF post-GM sequence does not contain llmfit"

if grep -Fq '2. Magnitude' HANDOFF.md; then
  fail "HANDOFF still schedules Magnitude as active post-GM integration"
fi

grep -Fq '1. GM-5 — Prompt / Reference Intelligence' HANDOFF.md \
  || fail "HANDOFF current GM next milestone is stale"

grep -Fq '## 2026-09-09 — Local Model Optimization — llmfit Primary' HANDOFF.md \
  || fail "HANDOFF decision record missing"

pass "HANDOFF current-state direction"

grep -Fq '#### Local Model Optimization — llmfit' AI_OS_MASTER_GUIDE.md \
  || fail "MASTER GUIDE llmfit architecture section missing"

grep -Fq -- '- `AlexsJones/llmfit`' AI_OS_MASTER_GUIDE.md \
  || fail "MASTER GUIDE llmfit component reference missing"

grep -Fq 'oMLX / Ollama / future replaceable Local Model Providers' AI_OS_MASTER_GUIDE.md \
  || fail "MASTER GUIDE execution architecture missing"

grep -Fq 'Magnitude is not a default planned v1.0 integration.' AI_OS_MASTER_GUIDE.md \
  || fail "MASTER GUIDE Magnitude future-only boundary missing"

grep -Fq '## 2026-09-09 — llmfit Local Model Optimization Direction' AI_OS_MASTER_GUIDE.md \
  || fail "MASTER GUIDE change log missing"

pass "MASTER GUIDE architecture direction"

git diff --check \
  || fail "git diff --check"

pass "git diff --check"

echo "PASS LLMFIT LOCAL MODEL DIRECTION DOCS"
