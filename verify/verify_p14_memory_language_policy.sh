#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: P14 Memory Language Policy"
  exit 1
}

run_check() {
  local label="$1"
  shift

  if "$@"; then
    echo "✅ $label"
  else
    fail "$label"
  fi
}

verify_language_policy() {
  node --experimental-strip-types --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {
  applyOutboundLanguage,
  detectCurrentLanguage,
  detectMemoryLanguage,
} from "./src/pages/chatLanguagePolicy.ts";

const memories = ["我喜欢中文回答", "记住我喜欢中文回答"];
const defaultLanguage = detectMemoryLanguage(memories);
assert.equal(defaultLanguage, "Chinese");

const cases = [
  ["Please answer in English. Explain what AI Council is.", "English"],
  ["Explain what AI Council is again.", "Chinese"],
  ["Please answer in English again. Explain one advantage of AI Council.", "English"],
  ["Explain another advantage.", "Chinese"],
];

for (const [content, expected] of cases) {
  const language = detectCurrentLanguage(content) ?? defaultLanguage;
  assert.equal(language, expected);
  const storedMessage = { role: "user", content };
  const outboundMessage = {
    ...storedMessage,
    content: applyOutboundLanguage(storedMessage.content, language),
  };
  assert.equal(storedMessage.content, content);
  assert.match(outboundMessage.content, new RegExp(`Current response language: ${expected}`));
}

assert.equal(detectMemoryLanguage(["I prefer concise answers."]), undefined);
assert.equal(applyOutboundLanguage("No language preference.", undefined), "No language preference.");
NODE
}

run_check "Language policy behavior" verify_language_policy
run_check "Frontend production build" npm run build
run_check "Cargo check" cargo check --manifest-path src-tauri/Cargo.toml
run_check "Git diff check" git diff --check

echo "PASS: P14 Memory Language Policy"
