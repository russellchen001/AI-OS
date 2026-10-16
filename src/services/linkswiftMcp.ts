import { invoke } from "@tauri-apps/api/core";
import type { LinkSwiftMcpActionResult, LinkSwiftMcpStatus } from "../types/index";

const SUPPORTED_SHARE_HOSTS = new Set([
  "123684.com",
  "123pan.com",
  "alipan.com",
  "aliyundrive.com",
  "caiyun.139.com",
  "cloud.189.cn",
  "drive.uc.cn",
  "pan.baidu.com",
  "pan.quark.cn",
  "pan.xunlei.com",
  "yun.baidu.com",
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

export function linkSwiftConsentMessage(status: LinkSwiftMcpStatus): string {
  return [
    `${status.name} ${status.pinnedVersion}`,
    `License: ${status.license}`,
    `Corresponding source: ${status.sourceUrl}`,
    "",
    "Permissions requested:",
    ...status.permissions.map((permission) => `• ${permission}`),
    "",
    "Cloud-drive providers may still require sign-in, CAPTCHA or file access approval. Installation does not promise anonymous access or support for every link.",
  ].join("\n");
}

export async function getLinkSwiftMcpStatus(): Promise<LinkSwiftMcpStatus> {
  return invoke<LinkSwiftMcpStatus>("get_linkswift_mcp_status");
}

export async function installLinkSwiftMcp(approved: boolean): Promise<LinkSwiftMcpActionResult> {
  return invoke<LinkSwiftMcpActionResult>("install_linkswift_mcp", { approved });
}

export async function updateLinkSwiftMcp(approved: boolean): Promise<LinkSwiftMcpActionResult> {
  return invoke<LinkSwiftMcpActionResult>("update_linkswift_mcp", { approved });
}

export async function uninstallLinkSwiftMcp(approved: boolean): Promise<LinkSwiftMcpActionResult> {
  return invoke<LinkSwiftMcpActionResult>("uninstall_linkswift_mcp", { approved });
}

type McpCallResult = {
  success: boolean;
  result?: unknown;
  error?: string;
};

export async function resolveLinkSwiftShareLink(url: string, requestText: string): Promise<McpCallResult> {
  return invoke<McpCallResult>("call_linkswift_mcp", {
    toolName: "resolve_share_link",
    arguments: { url, requestText },
  });
}

export function describeLinkSwiftResult(response: McpCallResult): string {
  if (!response.success) {
    return response.error || "LinkSwift MCP could not process this share link.";
  }
  const rpc = response.result as { result?: { content?: Array<{ text?: unknown }> } } | undefined;
  const text = rpc?.result?.content
    ?.map((item) => typeof item.text === "string" ? item.text : "")
    .filter(Boolean)
    .join("\n");
  return text || "LinkSwift MCP completed without a readable result.";
}
