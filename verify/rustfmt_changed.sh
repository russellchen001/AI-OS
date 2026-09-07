#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

EDITION="$(
  awk -F'"' '
    /^[[:space:]]*edition[[:space:]]*=/ {
      print $2
      exit
    }
  ' src-tauri/Cargo.toml
)"

if [[ -z "$EDITION" ]]; then
  EDITION="2021"
fi

FILES="$(
  {
    git diff --name-only --diff-filter=ACMR HEAD -- src-tauri/src
    git ls-files --others --exclude-standard -- src-tauri/src
  } |
  grep '\.rs$' |
  sort -u || true
)"

if [[ -z "$FILES" ]]; then
  echo "PASS Rust formatting: no changed Rust files"
  exit 0
fi

FAILED=0

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  [[ -f "$file" ]] || continue

  if rustfmt \
    --edition "$EDITION" \
    --check \
    --config skip_children=true \
    "$file"
  then
    echo "✓ $file"
  else
    FAILED=1
  fi
done <<< "$FILES"

if [[ "$FAILED" -ne 0 ]]; then
  echo "FAIL Rust formatting: changed Rust files"
  exit 1
fi

echo "PASS Rust formatting: changed Rust files"
