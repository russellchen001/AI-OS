#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"

fail() {
    echo "FAIL P15 Browser Provider Registry: $1"
    exit 1
}


echo "---- Check browser provider files ----"

test -f src-tauri/src/browser/provider.rs \
    || fail "provider.rs missing"

test -f src-tauri/src/browser/registry.rs \
    || fail "registry.rs missing"

echo "✓ Browser provider modules exist"


echo
echo "---- Build Rust runtime ----"

cargo check --manifest-path src-tauri/Cargo.toml \
    >/tmp/p15-browser-provider-build.log 2>&1 \
    || {
        grep -E "error:" /tmp/p15-browser-provider-build.log | head -20
        fail "cargo check"
    }

echo "✓ Runtime builds"


echo
echo "PASS P15 Browser Provider Registry"
