#!/bin/zsh

set -u

ROOT="${0:A:h:h}"
LOG_DIR="$(mktemp -d -t ai-os-cloud-drive-mcp.XXXXXX)"
trap 'rm -rf "$LOG_DIR"' EXIT

fail() {
  print "✗ $1"
  print "FAIL Cloud Drive MCP: $1"
  exit 1
}

cd "$ROOT/src-tauri" || fail "cannot enter Rust workspace"

if cargo test --lib -- cloud_drive_quark::tests cloud_drive_aliyun::tests cloud_drive_123::tests cloud_drive_189::tests cloud_drive_139::tests >"$LOG_DIR/quark.log" 2>&1; then
  print "✓ Quark, UC, Aliyun, 123, 天翼 and 移动 protocol parsing and error reporting"
else
  grep -E "FAILED|error:" "$LOG_DIR/quark.log" | head -20
  fail "provider protocol tests failed"
fi

if cargo test cloud_drive_core::tests --lib >"$LOG_DIR/core.log" 2>&1; then
  print "✓ nine-provider link, extraction-code, and descriptor behavior"
else
  grep -E "FAILED|error:" "$LOG_DIR/core.log" | head -20
  fail "core behavior tests failed"
fi

if cargo test --bin ai-os-cloud-drive-mcp >"$LOG_DIR/mcp-tests.log" 2>&1; then
  print "✓ bounded MCP protocol behavior"
else
  grep -E "FAILED|error:" "$LOG_DIR/mcp-tests.log" | head -20
  fail "MCP protocol tests failed"
fi

if cargo build --bin ai-os-cloud-drive-mcp >"$LOG_DIR/build.log" 2>&1; then
  print "✓ independent native MCP binary builds"
else
  grep -E "^error|error\[" "$LOG_DIR/build.log" | head -20
  fail "MCP binary build failed"
fi

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"inspect_share_link","arguments":{"url":"https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2"}}}'
RESPONSE="$(print -r -- "$REQUEST" | target/debug/ai-os-cloud-drive-mcp)"
if print -r -- "$RESPONSE" | node -e '
  let value="";
  process.stdin.on("data", chunk => value += chunk);
  process.stdin.on("end", () => {
    const rpc = JSON.parse(value);
    const result = JSON.parse(rpc.result.content[0].text);
    if (result.provider !== "baidu" || result.extractionCode !== "a1b2") process.exit(1);
  });
'; then
  print "✓ built MCP process recognizes a Baidu share and extraction code"
else
  fail "built MCP process returned an invalid Baidu inspection result"
fi

if [[ -n "${AI_OS_CLOUD_DRIVE_E2E_DESCRIPTOR:-}" && -n "${AI_OS_CLOUD_DRIVE_E2E_DESTINATION:-}" ]]; then
  LIVE_REQUEST="$(node -e '
    const descriptor = JSON.parse(process.env.AI_OS_CLOUD_DRIVE_E2E_DESCRIPTOR);
    process.stdout.write(JSON.stringify({jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"download_resolved_file",arguments:{descriptor,destination:process.env.AI_OS_CLOUD_DRIVE_E2E_DESTINATION,retryLimit:3}}}));
  ')"
  LIVE_RESPONSE="$(print -r -- "$LIVE_REQUEST" | target/debug/ai-os-cloud-drive-mcp)"
  if print -r -- "$LIVE_RESPONSE" | node -e '
    let value="";
    process.stdin.on("data", chunk => value += chunk);
    process.stdin.on("end", () => {
      const rpc = JSON.parse(value);
      if (rpc.result?.isError || !rpc.result?.content?.[0]?.text) process.exit(1);
      const receipt = JSON.parse(rpc.result.content[0].text);
      if (!receipt.integrityVerified || receipt.bytes < 1 || !receipt.sha256) process.exit(1);
    });
  '; then
    print "✓ authorized real resolved-file E2E"
  else
    fail "authorized real resolved-file E2E failed"
  fi
else
  print "○ real provider E2E not run: legal share/auth-derived descriptor not supplied"
fi

cd "$ROOT" || fail "cannot return to project root"
if node_modules/.bin/esbuild verify/cloud_drive_download.behavior.ts \
    --bundle --platform=node --format=esm \
    --outfile="$LOG_DIR/cloud-drive-download.mjs" >"$LOG_DIR/flow-build.log" 2>&1 \
  && node "$LOG_DIR/cloud-drive-download.mjs" >"$LOG_DIR/flow.log" 2>&1; then
  print "✓ chat flow: link → automatic download of every file → report (no confirmation)"
else
  cat "$LOG_DIR/flow-build.log"
  grep -E "FAIL|Error" "$LOG_DIR/flow.log" | head -20
  fail "chat download flow behavior failed"
fi

if npm run build >"$LOG_DIR/frontend.log" 2>&1; then
  print "✓ frontend production build"
else
  grep -E "error" "$LOG_DIR/frontend.log" | head -20
  fail "frontend production build failed"
fi

if git diff --check >"$LOG_DIR/diff.log" 2>&1; then
  print "✓ changed files have no whitespace errors"
else
  cat "$LOG_DIR/diff.log"
  fail "whitespace check failed"
fi

print "PASS Cloud Drive MCP"
exit 0
