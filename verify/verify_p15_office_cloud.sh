#!/bin/bash
set -u

NAME="P15 Office Cloud (Google Workspace)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/Cargo.toml"
LOG="${AIOS_RUN_DIR:-${TMPDIR:-/tmp}}/office-cloud.log"

cd "$ROOT" || exit 1

fail() {
  echo "❌ $1"
  echo "FAIL $NAME: $1"
  exit 1
}

echo "$NAME"

# Google's Docs, Sheets and Slides adapters were real, proven by their own
# E2Es, and reachable from no provider-neutral capability -- the fifth adapter
# in this work found working and callable from nowhere. Two things had to be
# settled to close that, and neither was an omission: a Drive file id is not a
# path, so the request needs a shape that says which resource it means; and the
# gateway is synchronous while the adapters are async.

cargo test --manifest-path "$MANIFEST" --lib \
  google_workspace::office::tests \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "cloud request contract"
  }

grep -q "running 3 tests" "$LOG" || {
  tail -20 "$LOG"
  fail "cloud request contract test count changed"
}

echo "✅ a cloud resource is read from either request shape, and a file id is required"
echo "✅ the same tab-separated body that creates a local workbook creates a Google Sheet"
echo "✅ a create needs a title and says so"

cargo test --manifest-path "$MANIFEST" --lib \
  document::resolver::tests::a_cloud_resource_reaches_google_and_a_local_path_never_does \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "cloud routing boundaries"
  }

echo "✅ every cloud capability reaches Docs, Sheets or Slides -- never a local application"
echo "✅ what Google has no adapter for claims no cloud route"
echo "✅ with the account not connected there is no cloud route at all"

cargo test --manifest-path "$MANIFEST" --lib \
  document::resolver::tests::local_paths_never_route_to_google \
  >"$LOG" 2>&1 || {
    tail -40 "$LOG"
    fail "local paths never route to Google"
  }

echo "✅ a local path never routes to Google, which was true before and still is"

# The live half needs a connected account, and it is a real write to the
# person's Drive, so it is opt-in by an explicit environment variable rather
# than run because credentials happen to exist.
if [ -z "${AI_OS_GOOGLE_LIVE:-}" ]; then
  echo "SKIP $NAME: live Google round trip not requested (set AI_OS_GOOGLE_LIVE=1)"
  echo "PASS $NAME"
  exit 0
fi

cargo test --manifest-path "$MANIFEST" --lib \
  google_workspace::live \
  -- --ignored >"$LOG" 2>&1 || {
    tail -60 "$LOG"
    fail "live Google round trip"
  }

grep -q "test result: ok" "$LOG" || fail "live Google result marker"

echo "✅ a Google Sheet is created, written and read back through the provider-neutral capability"

echo "PASS $NAME"
