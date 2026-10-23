import { isAiCenterChatModel } from "../src/services/aiCenter";

function assertEqual(actual: unknown, expected: unknown): void {
  if (actual !== expected) {
    throw new Error(`expected ${String(expected)}, received ${String(actual)}`);
  }
}

const model = (overrides: Partial<Parameters<typeof isAiCenterChatModel>[0]> = {}) => ({
  remoteModelId: "gemini-live-chat",
  displayName: "Gemini Live Chat",
  capabilities: ["chat"],
  enabled: true,
  ...overrides,
});

assertEqual(isAiCenterChatModel(model()), true);
assertEqual(isAiCenterChatModel(model({ enabled: false })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "gemini-embedding-001" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "imagen-4-generate" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "gemini-tts-preview" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "veo-3-generate" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "deep-research-preview" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "antigravity-agent-preview" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "aqa", displayName: "Attributed Question Answering" })), false);
assertEqual(
  isAiCenterChatModel(model({ capabilities: ["embeddings"], remoteModelId: "vector-model" })),
  false,
);

console.log("provider model filtering behavior passed");
