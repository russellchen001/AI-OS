#!/bin/bash
set -u

cd "$(dirname "$0")/.." || exit 1

echo "P15-2 Document Skill Registry"

CHECK_LOG="/tmp/p15-document-registry-check.log"
TEST_LOG="/tmp/p15-document-registry-tests.log"

if cargo check --manifest-path src-tauri/Cargo.toml >"$CHECK_LOG" 2>&1; then
    echo "✓ Rust compile passed"
else
    tail -30 "$CHECK_LOG"
    echo "✗ Rust compile failed"
    echo "FAIL P15-2 Document Skill Registry: compile"
    exit 1
fi

if cargo test \
    --manifest-path src-tauri/Cargo.toml \
    runtime::skills::registry::tests:: \
    --lib >"$TEST_LOG" 2>&1; then
    echo "✓ Skill registry behavior passed"
else
    tail -30 "$TEST_LOG"
    echo "✗ Skill registry tests failed"
    echo "FAIL P15-2 Document Skill Registry: registry behavior"
    exit 1
fi

echo "PASS P15-2 Document Skill Registry"
exit 0
