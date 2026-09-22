import type { RemoteInboundEvent, RemoteMode } from "../../types/remoteInteraction";

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function requiredString(value: unknown, label: string): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`Linco ${label} is required.`);
  }
  return value.trim();
}

function mode(value: unknown): RemoteMode | undefined {
  return value === "auto" || value === "council" || value === "simulation"
    ? value
    : undefined;
}

export class LincoRemoteTransportAdapter {
  normalizeInbound(value: unknown): RemoteInboundEvent {
    const input = record(value);
    if (!input) throw new Error("Linco inbound event is invalid.");
    const type = requiredString(input.type, "event type");
    const sessionKey = requiredString(input.sessionKey, "sessionKey");

    if (type === "inbound_message") {
      const content = record(input.message) ?? input;
      return {
        type,
        sessionKey,
        messageId: requiredString(
          input.messageId ?? content.id,
          "message id",
        ),
        text: requiredString(content.text ?? content.content, "message text"),
        mode: mode(input.mode ?? content.mode),
        profileId:
          typeof (input.profileId ?? content.profileId) === "string"
            ? String(input.profileId ?? content.profileId).trim() || undefined
            : undefined,
      };
    }

    if (type === "danger_confirm" || type === "permission_response") {
      return {
        type,
        sessionKey,
        confirmationId: requiredString(
          input.confirmationId ?? input.requestId,
          "confirmation id",
        ),
        taskId: requiredString(input.taskId, "task id"),
        approved: input.approved === true || input.decision === "approve",
      };
    }

    if (type === "recommendation_response") {
      return {
        type,
        sessionKey,
        councilSessionId: requiredString(input.councilSessionId, "Council session id"),
        recommendationId: requiredString(input.recommendationId, "recommendation id"),
        approved: input.approved === true || input.decision === "approve",
      };
    }

    if (type === "stop_turn") return { type, sessionKey };

    throw new Error(`Unsupported Linco inbound event: ${type}`);
  }
}
