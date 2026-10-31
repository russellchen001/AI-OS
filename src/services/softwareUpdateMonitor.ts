import { checkCliProviderVersions, type CliVersionReport } from "./cliProviderManager";
import { listAgentLifecycleAdapters, type AgentVersionPlan } from "./agentLifecycleManager";

/**
 * One daily, read-only check of every managed CLI provider and agent, run at
 * app launch instead of only when a page happens to open. Nothing is
 * installed here: updates always go through an approved plan.
 */
export type SoftwareUpdateSnapshot = {
  checkedAt: number;
  cli: CliVersionReport[];
  agents: AgentVersionPlan[];
};

export const SOFTWARE_UPDATES_EVENT = "ai-os:software-updates";
const STORAGE_KEY = "ai-os.software-updates";
export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export function loadSoftwareUpdates(): SoftwareUpdateSnapshot | undefined {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as SoftwareUpdateSnapshot | null;
    return value && typeof value.checkedAt === "number" ? value : undefined;
  } catch {
    return undefined;
  }
}

function save(snapshot: SoftwareUpdateSnapshot) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // The check still reports in this session.
  }
  window.dispatchEvent(new CustomEvent(SOFTWARE_UPDATES_EVENT, { detail: snapshot }));
}

export function isUpdateCheckDue(snapshot = loadSoftwareUpdates(), now = Date.now()): boolean {
  return !snapshot || now - snapshot.checkedAt >= UPDATE_CHECK_INTERVAL_MS;
}

/** Names of everything with an update, e.g. ["Gemini CLI 0.66.0"]. */
export function availableUpdates(snapshot: SoftwareUpdateSnapshot): string[] {
  return [
    ...snapshot.cli
      .filter((report) => report.updateAvailable)
      .map((report) => `${report.displayName} ${report.latestVersion}`),
    ...snapshot.agents
      .filter((plan) => plan.updateAvailable)
      .map((plan) => `${plan.agentId} ${plan.latestVersion}`),
  ];
}

let running: Promise<SoftwareUpdateSnapshot> | undefined;

export function checkSoftwareUpdates(): Promise<SoftwareUpdateSnapshot> {
  running ??= (async () => {
    const agentChecks = listAgentLifecycleAdapters()
      .filter((adapter) => adapter.versionManaged && adapter.checkVersion)
      .map((adapter) => adapter.checkVersion!().catch(() => undefined));
    const [cli, agents] = await Promise.all([
      checkCliProviderVersions().catch(() => [] as CliVersionReport[]),
      Promise.all(agentChecks),
    ]);
    const snapshot = {
      checkedAt: Date.now(),
      cli,
      agents: agents.filter((plan): plan is AgentVersionPlan => Boolean(plan)),
    };
    save(snapshot);
    return snapshot;
  })().finally(() => {
    running = undefined;
  });
  return running;
}

/** The launch-time check: only when the last one is a day old. */
export async function checkSoftwareUpdatesIfDue(): Promise<SoftwareUpdateSnapshot | undefined> {
  return isUpdateCheckDue() ? checkSoftwareUpdates() : undefined;
}
