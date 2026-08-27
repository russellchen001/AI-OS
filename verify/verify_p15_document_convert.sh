#!/bin/bash
set -u

NAME="P15-2 Document Convert"
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
  "runtime::openclaw_permission::tests::document_convert_requires_and_accepts_one_time_user_confirmation" \
  1 "Permission confirmation"

run_test \
  "document::registry::tests::document_convert_resolves_to_macos_native_first" \
  1 "Native Provider resolution"

run_test \
  "runtime::openclaw_gateway_adapter::tests::document_convert_" \
  3 "Input, Native helper and Gateway behavior"

printf 'P15 Document Convert real content\n' >"$TMP_DIR/source.txt"
/usr/bin/textutil -convert doc -output "$TMP_DIR/source.doc" "$TMP_DIR/source.txt" ||
  fail "Real DOC source creation"
/usr/bin/textutil -convert docx -output "$TMP_DIR/staged.docx" "$TMP_DIR/source.doc" ||
  fail "Real DOC to DOCX conversion"
/bin/ln "$TMP_DIR/staged.docx" "$TMP_DIR/result.docx" ||
  fail "Atomic destination creation"
/usr/bin/textutil -convert txt -stdout "$TMP_DIR/result.docx" >"$TMP_DIR/readback.txt" ||
  fail "Converted DOCX read"
grep -Fq "P15 Document Convert real content" "$TMP_DIR/readback.txt" ||
  fail "Converted content mismatch"
/bin/ln "$TMP_DIR/staged.docx" "$TMP_DIR/result.docx" 2>/dev/null &&
  fail "Existing destination was overwritten"
echo "✓ Real DOC to DOCX conversion and no-overwrite"

echo "PASS $NAME"
exit 0
