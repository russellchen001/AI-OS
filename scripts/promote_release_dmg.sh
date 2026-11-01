#!/usr/bin/env bash
set -euo pipefail

# Promotes one build candidate to the release name in the fixed releases
# folder: ~/AI-OS/releases/AI-OS_<version>_aarch64.dmg.
cd "$(dirname "$0")/.."

commit="${1:?usage: scripts/promote_release_dmg.sh <commit>}"
version=$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")
releases_directory="${AI_OS_RELEASES_DIR:-$HOME/AI-OS/releases}"
candidate="$releases_directory/candidates/AI-OS_${version}_${commit}_aarch64.dmg"
release="$releases_directory/AI-OS_${version}_aarch64.dmg"

if [ ! -f "$candidate" ]; then
  echo "FAIL promote: no candidate $candidate"
  exit 1
fi
if [ -e "$release" ]; then
  echo "FAIL promote: $release already exists"
  exit 1
fi
hdiutil verify "$candidate" >/dev/null
ditto "$candidate" "$release"
echo "Release: $release"
echo "SHA-256: $(shasum -a 256 "$release" | cut -d' ' -f1)"
