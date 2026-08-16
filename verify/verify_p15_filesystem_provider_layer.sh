#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"

fail() {
    echo "FAIL P15 Filesystem Provider Layer: $1"
    exit 1
}


echo "---- Check filesystem provider modules ----"

test -f src-tauri/src/filesystem/provider.rs \
|| fail "provider.rs missing"

test -f src-tauri/src/filesystem/registry.rs \
|| fail "registry.rs missing"

test -f src-tauri/src/filesystem/runtime.rs \
|| fail "runtime.rs missing"

echo "✓ Filesystem provider modules exist"


echo
echo "---- Build Rust runtime ----"

cargo check \
--manifest-path src-tauri/Cargo.toml \
>/tmp/p15-filesystem-provider.log 2>&1 \
|| {
    grep -E "error:" /tmp/p15-filesystem-provider.log | head -20
    fail "cargo check"
}

echo "✓ Runtime builds"


echo
echo "PASS P15 Filesystem Provider Layer"
