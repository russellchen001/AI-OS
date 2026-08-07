#!/bin/bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "===== CURRENT BRANCH / HEAD ====="
git branch --show-current
git log -1 --oneline

echo
echo "===== CURRENT APP ENTRY / ROUTES ====="
grep -nE 'import .*Page|activePage|setActivePage|<.*Page' src/App.tsx || true

echo
echo "===== ALL UI PAGES ====="
find src/pages -maxdepth 1 -type f -print | sort

echo
echo "===== ALL COMPONENTS ====="
find src/components -maxdepth 1 -type f -print | sort

echo
echo "===== OBVIOUS BACKUP / OLD UI FILES ====="
find src \
  \( -name '*.before-*' \
  -o -name '*.backup' \
  -o -name '*.bak' \
  -o -name '*.old' \
  -o -name '*Old*' \
  -o -name '*Legacy*' \
  \) \
  -print | sort

echo
echo "===== OLD UI KEYWORDS ====="
grep -R -nE \
  'DashboardPage|MultiLlmPage|PromptLibrary|OpenClawPage|legacy|Legacy|old UI|old-ui|Dashboard Header|MultiLLM Hub' \
  src \
  --include='*.tsx' \
  --include='*.ts' \
  --include='*.css' \
  || true

echo
echo "===== PAGE IMPORT REFERENCES ====="
for file in src/pages/*.tsx; do
  name="$(basename "$file" .tsx)"
  count="$(grep -R -l \
    --exclude="$(basename "$file")" \
    --include='*.tsx' \
    --include='*.ts' \
    "$name" src 2>/dev/null | wc -l | tr -d ' ')"
  printf "%-36s references=%s\n" "$name" "$count"
done

echo
echo "===== CSS BACKUPS / DUPLICATES ====="
find src -maxdepth 2 -type f \
  \( -name '*.css' -o -name '*.css.*' \) \
  -print | sort

echo
echo "===== LARGE UI FILES ====="
du -h src/pages/* src/components/* src/*.css 2>/dev/null | sort -hr | head -40

echo
echo "===== GIT STATUS ====="
git status --short

echo
echo "PASS: legacy ui context"
