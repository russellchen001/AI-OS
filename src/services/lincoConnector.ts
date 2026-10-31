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

/** `commands` is the text copied from the Linco App's Bridge page. */
export function pairLincoConnector(commands: string): Promise<LincoConnectorStatus> {
  return invoke<LincoConnectorStatus>("pair_linco_connector", { commands });
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
