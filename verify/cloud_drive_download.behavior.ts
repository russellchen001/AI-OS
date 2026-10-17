/*
 * Beta 5: "send a share link, get the files". Exercises the whole chat flow
 * with recorded MCP responses; no network, account or app is involved.
 */
import {
  downloadCloudDriveShare,
  extractionCodeFrom,
  formatBytes,
} from "../src/services/cloudDrive";

function check(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL ${message}`);
  console.log(`✓ ${message}`);
}

const ok = (value: unknown) => ({
  success: true,
  result: { result: { content: [{ type: "text", text: JSON.stringify(value) }] } },
});

const baiduShare = {
  provider: "baidu",
  providerName: "百度网盘",
  canonicalUrl: "https://pan.baidu.com/s/1SafeShare_1",
  extractionCode: "a1b2",
};
const receipts = {
  receipts: [
    { path: "/Users/u/Downloads/AI-OS/1SafeShare_1/Aimv001/video.mp4", bytes: 213_835_539, sha256: "a".repeat(64), integrityVerified: true },
    { path: "/Users/u/Downloads/AI-OS/1SafeShare_1/Aimv001/readme.txt", bytes: 623, sha256: "b".repeat(64), integrityVerified: true },
  ],
};

function harness(overrides: Partial<Record<string, unknown>> = {}) {
  const log: string[] = [];
  let status = { installed: true, signedIn: true, ...(overrides.bdpan as object | undefined) };
  const calls = {
    inspect: async (url: string, code?: string) => {
      log.push(`inspect ${url} ${code ?? "-"}`);
      return ok(overrides.share ?? baiduShare);
    },
    downloadBaidu: async (url: string, code: string) => {
      log.push(`download ${url} ${code}`);
      // Tauri rejects a failed command with the error string itself.
      if (overrides.downloadThrows) throw overrides.downloadThrows as string;
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    bdpanStatus: async () => status,
    installBdpan: async () => {
      log.push("install bdpan");
      status = { installed: true, signedIn: false };
      return status;
    },
    loginBdpan: async () => {
      log.push("login bdpan");
      status = { installed: true, signedIn: true };
      return status;
    },
  };
  const answers = [...((overrides.answers as boolean[] | undefined) ?? [])];
  const prompts: string[] = [];
  const confirm = async ({ title }: { title: string; message: string; confirmLabel: string }) => {
    prompts.push(title);
    return answers.shift() ?? false;
  };
  return { calls, confirm, log, prompts };
}

check(extractionCodeFrom("链接 https://pan.baidu.com/s/1x 提取码: ab12") === "ab12", "extraction code is read from Chinese text");
check(extractionCodeFrom("https://pan.baidu.com/s/1x pwd=zz99") === "zz99", "extraction code is read from pwd=");
check(formatBytes(623) === "623 B" && formatBytes(213_835_539) === "204 MB", "sizes are human readable");

{
  const h = harness();
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "下载这个", h.confirm, h.calls);
  check(reply.startsWith("已下载 2 个文件"), "a Baidu share downloads every file with no confirmation step");
  check(reply.includes("Aimv001/readme.txt") && reply.includes("已校验"), "the reply lists saved paths and verification");
  check(h.log.join("|") === "inspect https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2 -|download https://pan.baidu.com/s/1SafeShare_1 a1b2", "inspect → one automatic download call");
}
{
  const h = harness({ downloadThrows: "百度登录窗口被关闭，下载已取消。" });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply === "下载没有完成：百度登录窗口被关闭，下载已取消。", "closing the sign-in window is reported, not swallowed");
}
{
  const h = harness({ downloadThrows: "获取下载地址失败（errno 113）" });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply.includes("errno 113"), "provider errors reach the user");
}
{
  // Owner share 2026-10-17: Baidu refuses the 204 MB file as a direct share
  // download; the small file must still arrive and the refusal be explained.
  const h = harness({ downloadResult: {
    receipts: [receipts.receipts[1]],
    skipped: [{ name: "Aimv001/video.mp4", bytes: 213_835_539, reason: "百度要求这个文件先保存到网盘才能下载（文件较大）" }],
  } as never });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply.startsWith("已下载 1 个文件") && reply.includes("另有1 个文件没有下载") && reply.includes("先保存到网盘"), "a partly downloadable share downloads what it can and explains the rest");
}
{
  // Option 1: large files go through the user's drive and the folder is removed.
  const h = harness({ downloadResult: { ...receipts, viaDrive: 1, driveFolder: "/AI-OS 临时下载/1-2", driveFolderDeleted: true } as never });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply.startsWith("已下载 2 个文件") && reply.includes("先转存到你的网盘再下载") && reply.includes("临时文件夹已自动删除"), "files saved through the drive are reported and the folder removal confirmed");
}
{
  const h = harness({ downloadResult: { ...receipts, viaDrive: 0, driveFolder: "/AI-OS 临时下载/1-2", driveFolderDeleted: false } as never });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply.includes("仍保留在网盘") && reply.includes("/AI-OS 临时下载/1-2"), "a kept temporary folder is named so the user can remove it");
}
{
  const h = harness({ downloadResult: { receipts: [] } });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(reply.includes("没有返回任何文件"), "an empty result is not reported as success");
}
{
  const h = harness({ share: { ...baiduShare, extractionCode: undefined } });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1", "下载", h.confirm, h.calls);
  check(reply.includes("没有找到提取码") && !h.log.some((l) => l.startsWith("download")), "a missing extraction code is asked for");
}
{
  const h = harness({ share: { provider: "quark", providerName: "夸克网盘", canonicalUrl: "https://pan.quark.cn/s/x" } });
  const reply = await downloadCloudDriveShare("https://pan.quark.cn/s/x", "", h.confirm, h.calls);
  check(reply.includes("夸克网盘") && reply.includes("还在开发中") && !h.log.some((l) => l.startsWith("download")), "other providers are recognised and reported honestly");
}
{
  // Owner request 2026-10-18: no bdpan → offer to install, then authorize once.
  const h = harness({ bdpan: { installed: false, signedIn: false }, answers: [true, true] });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(h.prompts.join("|") === "安装百度网盘官方工具？|授权 AI-OS 使用百度网盘", "a missing bdpan is offered for install, then for sign-in");
  check(h.log.includes("install bdpan") && h.log.includes("login bdpan") && reply.startsWith("已下载 2 个文件"), "after install and sign-in the share downloads");
}
{
  const h = harness({ bdpan: { installed: false, signedIn: false }, answers: [false] });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(!h.log.includes("install bdpan") && reply.startsWith("没有安装 bdpan") && h.log.some((l) => l.startsWith("download")), "declining the install still downloads what Baidu allows, and says so");
}
{
  const h = harness({ bdpan: { installed: true, signedIn: false }, answers: [false] });
  const reply = await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(h.prompts.length === 1 && !h.log.includes("login bdpan") && reply.startsWith("没有授权 bdpan"), "declining sign-in is respected and reported");
}
{
  const h = harness();
  await downloadCloudDriveShare("https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2", "", h.confirm, h.calls);
  check(h.prompts.length === 0, "an installed, signed-in bdpan asks nothing");
}
console.log("PASS cloud drive download flow");
