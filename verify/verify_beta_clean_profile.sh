#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."

TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ai-os-beta-clean-profile.XXXXXX")"
TEST_HOME="$TEST_ROOT/home"
BACKUP_ROOT="$TEST_ROOT/backup"
RESTORE_SCRIPT="$TEST_ROOT/restore.sh"

cleanup() {
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

paths=(
  "Library/Application Support/AI-OS"
  "Library/Application Support/AI OS"
  "Library/Application Support/com.russellchen.aios"
  "Library/WebKit/com.russellchen.aios"
  "Library/Caches/com.russellchen.aios"
  "Library/HTTPStorages/com.russellchen.aios"
  "Library/Saved Application State/com.russellchen.aios.savedState"
  "Library/Preferences/com.russellchen.aios.plist"
  "Library/Cookies/com.russellchen.aios.binarycookies"
)

mkdir -p "$TEST_HOME" "$BACKUP_ROOT"
: > "$RESTORE_SCRIPT"
printf '%s\n' '#!/usr/bin/env bash' 'set -u' >> "$RESTORE_SCRIPT"

for relative in "${paths[@]}"; do
  original="$TEST_HOME/$relative"
  backup="$BACKUP_ROOT/$relative"

  case "$relative" in
    *.plist|*.binarycookies)
      mkdir -p "${original%/*}"
      printf 'fixture:%s\n' "$relative" > "$original"
      ;;
    *)
      mkdir -p "$original"
      printf 'fixture:%s\n' "$relative" > "$original/.beta-fixture"
      ;;
  esac

  mkdir -p "${backup%/*}"
  mv "$original" "$backup"
  printf 'mkdir -p %q\nmv %q %q\n' \
    "${original%/*}" "$backup" "$original" >> "$RESTORE_SCRIPT"
done

chmod 700 "$RESTORE_SCRIPT"

failures=0
for relative in "${paths[@]}"; do
  if [ -e "$TEST_HOME/$relative" ]; then
    echo "✗ isolation left profile data at $relative"
    failures=$((failures + 1))
  elif [ ! -e "$BACKUP_ROOT/$relative" ]; then
    echo "✗ backup missing for $relative"
    failures=$((failures + 1))
  else
    echo "✓ isolated $relative"
  fi
done

if [ -e "$TEST_HOME/Library/Application Support/AI-OS/conversations.sqlite3" ] ||
   [ -e "$TEST_HOME/Library/Application Support/AI OS/openclaw-servers.json" ]; then
  echo "✗ fresh profile retained chats or OpenClaw server records"
  failures=$((failures + 1))
else
  echo "✓ fresh profile has no chats or OpenClaw server records"
fi

if ! "$RESTORE_SCRIPT"; then
  echo "✗ restore script failed"
  failures=$((failures + 1))
fi

for relative in "${paths[@]}"; do
  if [ ! -e "$TEST_HOME/$relative" ]; then
    echo "✗ restore missing $relative"
    failures=$((failures + 1))
  elif [ -e "$BACKUP_ROOT/$relative" ]; then
    echo "✗ restore left backup copy for $relative"
    failures=$((failures + 1))
  else
    echo "✓ restored $relative"
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta clean-profile isolation: $failures checks failed"
  exit 1
fi

echo "PASS Beta clean-profile isolation and restoration"
