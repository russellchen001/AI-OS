import { isAiCenterChatModel, listAiCenterModels } from "../src/services/aiCenter";
import { createProviderInstance, initializeProviderInstances, saveProviderInstance, PROVIDERS_CHANGED_EVENT } from "../src/services/providers";
import { assignArenaModels } from "../src/services/arenaModelAssignment";

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
assertEqual(isAiCenterChatModel(model({ remoteModelId: "agent-preview", displayName: "Agent Preview" })), false);
assertEqual(isAiCenterChatModel(model({ remoteModelId: "aqa", displayName: "Attributed Question Answering" })), false);
assertEqual(
  isAiCenterChatModel(model({ capabilities: ["embeddings"], remoteModelId: "vector-model" })),
  false,
);

console.log("provider model filtering behavior passed");

const testWindow = Object.assign(new EventTarget(), {
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args: { instance?: unknown }) => {
      if (command === "list_provider_instances") return [];
      if (command === "save_provider_instance") return args.instance;
      throw new Error("unexpected native command");
    },
  },
});
Object.defineProperty(globalThis, "window", { value: testWindow });
Object.defineProperty(globalThis, "localStorage", { value: { getItem: () => null, removeItem: () => undefined } });
let providerEvents = 0;
testWindow.addEventListener(PROVIDERS_CHANGED_EVENT, () => { providerEvents += 1; });
await initializeProviderInstances();
assertEqual(listAiCenterModels().length, 0);
const instance = createProviderInstance({
  id: "beta6-fixture", providerId: "compatible", displayName: "Fixture",
  credentialKind: "api-key", liveTested: true, defaultModelId: "fixture-chat",
  models: [{ ...model(), capabilities: ["chat"], id: "fixture-chat", providerInstanceId: "beta6-fixture", isDefault: true }],
});
await saveProviderInstance(instance);
assertEqual(listAiCenterModels().length, 1);
assertEqual(listAiCenterModels()[0].providerInstanceId, "beta6-fixture");
const pinnedChoice = listAiCenterModels()[0];
const assignments = assignArenaModels(["participant", "synthesizer"].map((role, index) => ({
  id: `seat-${index}`, title: role, purpose: "test", requiredExpertise: [],
  role: role as "participant" | "synthesizer",
  modelConstraint: { mode: "pinned" as const, providerId: pinnedChoice.providerId, providerInstanceId: pinnedChoice.providerInstanceId, modelId: pinnedChoice.modelId },
})), [pinnedChoice, { ...pinnedChoice, modelId: "another-model" }]);
assertEqual(assignments.length, 2);
assertEqual(assignments[1].choice.modelId, pinnedChoice.modelId);
assertEqual(assignments[1].fallbackChoices.length, 0);
await saveProviderInstance({ ...instance, connectionState: "error" });
assertEqual(listAiCenterModels().length, 0);
assertEqual(providerEvents, 3);
console.log("empty, partially connected and failed Provider state synchronization passed");
