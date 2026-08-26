#!/bin/bash
set -u

NAME="P15-2 Document Read"
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
  local filter="$1" label="$2" log="$TMP_DIR/test.log"
  cargo test --manifest-path src-tauri/Cargo.toml "$filter" --lib >"$log" 2>&1 ||
    { tail -30 "$log"; fail "$label"; }
  grep -q "running 1 test" "$log" ||
    { tail -30 "$log"; fail "$label did not run"; }
  echo "✓ $label"
}

echo "$NAME"

cargo check --manifest-path src-tauri/Cargo.toml >"$TMP_DIR/check.log" 2>&1 ||
  { tail -30 "$TMP_DIR/check.log"; fail "Rust compile"; }
echo "✓ Rust compile"

run_test \
  "runtime::openclaw_permission::tests::document_read_requires_and_accepts_one_time_user_confirmation" \
  "Permission confirmation"

verify/verify_p15_office_provider_registry.sh >"$TMP_DIR/provider.log" 2>&1 ||
  { tail -30 "$TMP_DIR/provider.log"; fail "Office Provider Registry"; }
echo "✓ Office Provider Registry"

run_test \
  "runtime::openclaw_gateway_adapter::tests::document_read_uses_native_textutil_and_returns_text_result" \
  "Native Provider textutil Gateway behavior"

run_test \
  "runtime::openclaw_gateway_adapter::tests::document_read_rejects_relative_path_without_gateway_call" \
  "Invalid path fail-closed behavior"

printf 'P15 Document Read real DOCX\n' >"$TMP_DIR/source.txt"
/usr/bin/textutil -convert docx -output "$TMP_DIR/sample.docx" "$TMP_DIR/source.txt" ||
  fail "Real DOCX creation"
/usr/bin/textutil -convert txt -stdout "$TMP_DIR/sample.docx" >"$TMP_DIR/readback.txt" ||
  fail "Real DOCX read"
grep -Fq "P15 Document Read real DOCX" "$TMP_DIR/readback.txt" ||
  fail "Real DOCX content mismatch"
echo "✓ Real DOCX creation and read"

echo "PASS $NAME"
exit 0
