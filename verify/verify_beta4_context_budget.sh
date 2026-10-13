#!/bin/zsh

cd "$(git rev-parse --show-toplevel)" || exit 1

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

run_check() {
  local name="$1"
  shift
  if "$@"; then
    print "✓ $name"
  else
    print "✗ $name"
    print "FAIL Beta.4 context budget: $name"
    exit 1
  fi
}

run_check "behavior verifier builds" \
  node_modules/.bin/esbuild verify/beta4_context_budget.behavior.ts \
    --bundle --platform=node --format=esm \
    --outfile="$TMP_DIR/beta4-context-budget.mjs"

run_check "model-aware budget and bounded projection behavior" \
  node "$TMP_DIR/beta4-context-budget.mjs"

run_check "frontend production build" npm run build

run_check "changed files have valid whitespace" \
  git diff --check -- \
    src/services/aiCenter.ts \
    src/services/conversations.ts \
    src/pages/ChatPage.tsx \
    HANDOFF.md

print "PASS Beta.4 context budget"
