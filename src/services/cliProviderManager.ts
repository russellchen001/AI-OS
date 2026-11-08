import { invoke } from "@tauri-apps/api/core";
import type { ProviderModelEntry } from "../types/provider";

export type CliProviderStatus = {
  installed: boolean;
  authenticated: boolean;
  version?: string;
  message: string;
  models: string[];
  defaultModel?: string;
};

/**
 * One official account CLI. Everything shared (install, version checks,
 * updates, bounded requests) lives in the native CLI provider registry; an
 * adapter only describes the CLI to the setup screen.
 */
export type CliProviderAdapter = {
  id: "anthropic" | "grok" | "google";
  displayName: string;
  officialPackage?: string;
  loginCommand: string;
  /** Setup screen copy: title, how to sign in, and what AI-OS can access. */
  setupTitle: string;
  setupInstructions: string;
  protectionNote: string;
  probe: () => Promise<CliProviderStatus>;
  signIn?: (signal?: AbortSignal) => Promise<unknown>;
  /** A real request after the status check, when status alone can't prove sign-in. */
  verify?: () => Promise<void>;
  models: (instanceId: string, status: CliProviderStatus) => ProviderModelEntry[];
};

export type CliInstallMethod = "homebrew" | "ai-os-npm" | "npm-global" | "official-installer" | "native-download";

export type CliInstallPlan = {
  providerId: string;
  displayName: string;
  packageName: string;
  version: string;
  integrity: string;
  installedVersion?: string;
  action: "install" | "upgrade";
  method: CliInstallMethod;
  sourceLabel: string;
  requiresApproval: true;
};

export type CliVersionReport = {
  providerId: string;
  displayName: string;
  installed: boolean;
  installedVersion?: string;
  latestVersion?: string;
  updateAvailable: boolean;
  method: CliInstallMethod | "manual" | "none";
  sourceLabel: string;
  canUpdate: boolean;
  message: string;
};

export type CliInstallResult = {
  success: boolean;
  message: string;
  version?: string;
  loginCommand?: string;
};

function modelEntry(
  instanceId: string,
  remoteModelId: string,
  displayName: string,
  isDefault: boolean,
  capabilities: ProviderModelEntry["capabilities"],
): ProviderModelEntry {
  return {
    id: `${instanceId}:${remoteModelId}`,
    providerInstanceId: instanceId,
    remoteModelId,
    displayName,
    capabilities,
    enabled: true,
    isDefault,
  };
}

const adapters: CliProviderAdapter[] = [
  {
    id: "anthropic",
    displayName: "Claude Code",
    officialPackage: "@anthropic-ai/claude-code",
    loginCommand: "claude auth login",
    setupTitle: "Use Claude Code on this Mac",
    setupInstructions: "AI‑OS checks this Mac first. If Claude Code is missing, review and approve the official installation here. Then sign in on the official page opened by Claude Code. No terminal commands are needed.",
    protectionNote: "Claude Code owns authentication. AI‑OS only checks its status and sends bounded requests through the local CLI.",
    probe: async () => {
      const status = await invoke<Omit<CliProviderStatus, "models">>("get_claude_code_status");
      return { ...status, models: status.authenticated ? ["claude-code-default"] : [] };
    },
    signIn: async (signal) => {
      signal?.throwIfAborted();
      const operationId = crypto.randomUUID();
      const cancel = () => { void invoke("cancel_claude_code_request", { operationId }).catch(() => undefined); };
      signal?.addEventListener("abort", cancel, { once: true });
      try { await invoke("sign_in_claude_code", { operationId, userApproved: true }); }
      finally { signal?.removeEventListener("abort", cancel); }
    },
    models: (instanceId, status) => status.models.map((model) =>
      modelEntry(instanceId, model, "Claude Code account default", true, ["chat", "reasoning", "vision", "tool-use"])),
  },
  {
    id: "grok",
    displayName: "Grok CLI",
    officialPackage: "@xai-official/grok",
    loginCommand: "grok login",
    setupTitle: "Use the Grok CLI on this Mac",
    setupInstructions: "Install the official Grok CLI from x.ai/cli and run `grok login` in Terminal. AI‑OS never reads or stores its account credential.",
    protectionNote: "The Grok CLI owns authentication and keeps it current. AI‑OS only checks its status and sends bounded requests with tools and web access turned off.",
    probe: () => invoke<CliProviderStatus>("get_grok_cli_status"),
    models: (instanceId, status) => status.models.map((model, index) =>
      modelEntry(
        instanceId,
        model,
        `${model} (Grok CLI)`,
        model === (status.defaultModel ?? status.models[0]) || (index === 0 && !status.defaultModel),
        ["chat", "reasoning"],
      )),
  },
  {
    id: "google",
    displayName: "Gemini CLI",
    officialPackage: "@google/gemini-cli",
    loginCommand: "gemini",
    setupTitle: "Use the Gemini CLI with your Google account",
    // Google stopped serving personal accounts through Gemini CLI on
    // 2026-06-18; Code Assist Standard/Enterprise sign-ins and API keys
    // configured in the CLI still work, so the route stays for them.
    setupInstructions: "Run `gemini` in Terminal and sign in with a Gemini Code Assist Standard/Enterprise account or your own API key. Requests use your own Google quota, not anyone else's. Personal Google accounts are no longer served by Gemini CLI — use “Use API key” instead. AI‑OS never reads or stores the CLI credential.",
    protectionNote: "The Gemini CLI owns authentication. AI‑OS sends bounded headless requests with every tool, extension and MCP server turned off.",
    probe: () => invoke<CliProviderStatus>("get_gemini_cli_status"),
    verify: async () => {
      await invoke<boolean>("verify_gemini_cli_account");
    },
    models: (instanceId, status) => status.models.map((model) =>
      modelEntry(instanceId, model, "Gemini CLI account default", true, ["chat", "reasoning"])),
  },
];

