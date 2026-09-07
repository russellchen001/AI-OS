#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

AI_OS_RUN_WORD_REAL_E2E=1 \
  exec bash verify/verify_p15_word_common_capability.sh
