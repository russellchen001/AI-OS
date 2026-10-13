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
    print "FAIL Beta.4 knowledge engine: $name"
    exit 1
  fi
}

run_check "behavior verifier builds" \
  node_modules/.bin/esbuild verify/beta4_knowledge_engine.behavior.ts \
    --bundle --platform=node --format=esm \
    --outfile="$TMP_DIR/beta4-knowledge-engine.mjs"

run_check "history, compaction, provenance, and project isolation behavior" \
  node "$TMP_DIR/beta4-knowledge-engine.mjs"

run_check "knowledge backend unit behavior" \
  cargo test --manifest-path src-tauri/Cargo.toml knowledge::tests --lib --quiet

run_check "frontend production build" npm run build

run_check "Beta.4 files have valid whitespace" \
  git diff --check -- \
    src/services/aiCenter.ts \
    src/services/conversations.ts \
    src/services/knowledge.ts \
    src/pages/ChatPage.tsx \
    src/components/Sidebar.tsx \
    src/App.css \
    src-tauri/src/memory.rs \
    src-tauri/src/knowledge.rs \
    src-tauri/src/lib.rs \
    HANDOFF.md

print "PASS Beta.4 knowledge engine"
