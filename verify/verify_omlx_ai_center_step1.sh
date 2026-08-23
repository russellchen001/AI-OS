#!/bin/bash

set -u

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_FILE="$(mktemp)"
trap 'rm -f "$OUTPUT_FILE"' EXIT

fail() {
    echo "✗ $1"
    grep -E "error|FAILED|failures:|test result:" "$OUTPUT_FILE" | head -20
    echo "FAIL oMLX AI Center: $1"
    exit 1
}

echo "Running oMLX AI Center verification..."

if (cd "$ROOT_DIR" && cargo test --manifest-path src-tauri/Cargo.toml providers::tests --quiet) >"$OUTPUT_FILE" 2>&1; then
    echo "✓ Local First, Manual, fallback, cancellation, and oMLX provider tests passed"
else
    fail "Rust provider behavior failed"
fi

if (cd "$ROOT_DIR" && npm run build) >"$OUTPUT_FILE" 2>&1; then
    echo "✓ My AI oMLX provider UI build passed"
else
    fail "frontend build failed"
fi

if (cd "$ROOT_DIR" && git diff --check) >"$OUTPUT_FILE" 2>&1; then
    echo "✓ changed files contain no whitespace errors"
else
    fail "git diff check failed"
fi

echo "PASS oMLX AI Center"
