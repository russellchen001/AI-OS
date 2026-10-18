/*
 * Beta 5 built-in cloud-drive download: a share link sent in chat is
 * recognised here and downloaded by the backend (see src-tauri/src/cloud_drive.rs).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const SUPPORTED_SHARE_HOSTS = new Set([
  "123684.com",
  "123pan.com",
  "123pan.cn",
  "alipan.com",
  "aliyundrive.com",
  "caiyun.139.com",
  "yun.139.com",
  "cloud.189.cn",
  "drive.uc.cn",
  "guangyapan.com",
  "pan.baidu.com",
  "pan.quark.cn",
  "pan.xunlei.com",
  "yun.baidu.com",
  "www.123pan.cn",
  "www.123pan.com",
  "www.alipan.com",
  "www.aliyundrive.com",
  "www.guangyapan.com",
]);

export function supportedCloudDriveShareLink(value: string): string | undefined {
  const candidates = value.match(/https:\/\/[^\s<>()\[\]】）]+/giu) ?? [];
  for (const candidate of candidates) {
    const normalized = candidate.replace(/[，。；;、]+$/u, "");
    try {
      const url = new URL(normalized);
      const host = url.hostname.toLowerCase();
      if (SUPPORTED_SHARE_HOSTS.has(host)) return url.toString();
    } catch {
      // Ignore malformed text and leave ordinary Chat routing unchanged.
    }
  }
  return undefined;
}

type McpCallResult = {
  success: boolean;
  result?: unknown;
  error?: string;
};

export function extractionCodeFrom(requestText: string): string | undefined {
  return requestText.match(/(?:提取码|密码|访问码|pwd)\s*[:：=]?\s*([A-Za-z0-9_-]{4,32})/iu)?.[1];
}

/** The tool's JSON payload, or its error text. */
export function mcpToolPayload(response: McpCallResult): { ok: true; value: unknown } | { ok: false; error: string } {
  if (!response.success) {
    return { ok: false, error: response.error || "网盘组件没有返回结果。" };
  }
  const rpc = response.result as { result?: { isError?: boolean; content?: Array<{ text?: unknown }> } } | undefined;
  const text = rpc?.result?.content
    ?.map((item) => (typeof item.text === "string" ? item.text : ""))
    .join("") ?? "";
  if (rpc?.result?.isError) return { ok: false, error: text || "网盘组件报告失败。" };
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch {
    return { ok: false, error: text || "网盘组件返回了无法读取的结果。" };
  }
}

