import { createRemoteAiOsGateway, RemoteAiOsGateway } from "./remoteAiOs";
import { LincoBridgeAdapter } from "./integrations/lincoBridgeAdapter";
import { LincoRemoteTransportAdapter } from "./integrations/lincoRemoteTransportAdapter";
import type { RemoteAiOsEvent } from "../types/remoteInteraction";

export class LincoRemoteBridge {
  constructor(
    private readonly transport = new LincoRemoteTransportAdapter(),
    private readonly gateway: Pick<RemoteAiOsGateway, "receive"> = createRemoteAiOsGateway(),
    private readonly linco = new LincoBridgeAdapter(),
  ) {}

  async handleInbound(value: unknown): Promise<RemoteAiOsEvent[]> {
    return this.gateway.receive(this.transport.normalizeInbound(value));
  }

  async resumeTransportSession(sessionId: string): Promise<{
    transport: unknown;
    messages: unknown[];
  }> {
    const transport = await this.linco.resumeSession(sessionId);
    const messages = await this.linco.listMessages(sessionId);
    return { transport, messages };
  }
}

export function createLincoRemoteBridge(): LincoRemoteBridge {
  return new LincoRemoteBridge();
}
