import { invoke } from "@tauri-apps/api/core";

/** The built-in Linco connector (AI-OS plays linco-connect's role itself). */
export type LincoConnectorState = "not-paired" | "connecting" | "connected" | "reconnecting";

export type LincoConnectorStatus = {
  state: LincoConnectorState;
  agentType: string | null;
  deviceName: string;
  lastError: string | null;
  connectedAt: string | null;
};

export function getLincoConnectorStatus(): Promise<LincoConnectorStatus> {
  return invoke<LincoConnectorStatus>("get_linco_connector_status");
}

export const LINCO_AGENT_SLOTS = [
  { id: "openclaw", label: "OpenClaw" },
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code" },
  { id: "deepseek", label: "DeepSeek Harness" },
  { id: "hermes", label: "Hermes" },
] as const;

/**
 * `pairing` is what the Linco App hands out: the copied configuration, the
 * App ID / App Secret pair, or generated commands. `agentType` is the slot
 * chosen in "创建机器人" when the text does not name one.
 */
export function pairLincoConnector(pairing: string, agentType: string): Promise<LincoConnectorStatus> {
  return invoke<LincoConnectorStatus>("pair_linco_connector", { commands: pairing, agentType });
}

export function unpairLincoConnector(): Promise<LincoConnectorStatus> {
  return invoke<LincoConnectorStatus>("unpair_linco_connector");
}

export const LINCO_STATE_LABELS: Record<LincoConnectorState, string> = {
  "not-paired": "Not paired",
  connecting: "Connecting…",
  connected: "Connected",
  reconnecting: "Reconnecting…",
};
