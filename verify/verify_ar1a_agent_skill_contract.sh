#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PASS=0
FAIL=0

pass() {
  PASS=$((PASS + 1))
  echo "PASS $1"
}

fail() {
  FAIL=$((FAIL + 1))
  echo "FAIL $1"
}

echo "============================================================"
echo "VERIFY AR-1A — AGENT / SKILL CONTRACT"
echo "============================================================"

echo
echo "=== source contract ==="

if grep -q 'pub(crate) trait AgentExecutionAdapter' \
  src-tauri/src/runtime/agent_execution.rs; then
  pass "generic AgentExecutionAdapter exists"
else
  fail "generic AgentExecutionAdapter missing"
fi

if grep -q 'pub(crate) trait SkillInvocationGateway' \
  src-tauri/src/runtime/skill_invocation.rs; then
  pass "SkillInvocationGateway exists"
else
  fail "SkillInvocationGateway missing"
fi

if grep -q 'pub(crate) trait SkillBackend' \
  src-tauri/src/runtime/skill_invocation.rs; then
  pass "SkillBackend exists"
else
  fail "SkillBackend missing"
fi

if grep -q 'pub(crate) agent_id: AgentId' \
  src-tauri/src/runtime/agent_execution.rs; then
  pass "selected Agent identity is part of execution request"
else
  fail "Agent identity missing from execution request"
fi

if grep -q 'pub(crate) allowed_capabilities: Vec<String>' \
  src-tauri/src/runtime/agent_execution.rs; then
  pass "Agent execution carries capability constraints"
else
  fail "allowed capability constraints missing"
fi

if grep -q 'pub(crate) user_confirmed: bool' \
  src-tauri/src/runtime/skill_invocation.rs &&
   ! grep -A8 'pub(crate) struct SkillInvocationRequest' \
      src-tauri/src/runtime/skill_invocation.rs |
      grep -q 'user_confirmed'; then
  pass "confirmation belongs to Runtime context, not Agent request"
else
  fail "confirmation ownership boundary is wrong"
fi

echo
echo "=== contract tests ==="

if (
  cd src-tauri
  cargo test runtime::agent_execution::tests --lib
); then
  pass "Agent execution contract tests"
else
  fail "Agent execution contract tests"
fi

if (
  cd src-tauri
  cargo test runtime::skill_invocation::tests --lib
); then
  pass "Skill invocation contract tests"
else
  fail "Skill invocation contract tests"
fi

echo
echo "=== existing compatibility regression ==="

if (
  cd src-tauri
  cargo test runtime::plan_runtime_bridge::tests --lib
); then
  pass "existing PlanRuntime compatibility bridge remains green"
else
  fail "existing PlanRuntime compatibility bridge regressed"
fi

echo
echo "=== targeted formatting ==="

if rustfmt --check --edition 2021 --config skip_children=true \
  src-tauri/src/runtime/agent_execution.rs \
  src-tauri/src/runtime/skill_invocation.rs \
  src-tauri/src/runtime/mod.rs; then
  pass "targeted rustfmt"
else
  fail "targeted rustfmt"
fi

echo
echo "=== repository integrity ==="

if git diff --check; then
  pass "git diff --check"
else
  fail "git diff --check"
fi

echo
echo "============================================================"
echo "AR-1A RESULT"
echo "PASS=$PASS"
echo "FAIL=$FAIL"
echo "============================================================"

if [ "$FAIL" -ne 0 ]; then
  echo "FAIL AR-1A AGENT / SKILL CONTRACT"
  exit 1
fi

echo "PASS AR-1A AGENT / SKILL CONTRACT"
