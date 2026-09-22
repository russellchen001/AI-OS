import type {
  CouncilIntegrationId,
  CouncilIntegrationProbe,
} from "../types/councilIntegrations";
import { PaperclipAdapter } from "./integrations/paperclipAdapter";
import { AgencyAgentsAdapter } from "./integrations/agencyAgentsAdapter";
import { LincoBridgeAdapter } from "./integrations/lincoBridgeAdapter";

export type CouncilIntegrationAdapter = {
  readonly id: CouncilIntegrationId;
  readonly name: string;
  probe(): Promise<CouncilIntegrationProbe>;
};

function createAdapters(): CouncilIntegrationAdapter[] {
  return [
    new PaperclipAdapter(),
    new AgencyAgentsAdapter(),
    new LincoBridgeAdapter(),
  ];
}

export function listCouncilIntegrations(): CouncilIntegrationAdapter[] {
  return createAdapters();
}

export function getCouncilIntegration(
  id: CouncilIntegrationId,
): CouncilIntegrationAdapter {
  const adapter = createAdapters().find((candidate) => candidate.id === id);

  if (!adapter) {
    throw new Error(`Unknown Council integration: ${id}`);
  }

  return adapter;
}

export async function probeCouncilIntegrations(): Promise<
  CouncilIntegrationProbe[]
> {
  return Promise.all(createAdapters().map((adapter) => adapter.probe()));
}

export { PaperclipAdapter, AgencyAgentsAdapter, LincoBridgeAdapter };
