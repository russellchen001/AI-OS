import { shouldRetrieveConversationHistory as should } from
  "../src/services/conversationRetrievalGate.ts";

const cases = [
  ["hello", false],
  ["你好", false],
  ["好的", false],
  ["我正在研究群晖 NAS。", false],
  ["我正在研究一种全新的海洋探测技术。", false],
  ["我刚才在研究什么？", false],
  ["我们刚刚讨论了什么？", false],
  ["如果把刚才的内容保存为文档，会有什么好处？", false],
  ["上次我们讨论的 NAS 方案是什么？", true],
  ["你还记得以前我说过的存储需求吗？", true],
  ["找出之前的聊天内容", true],
  ["群晖 NAS 的存储方案应该怎么选择？", true],
  ["如何规划家庭 NAS 的备份方案？", true],
  ["What did we discuss last time?", true],
];

for (const [input, expected] of cases) {
  const actual = should(input);
  if (actual !== expected) {
    throw new Error(
      `FAIL: ${JSON.stringify(input)} expected=${expected} actual=${actual}`
    );
  }
  console.log(`PASS ${actual ? "RETRIEVE" : "SKIP"}: ${input}`);
}

console.log("PASS: Conversation retrieval relevance gate");
