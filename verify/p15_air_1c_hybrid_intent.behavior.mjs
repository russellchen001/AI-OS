import { classifySemanticIntent } from "../src/services/intentSemantic.ts";
import { publicProductRoutingIntent } from "../src/services/publicFactGrounding.ts";

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
expectIntent("请使用 computer.use.execute 在当前 TextEdit 文档中输入测试标记", "DO");
const guiRequest = "请立即执行：在当前 TextEdit 文档中输入 AIOS_RC_E2E_1019";
if (publicProductRoutingIntent(guiRequest) !== undefined) {
  throw new Error("GUI execution was misclassified as a product mention");
}
console.log("PASS DO: GUI execution bypasses product-fact ASK rule");

expectIntent("你好", "ASK");

expectIntent("我刚才在研究什么？", "ASK");
expectIntent("我们刚才聊了什么？", "ASK");
expectIntent("你还记得我刚才说了什么吗？", "ASK");
expectIntent("帮我总结刚才的对话", "ASK");

expectIntent("打开刚才提到的文件", "DO");
expectIntent("把刚才的内容保存为文档", "DO");
expectIntent("把这个文件移动到下载目录", "DO");
expectIntent("将报告导出为 PDF", "DO");
expectUnresolved("帮我写一首诗");

console.log("PASS: AIR-1C canonical semantic intent behavior");
