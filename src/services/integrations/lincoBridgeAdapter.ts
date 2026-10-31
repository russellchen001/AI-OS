import type { CouncilIntegrationProbe } from "../../types/councilIntegrations";
import { getLincoConnectorStatus, LINCO_STATE_LABELS } from "../lincoConnector";

/**
 * Linco is built into AI-OS: the native connector (src-tauri/src/linco_connector.rs)
 * speaks linco-bridge-connect's protocol to the official Linco channel, so no
 * separate Linco program or local server is involved.
 */
export const LINCO_CONNECTOR_SOURCE_COMMIT =
  "536a257dbce75de2e7000ff3b8bb01b010fb2dfe";

export class LincoBridgeAdapter {
  readonly id = "linco-bridge" as const;
  readonly name = "Linco (built-in)";

  async probe(): Promise<CouncilIntegrationProbe> {
    const checkedAt = new Date().toISOString();
    try {
      const status = await getLincoConnectorStatus();
      return {
        id: this.id,
        name: this.name,
        status: status.state === "connected" ? "available" : "unavailable",
        detail: status.lastError ?? LINCO_STATE_LABELS[status.state],
        checkedAt,
        metadata: {
          agentType: status.agentType,
          sourceCommit: LINCO_CONNECTOR_SOURCE_COMMIT,
        },
      };
    } catch (error) {
      return {
        id: this.id,
        name: this.name,
        status: "error",
        detail: error instanceof Error ? error.message : String(error),
        checkedAt,
        metadata: { sourceCommit: LINCO_CONNECTOR_SOURCE_COMMIT },
      };
    }
  }
}
