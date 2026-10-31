import { createRemoteAiOsGateway, RemoteAiOsGateway } from "./remoteAiOs";
import { LincoRemoteTransportAdapter } from "./integrations/lincoRemoteTransportAdapter";
import type { RemoteAiOsEvent } from "../types/remoteInteraction";

export class LincoRemoteBridge {
  constructor(
    private readonly transport = new LincoRemoteTransportAdapter(),
    private readonly gateway: Pick<RemoteAiOsGateway, "receive"> = createRemoteAiOsGateway(),
  ) {}

  async handleInbound(value: unknown): Promise<RemoteAiOsEvent[]> {
    return this.gateway.receive(this.transport.normalizeInbound(value));
  }
}

export function createLincoRemoteBridge(): LincoRemoteBridge {
  return new LincoRemoteBridge();
}
