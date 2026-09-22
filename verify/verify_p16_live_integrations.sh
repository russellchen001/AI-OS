#!/bin/zsh

cd "$(git rev-parse --show-toplevel)" || exit 1

PASS=0
SKIP=0
FAIL=0
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

record_result() {
  local name="$1"
  local rc="$2"
  if [[ "$rc" -eq 0 ]]; then
    print "PASS: $name"
    PASS=$((PASS + 1))
  elif [[ "$rc" -eq 2 ]]; then
    print "SKIP: $name"
    SKIP=$((SKIP + 1))
  else
    print "FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

node_modules/.bin/esbuild \
  verify/p16_live_integrations.behavior.ts \
  --bundle \
  --platform=node \
  --format=esm \
  --outfile="$TMP_DIR/live-integrations.mjs" \
  >/dev/null
BUILD_RC=$?
record_result "Live adapter verifier build" "$BUILD_RC"

if [[ "$BUILD_RC" -eq 0 ]]; then
  node "$TMP_DIR/live-integrations.mjs" agency-agents
  record_result "Agency Agents live pinned content" "$?"

  node "$TMP_DIR/live-integrations.mjs" paperclip
  record_result "Paperclip live endpoints and adapter" "$?"

  node "$TMP_DIR/live-integrations.mjs" linco-bridge
  record_result "Linco Bridge live endpoints, visitor session and adapter" "$?"

  node "$TMP_DIR/live-integrations.mjs" dynamic-assembly
  record_result "Chief of Staff live Paperclip and Agency assembly" "$?"
fi

npm run build
record_result "Frontend production build" "$?"

git diff --check
record_result "Git whitespace validation" "$?"

print
print "PASS=$PASS"
print "SKIP=$SKIP"
print "FAIL=$FAIL"

if [[ "$FAIL" -ne 0 ]]; then
  print "FAIL: P16 live integrations"
  exit 1
fi

if [[ "$SKIP" -ne 0 ]]; then
  print "SKIP: P16 live integrations are not fully live"
  exit 2
fi

print "PASS: P16 live integrations"