type ReceiptJson = { path: string; bytes: number; sha256: string; integrityVerified: boolean };
type SkippedJson = { name: string; bytes: number; reason: string };
type BaiduDownloadResult = {
  receipts: ReceiptJson[];
  skipped?: SkippedJson[];
  viaDrive?: number;
  driveFolder?: string | null;
  driveFolderDeleted?: boolean;
  /** 迅雷: files Xunlei keeps from guests were saved to the user's drive and handed to the Thunder app. */
  handedToThunder?: boolean;
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unit]}`;
}

/**
 * The whole "send a share link, get the files" flow, with no confirmation:
 * AI-OS recognises the link and downloads every file to
 * ~/Downloads/AI-OS/<share>. For Baidu the backend signs in with AI-OS's saved
 * session, or shows the Baidu window and continues by itself once the user has
 * signed in. Tests pass their own `calls`.
 */
export type ShareDownloadConfirm = (input: {
  title: string;
  message: string;
  confirmLabel: string;
}) => Promise<boolean>;

export type BdpanStatus = {
  installed: boolean;
  signedIn: boolean;
  version?: string | null;
  tokenExpiresAt?: string | null;
};

/** Baidu's own safety notice for bdpan (skills/baidu-drive/scripts/login.sh). */
export const BDPAN_SAFETY_NOTICE = [
  "百度网盘官方工具 bdpan 的安全须知：",
  "1. 请备份网盘重要数据，谨慎操作。",
  "2. AI Agent 的行为具有不可预测性，请留意执行过程，并对执行后果负责。",
  "3. 严禁在他人、公用或不可信的电脑上授权，以免网盘数据被窃取。",
  "4. 请保护好配置文件与令牌，不要在公开仓库或对话中暴露。",
].join("\n");

export async function downloadCloudDriveShare(
  url: string,
  requestText: string,
  confirm: ShareDownloadConfirm,
  calls = {
    inspect: (shareUrl: string, code?: string) =>
      invoke<McpCallResult>("call_cloud_drive_mcp", {
        toolName: "inspect_share_link",
        arguments: { url: shareUrl, extractionCode: code },
      }),
    downloadBaidu: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_baidu_share_automatically", { url: shareUrl, extractionCode: code }),
    downloadQuark: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_quark_share_automatically", { url: shareUrl, extractionCode: code }),
    downloadUc: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_uc_share_automatically", { url: shareUrl, extractionCode: code }),
    download123: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_123_share_automatically", { url: shareUrl, extractionCode: code }),
    download189: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_189_share_automatically", { url: shareUrl, extractionCode: code }),
    download139: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_139_share_automatically", { url: shareUrl, extractionCode: code }),
    downloadXunlei: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_xunlei_share_automatically", { url: shareUrl, extractionCode: code }),
    downloadAliyun: (shareUrl: string, code: string) =>
      invoke<BaiduDownloadResult>("download_aliyun_share_automatically", { url: shareUrl, extractionCode: code }),
    bdpanStatus: () => invoke<BdpanStatus>("cloud_drive_bdpan_status"),
    installBdpan: () => invoke<BdpanStatus>("cloud_drive_install_bdpan"),
    loginBdpan: () => invoke<BdpanStatus>("cloud_drive_bdpan_login"),
  },
): Promise<string> {
  const inspected = mcpToolPayload(await calls.inspect(url, extractionCodeFrom(requestText)));
  if (!inspected.ok) return `无法识别这个分享链接：${inspected.error}`;
  const share = inspected.value as { provider: string; providerName: string; canonicalUrl: string; extractionCode?: string };

  // Quark, UC, Aliyun, 123, 天翼, 移动 and 迅雷: the backend reads the share, then uses
  // the user's own sign-in when needed (asked for once, kept in AI-OS). Quark
  // and UC save through a temporary drive folder and remove it; the others
  // download from the share (123 even as a guest).
  const signedInDownloads: Record<string, { call: (shareUrl: string, code: string) => Promise<BaiduDownloadResult>; provider: string }> = {
    quark: { call: calls.downloadQuark, provider: "夸克" },
    uc: { call: calls.downloadUc, provider: "UC 网盘" },
    aliyun: { call: calls.downloadAliyun, provider: "阿里云盘" },
    pan123: { call: calls.download123, provider: "123 云盘" },
    "china-telecom": { call: calls.download189, provider: "天翼云盘" },
    "china-mobile": { call: calls.download139, provider: "移动云盘" },
    xunlei: { call: calls.downloadXunlei, provider: "迅雷云盘" },
  };
  const signedInDownload = signedInDownloads[share.provider];
  if (signedInDownload) {
    const optionalCode = share.extractionCode ?? extractionCodeFrom(requestText) ?? "";
    try {
      return downloadReply(await signedInDownload.call(share.canonicalUrl, optionalCode), signedInDownload.provider, share.providerName, []);
    } catch (error) {
      return `下载没有完成：${error instanceof Error ? error.message : String(error)}`;
    }
  }
  if (share.provider !== "baidu") {
    return `已识别为${share.providerName}分享链接。目前 AI-OS 已支持百度网盘、夸克网盘、UC 网盘、阿里云盘、123 云盘、天翼云盘、移动云盘和迅雷云盘分享下载，${share.providerName}还在开发中，暂时不能下载。`;
  }
  const code = share.extractionCode ?? extractionCodeFrom(requestText);
  if (!code) {
    return "这是百度网盘分享链接，但没有找到提取码。请把链接和提取码一起发给我，例如：`链接 提取码: abcd`。";
  }

  // Baidu's official CLI is the only route for large files. Offer to install
  // it and to sign it in, once; afterwards it renews its own token.
  const notes: string[] = [];
  const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));
  let status = await calls.bdpanStatus().catch(() => undefined);
  if (status && !status.installed) {
    const install = await confirm({
      title: "安装百度网盘官方工具？",
      message: [
        "下载大文件需要百度网盘官方命令行工具 bdpan，这台电脑还没有安装。",
        "",
        "AI-OS 会从百度官方服务器下载安装器，按百度官方仓库公布的 SHA-256 校验后安装到 ~/.local/bin/bdpan。",
        "",
        "不安装也可以继续，但只能下载百度允许直接下载的小文件。",
      ].join("\n"),
      confirmLabel: "安装",
    });
    if (install) {
      try {
        status = await calls.installBdpan();
      } catch (error) {
        notes.push(`bdpan 安装失败：${failure(error)}。本次只下载能直接下载的文件。`);
      }
    } else {
      notes.push("没有安装 bdpan，本次只下载能直接下载的文件。");
    }
  }
  if (status && status.installed && !status.signedIn) {
    const authorize = await confirm({
      title: "授权 AI-OS 使用百度网盘",
      message: [
        BDPAN_SAFETY_NOTICE,
        "",
        "点「同意并授权」后会打开百度授权页面，你只需要点「授权」（未登录百度时先登录）。授权一次后，AI-OS 会自动续期，不需要再次登录。",
      ].join("\n"),
      confirmLabel: "同意并授权",
    });
    if (authorize) {
      try {
        status = await calls.loginBdpan();
      } catch (error) {
        notes.push(`百度授权没有完成：${failure(error)}。本次只下载能直接下载的文件。`);
      }
    } else {
      notes.push("没有授权 bdpan，本次只下载能直接下载的文件。");
    }
  }

  try {
    return downloadReply(await calls.downloadBaidu(share.canonicalUrl, code), "百度", "百度网盘", notes);
  } catch (error) {
    return `下载没有完成：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** The chat reply for a finished download: files, skipped files, drive cleanup. */
