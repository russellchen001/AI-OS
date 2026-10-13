import {
  buildProductAnswerPrompt,
  buildPublicProductTaskPrompt,
  isProductFactQuestion,
  isUncertainProductAnswer,
  PRODUCT_UNSURE_MARKER,
  publicProductFactSubject,
  publicProductRoutingIntent,
} from "../src/services/publicFactGrounding.ts";
import {
  buildGroundedSearchPrompt,
  formatGroundedSearchAnswer,
  searchEvidence,
} from "../src/services/workOutputFormatter.ts";

const routingCases = [
  ["我在研究 NAS DS925+", undefined],
  // Owner decision 2026-10-14: product questions are answered by the model;
  // only an explicit search request goes online.
  ["群晖 DS925+ 是哪个品牌的产品？", undefined],
  ["帮我搜索 DS925+ 官方参数", "ds925+"],
  ["Who makes QNAP TS-464?", undefined],
  ["MacBook Air M4 16GB 的规格是什么？", undefined],
  ["查一下 MacBook Air M4 16GB 的规格", "macbook air m4 16gb"],
  ["你好", undefined],
  ["什么是 RAID？", undefined],
  ["写一首关于 DS925+ 的诗", undefined],
  ["删除本地 Qwen2.5 模型", undefined],
];

for (const [request, expected] of routingCases) {
  const actual = publicProductFactSubject(request);
  if (actual !== expected) {
    throw new Error(`FAIL routing ${JSON.stringify(request)} expected=${expected} actual=${actual}`);
  }
  console.log(`✓ ${expected ? "VERIFY" : "LOCAL"}: ${request}`);
}

const taskPrompt = buildPublicProductTaskPrompt("nas ds925+");
if (
  !taskPrompt.includes("nas ds925+") ||
  !taskPrompt.includes("简洁中文回答") ||
  taskPrompt.includes("我在研究")
) {
  throw new Error(`FAIL minimized task prompt: ${taskPrompt}`);
}
console.log("✓ OpenClaw receives only the compact public product identifier");

const intentCases = [
  ["我在研究 NAS DS925+", "ASK"],
  ["DS925+ 是哪个品牌的？", "ASK"],
  ["帮我搜索 DS925+ 官方参数", "DO"],
  // BUG-41-A: statements and questions stay with the model even when they
  // contain fact-topic words; only an explicit search request goes online.
  ["我在研究nas ds925+", "ASK"],
  ["目前我在用 DS925+", "ASK"],
  ["我在研究 DS925+ 这个产品", "ASK"],
  ["我的设备是 DS925+", "ASK"],
  ["我在研究 DS925+ 的配置", "ASK"],
  ["我刚买了一台 DS925+", "ASK"],
  ["群晖 DS925+ 是哪个品牌的产品？", "ASK"],
  ["DS925+ 有几个盘位？", "ASK"],
  ["DS925+ 支持 10GbE 吗", "ASK"],
  ["查一下 DS925+ 价格", "DO"],
  ["Who makes QNAP TS-464?", "ASK"],
  ["search DS925+ specs", "DO"],
];

// Questions get the answer-from-knowledge guidance; statements get the
// restate-only guidance. The search path is decided above, not here.
for (const [request, expected] of [
  ["DS925+ 是哪个品牌的？", true],
  ["DS925+ 有几个盘位？", true],
  ["我在研究 NAS DS925+", false],
  ["我在研究 DS925+ 的配置", false],
  ["帮我搜索 DS925+ 官方参数", false],
  ["你好", false],
]) {
  if (isProductFactQuestion(request) !== expected) {
    throw new Error(`FAIL product question ${JSON.stringify(request)} expected=${expected}`);
  }
}
console.log("✓ Product questions answered by the model; background mentions restated only");

// Owner decision 2026-10-14: the model answers first and searches only when unsure.
const answerPrompt = buildProductAnswerPrompt("DS925+ 是哪个品牌的？");
if (!answerPrompt.includes(PRODUCT_UNSURE_MARKER) || !answerPrompt.includes("DS925+ 是哪个品牌的？") || !answerPrompt.includes("Never guess")) {
  throw new Error(`FAIL product answer prompt: ${answerPrompt}`);
}
for (const [text, expected] of [
  ["NEED_SEARCH", true],
  ["  need_search\n", true],
  ["", true],
  ["DS925+ 是群晖（Synology）的 NAS。", false],
]) {
  if (isUncertainProductAnswer(text) !== expected) {
    throw new Error(`FAIL uncertainty ${JSON.stringify(text)} expected=${expected}`);
  }
}
console.log("✓ Model answers product questions first and escalates to search only when unsure");

for (const [request, expected] of intentCases) {
  const actual = publicProductRoutingIntent(request);
  if (actual?.taskType !== expected) {
    throw new Error(`FAIL intent ${JSON.stringify(request)} expected=${expected} actual=${actual?.taskType}`);
  }
  console.log(`✓ ${expected}: ${request}`);
}

// Case E: the decision depends on the current turn only, so a question or a
// background statement after a search turn in the same conversation stays ASK.
const conversation = ["帮我搜索 DS925+ 官方参数", "DS925+ 是哪个品牌的？", "我在研究 NAS DS925+"];
const routed = conversation.map((turn) => publicProductRoutingIntent(turn)?.taskType);
if (routed.join() !== "DO,ASK,ASK") {
  throw new Error(`FAIL consecutive conversation routing: ${routed.join()}`);
}
console.log("✓ Consecutive turns: a background statement after searches stays ASK");

// BUG-41-B: search output becomes a grounded Chinese answer with clean sources.
const searchOutput = {
  skillInvocation: {
    capability: "browser.search",
    output: {
      query: "ds925+",
      results: [
        { title: "DiskStation DS925+ | Synology Inc.", url: "https://www.synology.com/en-global/products/DS925+", snippet: "Synology DS925+ 4-bay NAS." },
        { title: "Not a web link", url: "javascript:alert(1)", snippet: "" },
      ],
    },
  },
};
const evidence = searchEvidence(searchOutput);
if (evidence?.length !== 1 || !evidence[0].url.startsWith("https://www.synology.com/")) {
  throw new Error(`FAIL search evidence: ${JSON.stringify(evidence)}`);
}
const groundedPrompt = buildGroundedSearchPrompt("DS925+ 是哪个品牌的？", evidence);
if (!groundedPrompt.includes("仅根据") || !groundedPrompt.includes("DiskStation DS925+") || !groundedPrompt.includes("简体中文")) {
  throw new Error(`FAIL grounded prompt: ${groundedPrompt}`);
}
const answer = formatGroundedSearchAnswer("DS925+ 是群晖（Synology）的 NAS。", evidence);
if (!answer.startsWith("DS925+ 是群晖") || !answer.includes("[DiskStation DS925+ | Synology Inc.](https://www.synology.com/en-global/products/DS925+)") || answer.includes("### 搜索结果")) {
  throw new Error(`FAIL grounded answer: ${answer}`);
}
if (searchEvidence({ skillInvocation: { capability: "filesystem.read", output: {} } }) !== undefined) {
  throw new Error("FAIL non-search output must not be treated as search evidence");
}
console.log("✓ Search evidence becomes a grounded Chinese answer with clean source links");
console.log("PASS: Beta 4.1 selective public fact grounding");
