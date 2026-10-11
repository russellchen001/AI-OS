import { classifySemanticIntent } from "../src/services/intentSemantic.ts";

function expectIntent(input, expected) {
  const result = classifySemanticIntent(input);

  if (result?.taskType !== expected) {
    throw new Error(
      `Expected ${expected} for ${JSON.stringify(input)}, got ${JSON.stringify(result)}`,
    );
  }

  console.log(`PASS ${expected}: ${input || "<empty>"}`);
}

function expectUnresolved(input) {
  const result = classifySemanticIntent(input);

  if (result !== undefined) {
    throw new Error(
      `Expected unresolved intent for ${JSON.stringify(input)}, got ${JSON.stringify(result)}`,
    );
  }

  console.log(`PASS UNRESOLVED: ${input}`);
}

expectIntent("", "ASK");
expectIntent("什么是 KNX？", "ASK");
expectIntent("比较一下 oMLX 和 ComfyUI", "ASK");
expectIntent("你建议我怎么整理我的 NAS 文件？", "ASK");
expectIntent("What is oMLX?", "ASK");

expectIntent("帮我查看这台电脑的磁盘空间", "DO");
expectIntent("请删除这个文件", "DO");
expectIntent("检查我现在的 NAS 可用空间", "DO");
expectIntent("Check my current disk space", "DO");

expectUnresolved("你好");
expectUnresolved("帮我写一首诗");

console.log("PASS: AIR-1C canonical semantic intent behavior");
