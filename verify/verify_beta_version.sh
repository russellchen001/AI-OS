#!/usr/bin/env bash
set -u

cd "$(dirname "$0")/.."
EXPECTED='1.0.1-beta.4'
failures=0

check_version() {
  label="$1"
  actual="$2"
  if [ "$actual" = "$EXPECTED" ]; then
    echo "✓ $label: $actual"
  else
    echo "✗ $label: expected $EXPECTED, got $actual"
    failures=$((failures + 1))
  fi
}

package_version=$(node -p "JSON.parse(require('fs').readFileSync('package.json','utf8')).version")
package_lock_version=$(node -p "JSON.parse(require('fs').readFileSync('package-lock.json','utf8')).version")
package_lock_root_version=$(node -p "JSON.parse(require('fs').readFileSync('package-lock.json','utf8')).packages[''].version")
tauri_version=$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")
cargo_version=$(
  cd src-tauri &&
    cargo metadata --no-deps --format-version 1 2>/dev/null |
    node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const m=JSON.parse(s);process.stdout.write(m.packages.find(p=>p.name==="dashboard").version)})'
)
app_info_version=$(
  node --experimental-strip-types --no-warnings -e \
    'import("./src/config/constants.ts").then(m=>process.stdout.write(m.APP_INFO.version))'
)

check_version package.json "$package_version"
check_version package-lock.json "$package_lock_version"
check_version 'package-lock.json root package' "$package_lock_root_version"
check_version src-tauri/Cargo.toml "$cargo_version"
check_version src-tauri/tauri.conf.json "$tauri_version"
check_version src/config/constants.ts "$app_info_version"

if [ "$(git rev-parse 'ai-os-v1.0.0^{}' 2>/dev/null)" != "0206dbea2803433a77f75b22d344436b529609bc" ]; then
  echo "✗ historical ai-os-v1.0.0 tag changed or is missing"
  failures=$((failures + 1))
else
  echo "✓ historical ai-os-v1.0.0 tag remains immutable"
fi

if [ "$(git rev-parse 'ai-os-v1.0.1-beta.1^{}' 2>/dev/null)" != "10e493728e70a88b7595773f2c3b6d778a0e0500" ]; then
  echo "✗ historical ai-os-v1.0.1-beta.1 tag changed or is missing"
  failures=$((failures + 1))
else
  echo "✓ historical ai-os-v1.0.1-beta.1 tag remains immutable"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL Beta version consistency: $failures checks failed"
  exit 1
fi

echo "PASS Beta version consistency"
