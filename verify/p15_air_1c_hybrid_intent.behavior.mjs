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
for (const fileRequest of ["在桌面新建一个叫 test2 的文件夹", "把 report2 移动到下载目录", "列出 Q3 文件夹里的文件"]) {
  if (publicProductRoutingIntent(fileRequest) !== undefined) {
    throw new Error(`File request was misclassified as a product mention: ${fileRequest}`);
  }
}
console.log("PASS DO: file requests with model-like names bypass product-fact ASK rule");

expectIntent("你好", "ASK");

expectIntent("我刚才在研究什么？", "ASK");
expectIntent("我们刚才聊了什么？", "ASK");
expectIntent("你还记得我刚才说了什么吗？", "ASK");
expectIntent("帮我总结刚才的对话", "ASK");

expectIntent("打开刚才提到的文件", "DO");
expectIntent("把刚才的内容保存为文档", "DO");
expectIntent("把这个文件移动到下载目录", "DO");
expectIntent("将报告导出为 PDF", "DO");
// Direct commands led by a location (phone remote requests, 2026-11-01).
expectIntent("在桌面新建一个叫 test2 的文件夹", "DO");
expectIntent("在桌面新建一个叫test的文件夹", "DO");
expectIntent("列出我桌面上的文件", "DO");
expectIntent("在下载里列出所有文件", "DO");
expectIntent("新建一个文件夹 notes", "DO");
expectUnresolved("在桌面上放什么文件比较好？");
expectIntent("在桌面新建文件夹有什么建议？", "ASK");
// How-to questions are instructions to explain, not commands (phone, 2026-11-01 23:05).
expectIntent("怎么在电脑上新建文件夹？", "ASK");
expectIntent("如何在桌面新建文件夹", "ASK");
expectIntent("在桌面新建文件夹的方法是什么？", "ASK");
expectIntent("请问怎么删除文件", "ASK");
expectIntent("How do I create a folder on my Mac?", "ASK");
expectIntent("在桌面新建一个叫 test3 的文件夹", "DO");
expectUnresolved("帮我写一首诗");

console.log("PASS: AIR-1C canonical semantic intent behavior");
