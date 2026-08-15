#!/bin/bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "✗ $1"
  echo "FAIL P15 local model core skill: $1"
  exit 1
}

echo "P15 Local Model Core Skill"

if cargo test --manifest-path "$MANIFEST" \
  runtime::skills::registry::tests --quiet >"$TMP/registry.log" 2>&1; then
  echo "✓ Local Models Skill registry"
else
  grep -E "error|FAILED|failures:" "$TMP/registry.log" | head -20
  fail "Skill registry tests"
fi

if cargo test --manifest-path "$MANIFEST" \
  runtime::plan_runtime_bridge::tests --quiet >"$TMP/bridge.log" 2>&1; then
  echo "✓ Plan Runtime routing"
else
  grep -E "error|FAILED|failures:" "$TMP/bridge.log" | head -30
  fail "Plan Runtime routing"
fi

if cargo test --manifest-path "$MANIFEST" \
  runtime::executor::tests --quiet >"$TMP/executor.log" 2>&1; then
  echo "✓ Runtime executor regression"
else
  grep -E "error|FAILED|failures:" "$TMP/executor.log" | head -30
  fail "Runtime executor regression"
fi

if cargo check --manifest-path "$MANIFEST" \
  --quiet >"$TMP/check.log" 2>&1; then
  echo "✓ Rust backend compiles"
else
  grep -E "error\[|error:" "$TMP/check.log" | head -30
  fail "Rust backend compile"
fi

if curl --silent --fail --max-time 3 \
  http://127.0.0.1:11434/api/tags >"$TMP/tags.json" 2>/dev/null; then
  echo "✓ Live Ollama API reachable"
else
  fail "Ollama API is not reachable"
fi

echo "PASS P15 local model core skill"
exit 0
