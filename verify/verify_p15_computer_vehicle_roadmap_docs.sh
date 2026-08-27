#!/bin/bash
set -u

NAME="P15 Computer Vehicle Roadmap Docs"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

fail() {
  echo "✗ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

for file in AI_OS_MASTER_GUIDE.md HANDOFF.md; do
  if test -s "$file"; then
    echo "✓ $file exists and is non-empty"
  else
    fail "$file missing or empty"
  fi
done

if git diff --check -- AI_OS_MASTER_GUIDE.md HANDOFF.md; then
  echo "✓ Markdown diff integrity"
else
  fail "Markdown diff integrity"
fi

echo "✓ Content decision requires manual review"
echo "PASS $NAME"
exit 0
