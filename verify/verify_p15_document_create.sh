#!/bin/bash
set -u

NAME="P15-2 Document Create"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
cd "$ROOT" || exit 1

fail() {
  echo "✗ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

run_test() {
  local filter="$1" expected="$2" label="$3" log="$TMP_DIR/test.log"
  cargo test --manifest-path src-tauri/Cargo.toml "$filter" --lib >"$log" 2>&1 ||
    { tail -30 "$log"; fail "$label"; }
  grep -q "running $expected test" "$log" ||
    { tail -30 "$log"; fail "$label did not run"; }
  echo "✓ $label"
}

echo "$NAME"

cargo check --manifest-path src-tauri/Cargo.toml >"$TMP_DIR/check.log" 2>&1 ||
  { tail -30 "$TMP_DIR/check.log"; fail "Rust compile"; }
echo "✓ Rust compile"

run_test \
  "runtime::openclaw_permission::tests::document_create_requires_and_accepts_one_time_user_confirmation" \
  1 "Permission confirmation"

run_test \
  "document::registry::tests::document_create_resolves_to_macos_native_first" \
  1 "Native Provider resolution"

run_test \
  "runtime::openclaw_gateway_adapter::tests::document_create_" \
  3 "Input, Native helper and Gateway behavior"

printf 'P15 Document Create real DOCX\n' >"$TMP_DIR/source.txt"
/usr/bin/textutil -convert docx -output "$TMP_DIR/staged.docx" "$TMP_DIR/source.txt" ||
  fail "Real DOCX conversion"
/bin/ln "$TMP_DIR/staged.docx" "$TMP_DIR/created.docx" ||
  fail "Atomic create"
/usr/bin/textutil -convert txt -stdout "$TMP_DIR/created.docx" >"$TMP_DIR/readback.txt" ||
  fail "Real DOCX read"
grep -Fq "P15 Document Create real DOCX" "$TMP_DIR/readback.txt" ||
  fail "Real DOCX content mismatch"
/bin/ln "$TMP_DIR/staged.docx" "$TMP_DIR/created.docx" 2>/dev/null &&
  fail "Existing target was overwritten"
echo "✓ Real DOCX create, read and no-overwrite"

echo "PASS $NAME"
exit 0
