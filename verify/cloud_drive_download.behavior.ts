/*
 * Beta 5: "send a share link, get the files". Exercises the whole chat flow
 * with recorded MCP responses; no network, account or app is involved.
 */
import {
  cloudDriveProgressText,
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
    downloadQuark: async (url: string, code: string) => {
      log.push(`quark ${url} ${code || "-"}`);
      if (overrides.downloadThrows) throw overrides.downloadThrows as string;
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    downloadUc: async (url: string, code: string) => {
      log.push(`uc ${url} ${code || "-"}`);
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    download123: async (url: string, code: string) => {
      log.push(`pan123 ${url} ${code || "-"}`);
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    download189: async (url: string, code: string) => {
      log.push(`189 ${url} ${code || "-"}`);
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    download139: async (url: string, code: string) => {
      log.push(`139 ${url} ${code || "-"}`);
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    downloadXunlei: async (url: string, code: string) => {
      log.push(`xunlei ${url} ${code || "-"}`);
      return (overrides.downloadResult as typeof receipts) ?? receipts;
    },
    downloadAliyun: async (url: string, code: string) => {
      log.push(`aliyun ${url} ${code || "-"}`);
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
  const h = harness({ share: { provider: "guangya", providerName: "光鸭云盘", canonicalUrl: "https://www.guangyapan.com/s/x" } });
  const reply = await downloadCloudDriveShare("https://www.guangyapan.com/s/x", "", h.confirm, h.calls);
  check(reply.includes("光鸭云盘") && reply.includes("还在开发中") && !h.log.some((l) => /^(download|quark|uc|aliyun|pan123|189|139|xunlei) /.test(l)), "providers not yet built are recognised and reported honestly");
}
{
  const h = harness({ share: { provider: "xunlei", providerName: "迅雷云盘", canonicalUrl: "https://pan.xunlei.com/s/VN_t0dUikZqOwt-5DZWHuMvqA1?pwd=66ep", extractionCode: "66ep" } });
  const reply = await downloadCloudDriveShare("https://pan.xunlei.com/s/VN_t0dUikZqOwt-5DZWHuMvqA1?pwd=66ep", "", h.confirm, h.calls);
  check(h.log.at(-1) === "xunlei https://pan.xunlei.com/s/VN_t0dUikZqOwt-5DZWHuMvqA1?pwd=66ep 66ep" && reply.startsWith("已下载 2 个文件"), "a 迅雷 share goes to the 迅雷 download with its pass code");
}
{
  const h = harness({
    share: { provider: "xunlei", providerName: "迅雷云盘", canonicalUrl: "https://pan.xunlei.com/s/VMkCJx-bOnpF431_9R0E8vAsA1?pwd=ugha", extractionCode: "ugha" },
    downloadResult: { receipts: [{ path: "/Users/u/Downloads/fish-dataset.zip", bytes: 227_480_946, sha256: "c".repeat(64), integrityVerified: true }], handedToThunder: true } as never,
  });
  const reply = await downloadCloudDriveShare("https://pan.xunlei.com/s/VMkCJx-bOnpF431_9R0E8vAsA1?pwd=ugha", "", h.confirm, h.calls);
  check(reply.startsWith("已下载 1 个文件") && reply.includes("交给迅雷客户端下载") && reply.includes("取回"), "files handed to Thunder are reported with the 取回 step");
}
{
  const h = harness({ share: { provider: "china-telecom", providerName: "天翼云盘", canonicalUrl: "https://cloud.189.cn/t/Ef2IjyuuemIn" } });
  await downloadCloudDriveShare("https://cloud.189.cn/t/Ef2IjyuuemIn", "https://cloud.189.cn/t/Ef2IjyuuemIn 访问码：jj8t", h.confirm, h.calls);
  check(h.log.at(-1) === "189 https://cloud.189.cn/t/Ef2IjyuuemIn jj8t", "a 天翼 share passes its access code to the 天翼 download");
}
{
  const h = harness({ share: { provider: "china-mobile", providerName: "移动云盘", canonicalUrl: "https://yun.139.com/shareweb/#/w/i/2u8omYzvLhqdo" } });
  await downloadCloudDriveShare("https://yun.139.com/shareweb/#/w/i/2u8omYzvLhqdo", "https://yun.139.com/shareweb/#/w/i/2u8omYzvLhqdo (提取码:3dgn)", h.confirm, h.calls);
  check(h.log.at(-1) === "139 https://yun.139.com/shareweb/#/w/i/2u8omYzvLhqdo 3dgn", "a 移动 share passes its code to the 移动 download");
}
{
  const h = harness({ share: { provider: "pan123", providerName: "123 云盘", canonicalUrl: "https://www.123pan.com/s/uAxvTd-8229d" } });
  const reply = await downloadCloudDriveShare("https://www.123pan.com/s/uAxvTd-8229d", "", h.confirm, h.calls);
  check(h.log.at(-1) === "pan123 https://www.123pan.com/s/uAxvTd-8229d -" && reply.startsWith("已下载 2 个文件") && h.prompts.length === 0, "a 123 share downloads with no questions");
}
{
  const h = harness({
    share: { provider: "uc", providerName: "UC 网盘", canonicalUrl: "https://drive.uc.cn/s/0be7077c38654" },
    downloadResult: { ...receipts, viaDrive: 2, driveFolder: "/AI-OS 临时下载/0be7077c38654-1", driveFolderDeleted: true } as never,
  });
  const reply = await downloadCloudDriveShare("https://drive.uc.cn/s/0be7077c38654?public=1", "", h.confirm, h.calls);
  check(h.log.at(-1) === "uc https://drive.uc.cn/s/0be7077c38654 -" && reply.includes("UC 网盘回收站"), "a UC share goes to the UC download and reports its cleanup");
}
{
  // Aliyun downloads straight from the share: no drive folder to report.
  const h = harness({ share: { provider: "aliyun", providerName: "阿里云盘", canonicalUrl: "https://www.alipan.com/s/SwCbQBN2nZQ" } });
  const reply = await downloadCloudDriveShare("https://www.alipan.com/s/SwCbQBN2nZQ", "下载", h.confirm, h.calls);
  check(h.log.join("|") === "inspect https://www.alipan.com/s/SwCbQBN2nZQ -|aliyun https://www.alipan.com/s/SwCbQBN2nZQ -", "an Aliyun share goes straight to the Aliyun download");
  check(h.prompts.length === 0 && reply.startsWith("已下载 2 个文件") && !reply.includes("临时文件夹"), "Aliyun downloads report files without a drive folder");
}
{
  // Quark needs no extraction code for open shares and no bdpan questions.
  const h = harness({
    share: { provider: "quark", providerName: "夸克网盘", canonicalUrl: "https://pan.quark.cn/s/97e0db6659eb" },
    downloadResult: { ...receipts, viaDrive: 2, driveFolder: "/AI-OS 临时下载/97e0db6659eb-1", driveFolderDeleted: true } as never,
  });
  const reply = await downloadCloudDriveShare("https://pan.quark.cn/s/97e0db6659eb", "下载", h.confirm, h.calls);
  check(h.log.join("|") === "inspect https://pan.quark.cn/s/97e0db6659eb -|quark https://pan.quark.cn/s/97e0db6659eb -", "a Quark share goes straight to the Quark download");
  check(h.prompts.length === 0 && reply.startsWith("已下载 2 个文件") && reply.includes("夸克网盘回收站"), "Quark downloads report files and the removed temporary folder");
}
{
  const h = harness({ share: { provider: "quark", providerName: "夸克网盘", canonicalUrl: "https://pan.quark.cn/s/x" } });
  await downloadCloudDriveShare("https://pan.quark.cn/s/x", "链接 https://pan.quark.cn/s/x 提取码：Ab12", h.confirm, h.calls);
  check(h.log.includes("quark https://pan.quark.cn/s/x Ab12"), "a Quark extraction code from the message is passed on");
}
{
  const h = harness({ share: { provider: "quark", providerName: "夸克网盘", canonicalUrl: "https://pan.quark.cn/s/x" }, downloadThrows: "夸克网盘登录窗口被关闭，下载已取消。" });
  const reply = await downloadCloudDriveShare("https://pan.quark.cn/s/x", "", h.confirm, h.calls);
  check(reply === "下载没有完成：夸克网盘登录窗口被关闭，下载已取消。", "closing the Quark sign-in window is reported");
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
check(cloudDriveProgressText({}) === "正在处理网盘链接…", "progress starts with a neutral line");
check(
  cloudDriveProgressText({ stage: "正在转存到你的网盘", bytes: 31_358_097 }) === "正在转存到你的网盘…\n\n已下载 29.9 MB",
  "progress shows the stage and bytes so far",
);
check(
  cloudDriveProgressText({ stage: "正在下载第 3/12 个文件（85.8 MB）", bytes: 50_438_144, totalBytes: 166_223_438 }) ===
    "正在下载第 3/12 个文件（85.8 MB）…\n\n已下载 48.1 MB / 159 MB",
  "progress shows the file position and bytes of the total",
);
console.log("PASS cloud drive download flow");
