import { isAiCenterChatModel, listAiCenterModels } from "../src/services/aiCenter";
import { createProviderInstance, initializeProviderInstances, saveProviderInstance, PROVIDERS_CHANGED_EVENT } from "../src/services/providers";
import { assignArenaModels } from "../src/services/arenaModelAssignment";
import { extractUserModelConstraints } from "../src/services/arenaStrategyRouter";
import { resolveUserConstraintsForSeats } from "../src/services/arenaConstraintBinding";

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
const globalChoice = { providerId: "omlx", providerInstanceId: "omlx-local", modelId: "Qwen2.5-7B-Instruct-4bit", label: "oMLX · Qwen2.5-7B-Instruct-4bit" };
const globalConstraints = extractUserModelConstraints(
  "三个席位都固定使用 oMLX Qwen2.5-7B-Instruct-4bit，不使用云端模型。",
  [{ ...globalChoice, modelId: "DeepSeek-local", label: "oMLX · DeepSeek-local" }, globalChoice],
);
assertEqual(globalConstraints.length, 1);
const globalResolved = resolveUserConstraintsForSeats(["participant", "participant", "synthesizer"], [
  { mode: "pinned", ...globalChoice }, ...globalConstraints,
]);
assertEqual(globalResolved.every((constraint) => constraint.mode === "pinned" && constraint.modelId === globalChoice.modelId), true);
const roleConstraints = extractUserModelConstraints(
  `Only use ${pinnedChoice.modelId} as participant. Use ${pinnedChoice.modelId} as synthesizer.`,
  [pinnedChoice],
);
assertEqual(roleConstraints.length, 2);
const resolved = resolveUserConstraintsForSeats(["participant", "participant", "synthesizer"], [
  { mode: "pinned", ...pinnedChoice }, ...roleConstraints,
]);
assertEqual(resolved.every((constraint) => constraint.mode === "pinned"), true);
const roleAssignments = assignArenaModels(resolved.map((modelConstraint, index) => ({
  id: `role-${index}`, title: "seat", purpose: "test", requiredExpertise: [],
  role: index === 2 ? "synthesizer" as const : "participant" as const, modelConstraint,
})), [pinnedChoice]);
assertEqual(roleAssignments.every((assignment) => assignment.choice.modelId === pinnedChoice.modelId && assignment.fallbackChoices.length === 0), true);
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
