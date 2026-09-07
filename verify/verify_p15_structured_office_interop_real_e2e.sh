#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

AI_OS_RUN_STRUCTURED_OFFICE_REAL_E2E=1 \
  exec bash verify/verify_p15_structured_office_files.sh
