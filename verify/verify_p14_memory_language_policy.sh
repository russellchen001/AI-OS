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
  applyMemoryPolicyToOutbound,
  resolveMemoryPolicy,
} from "./src/services/memoryPolicy.ts";

const memories = ["我喜欢中文回答", "记住我喜欢中文回答"].map((content, index) => ({
  id: String(index),
  type: "user",
  content,
  createdAt: `2026-08-12T00:00:0${index}Z`,
  updatedAt: `2026-08-12T00:00:0${index}Z`,
}));

const cases = [
  ["Please answer in English. Explain what AI Council is.", "English"],
  ["Explain what AI Council is again.", "Chinese"],
  ["Please answer in English again. Explain one advantage of AI Council.", "English"],
  ["Explain another advantage.", "Chinese"],
];

for (const [content, expected] of cases) {
  const { resolvedPolicy } = resolveMemoryPolicy(memories, content);
  assert.equal(resolvedPolicy.language, expected);
  const storedMessage = { role: "user", content };
  const outboundMessage = applyMemoryPolicyToOutbound(storedMessage, resolvedPolicy);
  assert.equal(storedMessage.content, content);
  assert.match(outboundMessage.content, new RegExp(`- Language: ${expected}`));
}

assert.equal(resolveMemoryPolicy([], "No language preference.").resolvedPolicy.language, undefined);
assert.equal(
  applyMemoryPolicyToOutbound({ content: "No language preference." }, {}).content,
  "No language preference.",
);
NODE
}

run_check "Language policy behavior" verify_language_policy
run_check "Frontend production build" npm run build
run_check "Cargo check" cargo check --manifest-path src-tauri/Cargo.toml
run_check "Git diff check" git diff --check

echo "PASS: P14 Memory Language Policy"
