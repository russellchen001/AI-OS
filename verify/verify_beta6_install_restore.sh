#!/bin/bash
set -uo pipefail

fail() { echo "❌ $1"; echo "FAIL: Beta 6 install restore — $1"; exit 1; }
expected="${1:-}"
backup="${2:-}"
dmg="${3:-}"
app='/Applications/AI OS.app'
test -n "$expected" && test -d "$backup" && test -f "$dmg" || fail "expected hash, backup app and DMG are required"
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist") || fail "installed version unavailable"
test "$version" = '1.0.1-beta.5' || fail "installed app is not restored Beta 5"
echo '✅ Installed version is Beta 5'
actual=$(shasum -a 256 "$app/Contents/MacOS/dashboard") || fail "installed executable unavailable"
test "${actual%% *}" = "$expected" || fail "restored executable hash differs"
echo '✅ Restored executable SHA-256 matches pre-install receipt'
cmp -s "$app/Contents/MacOS/dashboard" "$backup/Contents/MacOS/dashboard" || fail "backup executable differs"
diff -qr "$app" "$backup" >/dev/null || fail "restored bundle differs from complete backup"
echo '✅ Complete restored bundle matches backup'
codesign --verify --deep --strict "$app" || fail "restored signature invalid"
echo '✅ Restored app signature is valid'
hdiutil verify "$dmg" >/dev/null 2>&1 || fail "DMG integrity verification failed"
echo '✅ Final DMG integrity is valid'
echo 'PASS: Beta 6 install restore'
exit 0