/** Providers whose account sign-in is owned by their official local CLI. */
export const CLI_ACCOUNT_PROVIDER_IDS: ReadonlySet<string> = new Set(adapters.map((adapter) => adapter.id));

export function listCliProviderAdapters(): CliProviderAdapter[] {
  return [...adapters];
}

export function getCliProviderAdapter(providerId: string): CliProviderAdapter | undefined {
  return adapters.find((adapter) => adapter.id === providerId);
}

export function prepareCliProviderInstall(providerId: string): Promise<CliInstallPlan> {
  return invoke<CliInstallPlan>("prepare_cli_provider_install", { providerId });
}

export function installCliProvider(plan: CliInstallPlan): Promise<CliInstallResult> {
  return invoke<CliInstallResult>("install_cli_provider", {
    request: {
      providerId: plan.providerId,
      version: plan.version,
      integrity: plan.integrity,
      method: plan.method,
      userApproved: true,
    },
  });
}

/** The same consent and verification path used by the connection dialog. */
export async function connectCliProvider(
  adapter: CliProviderAdapter,
  approve: (plan: CliInstallPlan) => Promise<boolean>,
  progress: (message: string) => void = () => undefined,
  signal?: AbortSignal,
): Promise<CliProviderStatus> {
  progress(`Checking ${adapter.displayName} on this Mac…`);
  let status = await adapter.probe();
  signal?.throwIfAborted();
  if (!status.installed && adapter.officialPackage) {
    const plan = await prepareCliProviderInstall(adapter.id);
    signal?.throwIfAborted();
    if (!await approve(plan)) throw new Error("Installation cancelled. Nothing was installed. You can try again when ready.");
    signal?.throwIfAborted();
    progress(`Installing ${adapter.displayName}. Keep this window open; the download may take a few minutes…`);
    const installed = await installCliProvider(plan);
    if (!installed.success) throw new Error(`${installed.message} Check your internet connection and available disk space, then retry, or open the official setup help.`);
    status = await adapter.probe();
    signal?.throwIfAborted();
  }
  if (!status.installed) throw new Error(`${adapter.displayName} is not installed. Open the official setup help, then retry.`);
  if (!status.authenticated && adapter.signIn) {
    progress("Complete sign-in on the official page opened by Claude Code, then return here. AI‑OS will check the result.");
    await adapter.signIn(signal);
    signal?.throwIfAborted();
    status = await adapter.probe();
  }
  if (!status.authenticated) {
    // CLIs without an in-app sign-in (Grok, Gemini) are signed in by the user in
    // Terminal; their status message names the official command.
    throw new Error(adapter.signIn
      ? `${adapter.displayName} sign-in is not complete. Retry sign-in, open the official setup help, or use an API key instead.`
      : `${status.message} Then return here and retry, or choose API key above.`);
  }
  await adapter.verify?.();
  return status;
}

/** Installed and latest official versions of every registered CLI (read-only). */
export function checkCliProviderVersions(): Promise<CliVersionReport[]> {
  return invoke<CliVersionReport[]>("check_cli_provider_versions");
}

const METHOD_DESCRIPTIONS: Record<CliInstallMethod, string> = {
  "native-download": "from downloads.claude.ai using Anthropic's native installer, after checking its SHA-256 and Apple developer signature. No Node.js, Homebrew or administrator password is required. The official installer manages its own updates; Claude Code keeps its own sign-in credentials",
  homebrew: "with Homebrew (`brew upgrade`), the way it was installed",
  "ai-os-npm": "from the official npm registry into AI‑OS's own folder after rechecking its published SHA-512 integrity; the previous version is kept for rollback",
  "npm-global": "with npm from the official registry, the way it was installed",
  "official-installer": "with the vendor's own updater, the way it was installed",
};

/** The approval text for one install or update plan. */
export function describeCliInstallPlan(plan: CliInstallPlan): string {
  const change = plan.action === "upgrade" && plan.installedVersion
    ? `update ${plan.displayName} from ${plan.installedVersion} to ${plan.version}`
    : `install ${plan.displayName} ${plan.version}`;
  return `AI‑OS will ${change} ${METHOD_DESCRIPTIONS[plan.method]}. Requests to ${plan.displayName} pause until it finishes.`;
}