function downloadReply(drive: BaiduDownloadResult, provider: string, driveName: string, notes: string[]): string {
  const receipts = drive.receipts ?? [];
  const skipped = drive.skipped ?? [];
  if (receipts.length === 0 && skipped.length === 0) return `下载没有完成：${provider}没有返回任何文件。`;
  const lines: string[] = [];
  if (receipts.length > 0) {
    const total = receipts.reduce((sum, receipt) => sum + receipt.bytes, 0);
    lines.push(
      `已下载 ${receipts.length} 个文件，共 ${formatBytes(total)}：`,
      "",
      ...receipts.map(
        (receipt) =>
          `- \`${receipt.path}\`（${formatBytes(receipt.bytes)}，SHA-256 ${receipt.sha256.slice(0, 12)}…${receipt.integrityVerified ? "，已校验" : ""}）`,
      ),
    );
  }
  if (skipped.length > 0) {
    if (lines.length > 0) lines.push("");
    lines.push(
      `${receipts.length > 0 ? "另有" : "下载没有完成："}${skipped.length} 个文件没有下载：`,
      "",
      ...skipped.map((item) => `- ${item.name}（${formatBytes(item.bytes)}）：${item.reason}`),
    );
  }
  if (drive.driveFolder) {
    lines.push(
      "",
      drive.driveFolderDeleted
        ? `这些文件按${provider}规则先转存到你的网盘再下载，临时文件夹已自动删除（可在${driveName}回收站找回）。`
        : `文件按${provider}规则经你的网盘中转，临时文件夹仍保留在网盘：\`${drive.driveFolder}\`，可以手动删除。`,
    );
  }
  if (drive.handedToThunder) {
    lines.push(
      "",
      "迅雷只允许游客直接下载部分类型的文件，其余文件已用你的迅雷账号转存到迅雷云盘的“我的云盘”，并交给迅雷客户端下载（迅雷会弹出“取回”提示，需要点一下）。转存的文件留在你的迅雷云盘里，不需要时可以删除。",
    );
  }
  if (notes.length > 0) lines.unshift(...notes, "");
  return lines.join("\n");
}

/** Live progress while a share downloads (backend event "cloud-drive-progress"). */
export type CloudDriveProgress = { stage?: string; bytes?: number; totalBytes?: number; fileCount?: number };

export function onCloudDriveProgress(callback: (progress: CloudDriveProgress) => void) {
  return listen<CloudDriveProgress>("cloud-drive-progress", (event) => callback(event.payload));
}

/** The temporary chat reply shown while a share downloads. */
export function cloudDriveProgressText(progress: CloudDriveProgress): string {
  const lines = [progress.stage ? `${progress.stage}…` : "正在处理网盘链接…"];
  if (progress.bytes && progress.bytes > 0) {
    lines.push(
      progress.totalBytes && progress.totalBytes > 0
        ? `已下载 ${formatBytes(Math.min(progress.bytes, progress.totalBytes))} / ${formatBytes(progress.totalBytes)}`
        : `已下载 ${formatBytes(progress.bytes)}`,
    );
  }
  return lines.join("\n\n");
}
