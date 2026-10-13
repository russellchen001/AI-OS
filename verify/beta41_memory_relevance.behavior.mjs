import {
  selectRelevantMemoryFacts,
  scopedMemoryPolicy,
} from "../src/services/memoryRelevanceGate.ts";

const memories = [
  {
    id: "1",
    type: "user",
    content: "我之前购物的预算是500 AUD",
    createdAt: "",
    updatedAt: "",
  },
  {
    id: "2",
    type: "user",
    content: "我有群晖 NAS DS925+",
    createdAt: "",
    updatedAt: "",
  },
  {
    id: "3",
    type: "user",
    content: "默认使用中文回答",
    createdAt: "",
    updatedAt: "",
  },
  {
    id: "4",
    type: "user",
    content: "我研究过 SpaceX 股票",
    createdAt: "",
    updatedAt: "",
  },
];

const checks = [
  ["我正在研究群晖 NAS。", []],
  ["我正在研究海洋探测技术。", []],
  ["群晖 NAS 怎么备份？", ["2"]],
  ["之前购物的预算是多少？", ["1"]],
];

for (const [request, expected] of checks) {
  const actual = selectRelevantMemoryFacts(memories, request)
    .map((memory) => memory.id);

  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${request}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    );
  }

  console.log(`PASS: ${request}`);
}

const policy = {
  language: "Chinese",
  responseDetail: "concise",
  currency: "AUD",
  budget: { amount: 500 },
};

const scoped = scopedMemoryPolicy(policy, {});

if (
  scoped.language !== "Chinese" ||
  scoped.responseDetail !== "concise" ||
  scoped.budget !== undefined
) {
  throw new Error("Historical budget leaked into global policy.");
}

const current = scopedMemoryPolicy(policy, {
  budget: { amount: 100 },
});

if (current.budget?.amount !== 100) {
  throw new Error("Current-request budget was lost.");
}

console.log("PASS: Memory relevance and budget isolation");
