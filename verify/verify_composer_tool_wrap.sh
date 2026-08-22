#!/bin/bash

set -u

cd "$(dirname "$0")/.." || {
  echo "FAIL Composer Tool Wrap: cannot enter project"
  exit 1
}

echo "---- Frontend build ----"

if npm run build >/tmp/ai-os-composer-build.log 2>&1; then
  echo "✓ Frontend builds"
else
  echo "✗ Frontend build failed"
  tail -20 /tmp/ai-os-composer-build.log
  echo "FAIL Composer Tool Wrap: frontend build"
  exit 1
fi

echo "PASS: composer tool wrap"
exit 0
