import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createLincoRemoteBridge, type LincoRemoteBridge } from "./lincoRemoteBridge";
import type { RemoteAiOsEvent } from "../types/remoteInteraction";

type DesktopInboundPayload = {
  requestId: string;
  envelope: unknown;
};

type DesktopBridge = Pick<LincoRemoteBridge, "handleInbound">;

export async function handleDesktopLincoInbound(
  payload: DesktopInboundPayload,
  bridge: DesktopBridge,
): Promise<RemoteAiOsEvent[]> {
  if (!payload || typeof payload.requestId !== "string" || !payload.requestId.trim()) {
    throw new Error("Desktop Linco request id is required.");
  }
  return bridge.handleInbound(payload.envelope);
}

let startPromise: Promise<void> | undefined;

export function startDesktopLincoInboundBridge(): Promise<void> {
  if (startPromise) return startPromise;
  const bridge = createLincoRemoteBridge();
  startPromise = listen<DesktopInboundPayload>("remote-linco://inbound", (event) => {
    void handleDesktopLincoInbound(event.payload, bridge)
      .then((events) => invoke("complete_remote_linco_request", {
        requestId: event.payload.requestId,
        events,
        error: null,
      }))
      .catch((error: unknown) => invoke("complete_remote_linco_request", {
        requestId: event.payload.requestId,
        events: null,
        error: error instanceof Error ? error.message : "Remote request failed.",
      }));
  }).then(() => undefined);
  return startPromise;
}
