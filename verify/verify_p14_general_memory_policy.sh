#!/bin/bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail() {
  echo "❌ $1"
  echo "FAIL: P14 General Memory Policy"
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

verify_general_policy() {
  node --experimental-strip-types --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {
  applyMemoryPolicyToOutbound,
  resolveMemoryPolicy,
} from "./src/services/memoryPolicy.ts";

const memory = (content, index = 0) => ({
  id: String(index),
  type: "user",
  content,
  createdAt: `2026-08-12T00:00:0${index}Z`,
  updatedAt: `2026-08-12T00:00:0${index}Z`,
});

const languageMemories = [memory("我喜欢中文回答")];
assert.equal(
  resolveMemoryPolicy(languageMemories, "Please answer in English.").resolvedPolicy.language,
  "English",
);
assert.equal(
  resolveMemoryPolicy(languageMemories, "Explain it again.").resolvedPolicy.language,
  "Chinese",
);

const detailMemories = [memory("记住回答尽量简洁")];
assert.equal(
  resolveMemoryPolicy(detailMemories, "这次展开讲").resolvedPolicy.responseDetail,
  "detailed",
);
assert.equal(
  resolveMemoryPolicy(detailMemories, "下一个问题").resolvedPolicy.responseDetail,
  "concise",
);

const currencyMemories = [memory("记住金额默认用澳币")];
assert.equal(
  resolveMemoryPolicy(currencyMemories, "这次金额用美元").resolvedPolicy.currency,
  "USD",
);
assert.equal(
  resolveMemoryPolicy(currencyMemories, "下一条").resolvedPolicy.currency,
  "AUD",
);

const budgetMemories = [memory("记住我的默认预算是 500 澳币")];
assert.deepEqual(
  resolveMemoryPolicy(budgetMemories, "这次预算可以到 1000 澳币").resolvedPolicy.budget,
  { amount: 1000, currency: "AUD" },
);
assert.deepEqual(
  resolveMemoryPolicy(budgetMemories, "下一条").resolvedPolicy.budget,
  { amount: 500, currency: "AUD" },
);

const allMemories = [
  memory("我喜欢中文回答", 0),
  memory("回答尽量简洁", 1),
  memory("金额默认用澳币", 2),
  memory("默认预算 500 澳元", 3),
];
assert.deepEqual(
  resolveMemoryPolicy(allMemories, "本次预算 1000 AUD").resolvedPolicy,
  {
    language: "Chinese",
    responseDetail: "concise",
    currency: "AUD",
    budget: { amount: 1000, currency: "AUD" },
  },
);

const storedMessage = Object.freeze({ role: "user", content: "帮我推荐一件礼物" });
const originalHistory = Object.freeze([storedMessage]);
const resolved = resolveMemoryPolicy(allMemories, storedMessage.content).resolvedPolicy;
const outboundMessage = applyMemoryPolicyToOutbound(storedMessage, resolved);
assert.equal(storedMessage.content, "帮我推荐一件礼物");
assert.equal(originalHistory[0], storedMessage);
assert.notEqual(outboundMessage, storedMessage);
assert.match(outboundMessage.content, /Language: Chinese/);
assert.match(outboundMessage.content, /Response detail: concise/);
assert.match(outboundMessage.content, /Currency: AUD/);
assert.match(outboundMessage.content, /Budget: 500 AUD/);

assert.deepEqual(
  resolveMemoryPolicy([memory("我的电脑是 M4 MacBook Air")], "介绍我的电脑").resolvedPolicy,
  {
    language: undefined,
    responseDetail: undefined,
    currency: undefined,
    budget: undefined,
  },
);

const latestPreferences = [
  memory("默认回答简洁一点", 0),
  memory("默认展开讲", 1),
  memory("默认用美元", 2),
  memory("默认用人民币", 3),
  memory("默认预算 500 USD", 4),
  memory("默认预算 300.50 CNY", 5),
];
const latestPolicy = resolveMemoryPolicy(latestPreferences, "普通问题").longTermPolicy;
assert.equal(latestPolicy.responseDetail, "detailed");
assert.equal(latestPolicy.currency, "CNY");
assert.deepEqual(latestPolicy.budget, { amount: 300.5, currency: "CNY" });

assert.equal(
  resolveMemoryPolicy(
    [memory("我喜欢中文回答", 0), memory("我喜欢英文回答", 1)],
    "普通问题",
  ).longTermPolicy.language,
  undefined,
);
NODE
}

run_check "General policy behavior" verify_general_policy
run_check "Existing language baseline" ./verify/verify_p14_memory_language_policy.sh
run_check "Frontend production build" npm run build
run_check "Cargo check" cargo check --manifest-path src-tauri/Cargo.toml
run_check "Git diff check" git diff --check

echo "PASS: P14 General Memory Policy"
