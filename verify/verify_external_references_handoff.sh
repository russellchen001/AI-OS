#!/bin/bash

set -u

PASS=true
HANDOFF="HANDOFF.md"

check_section() {
  local heading="$1"

  if python3 - "$HANDOFF" "$heading" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
heading = sys.argv[2]
text = path.read_text()

raise SystemExit(0 if heading in text else 1)
PY
  then
    echo "✓ $heading 已记录"
  else
    echo "✗ 缺少 $heading"
    PASS=false
  fi
}

check_repo() {
  local repo="$1"
  local expected="$2"
  local actual

  if actual="$(git -C "$repo" rev-parse --short HEAD 2>/dev/null)" && [ "$actual" = "$expected" ]; then
    echo "✓ $(basename "$repo") reference commit = $actual"
  else
    echo "✗ $(basename "$repo") reference commit 不符合预期"
    PASS=false
  fi
}

check_section "### Agency Agents Skill Reference"
check_section "### Paperclip Reference"
check_section "### Local external reference repositories"

check_repo "../agency-agents" "ebe9c99"
check_repo "../paperclip" "f0e6c0f54"

if [ "$PASS" = true ]; then
  echo "PASS external references handoff"
  exit 0
fi

echo "FAIL external references handoff: HANDOFF 或本地参考仓库状态不正确"
exit 1
