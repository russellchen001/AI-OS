#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.." || exit 1

NAME="Beta 4.1 selective public fact grounding"
TMP_DIR="$(mktemp -d)"
trap 'find "$TMP_DIR" -depth -delete 2>/dev/null || true' EXIT

fail() {
  echo "❌ $1"
  if [ -f "$2" ]; then tail -30 "$2"; fi
  echo "FAIL: $NAME — $1"
  exit 1
}

if node --experimental-strip-types verify/beta41_public_fact_grounding.behavior.mjs >"$TMP_DIR/behavior.log" 2>&1; then
  echo "✓ Product facts route selectively; ordinary chat stays local"
else
  fail "Behavior verification failed" "$TMP_DIR/behavior.log"
fi

if npm run build >"$TMP_DIR/build.log" 2>&1; then
  echo "✓ Frontend production build"
else
  fail "Frontend production build failed" "$TMP_DIR/build.log"
fi

echo "PASS: $NAME"
exit 0
