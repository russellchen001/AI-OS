#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
node verify/verify_beta7_format_scope.mjs
