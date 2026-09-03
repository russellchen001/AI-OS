#!/bin/bash
# Every verifier and probe in this directory must be executable.
#
# `sed -i` on macOS rewrites a file rather than editing it in place, so it
# replaces the mode with the default and silently drops +x. That has cost three
# scripts their executable bit during this work, each time discovered by hand
# afterwards. This turns it into a failed gate step instead.
set -u

NAME="Verifier file modes"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT" || exit 1

echo "$NAME"

broken=""
for script in verify/*.sh verify/*.command; do
  [ -e "$script" ] || continue
  [ -x "$script" ] || broken="$broken $script"
done

if [ -n "$broken" ]; then
  echo "❌ not executable:$broken"
  echo "   restore with: chmod +x$broken"
  echo "FAIL $NAME: a verifier lost its executable bit"
  exit 1
fi

echo "✅ every verifier and probe in verify/ is executable"
echo "PASS $NAME"
