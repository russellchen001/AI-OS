#!/bin/zsh

cd "$(git rev-parse --show-toplevel)" || exit 1

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

run_check() {
  local name="$1"
  shift
  local log="$TMP_DIR/${name// /-}.log"
  if "$@" >"$log" 2>&1; then
    print "✓ $name"
  else
    print "✗ $name"
    tail -40 "$log"
    print "FAIL Beta.4 release candidate: $name"
    exit 1
  fi
}

run_check "version consistency" ./verify/verify_beta_version.sh
run_check "model context budget" ./verify/verify_beta4_context_budget.sh
run_check "knowledge engine behavior" ./verify/verify_beta4_knowledge_engine.sh
run_check "Rust compile" cargo check --manifest-path src-tauri/Cargo.toml --quiet
run_check "full Rust library regression" cargo test --manifest-path src-tauri/Cargo.toml --lib --quiet
run_check "Beta4 Rust formatting" rustfmt --edition 2021 --check \
  src-tauri/src/knowledge.rs src-tauri/src/memory.rs
run_check "macOS app and DMG signatures" ./verify/verify_beta_macos_bundle_signature.sh
run_check "release package data hygiene" ./verify/verify_beta_package_hygiene.sh
run_check "repository whitespace" git diff --check

print "PASS Beta.4 release candidate"
