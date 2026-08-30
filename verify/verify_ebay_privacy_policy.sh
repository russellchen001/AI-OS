#!/bin/bash

set -u

FEATURE="eBay privacy policy"
BUILD_LOG="$(mktemp)"
PREVIEW_LOG="$(mktemp)"
RESPONSE_FILE="$(mktemp)"
PREVIEW_PID=""

cleanup() {
  if [[ -n "$PREVIEW_PID" ]]; then
    kill "$PREVIEW_PID" 2>/dev/null || true
    wait "$PREVIEW_PID" 2>/dev/null || true
  fi
  rm -f "$BUILD_LOG" "$PREVIEW_LOG" "$RESPONSE_FILE"
}
trap cleanup EXIT

fail() {
  echo "❌ $1"
  echo "FAIL: $FEATURE — $1"
  exit 1
}

if npm run build >"$BUILD_LOG" 2>&1; then
  echo "✅ Frontend build produced the static privacy page"
else
  tail -20 "$BUILD_LOG"
  fail "frontend build failed"
fi

[[ -s dist/privacy/index.html ]] || fail "built privacy page is missing"

npx vite preview --host 127.0.0.1 --port 4179 --strictPort >"$PREVIEW_LOG" 2>&1 &
PREVIEW_PID=$!

HTTP_STATUS=""
for _ in {1..40}; do
  HTTP_STATUS="$(curl --silent --output "$RESPONSE_FILE" --write-out '%{http_code}' http://127.0.0.1:4179/privacy/ || true)"
  [[ "$HTTP_STATUS" == "200" ]] && break
  sleep 0.25
done

[[ "$HTTP_STATUS" == "200" ]] || {
  tail -20 "$PREVIEW_LOG"
  fail "local privacy URL did not return HTTP 200"
}
echo "✅ /privacy/ opens locally over HTTP"

node --input-type=module - "$RESPONSE_FILE" <<'NODE' || fail "rendered policy validation failed"
import { readFileSync } from "node:fs";

const html = readFileSync(process.argv[2], "utf8");
const required = [
  "AI-OS Privacy Policy",
  "Effective date:",
  "What data AI-OS accesses",
  "How authentication works",
  "Data storage and retention",
  "eBay data handling",
  "Revoking access",
  "Changes to this policy",
  "Contact",
];

for (const heading of required) {
  if (!html.includes(heading)) {
    throw new Error(`missing rendered section: ${heading}`);
  }
}

if (!/^<!doctype html>/i.test(html.trim()) || !html.includes("</html>")) {
  throw new Error("rendered output is not a complete HTML document");
}

const credentialPatterns = [
  /client[_ -]?secret\s*[:=]\s*["']?[A-Za-z0-9._~-]{8,}/i,
  /access[_ -]?token\s*[:=]\s*["']?[A-Za-z0-9._~-]{8,}/i,
  /refresh[_ -]?token\s*[:=]\s*["']?[A-Za-z0-9._~-]{8,}/i,
  /bearer\s+[A-Za-z0-9._~-]{20,}/i,
  /-----BEGIN (?:RSA |EC )?PRIVATE KEY-----/,
];

if (credentialPatterns.some((pattern) => pattern.test(html))) {
  throw new Error("rendered page appears to contain a credential value");
}
NODE

echo "✅ Rendered HTML contains every required section"
echo "✅ Rendered page contains no credential-shaped values"
echo "PASS: $FEATURE"
