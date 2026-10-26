import { listCliProviderAdapters } from "../src/services/cliProviderManager";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const adapters = listCliProviderAdapters();
assert(adapters.map((adapter) => adapter.id).join(",") === "anthropic,grok", "CLI adapters are not registered deterministically");
assert(adapters.every((adapter) => adapter.loginCommand.length > 0), "CLI adapter lacks login guidance");

const claude = adapters.find((adapter) => adapter.id === "anthropic")!;
const claudeModels = claude.models("anthropic-default", {
  installed: true, authenticated: true, message: "ready", models: ["claude-code-default"],
});
assert(claudeModels[0]?.remoteModelId === "claude-code-default", "Claude compatibility model changed");

const grok = adapters.find((adapter) => adapter.id === "grok")!;
assert(grok.officialPackage === "@xai-official/grok", "Grok installation is not routed to xAI's official package");
const grokModels = grok.models("grok-default", {
  installed: true, authenticated: true, version: "1.0.70", message: "ready",
  models: ["grok-4.7", "grok-4.7-fast"], defaultModel: "grok-4.7-fast",
});
assert(grokModels.length === 2 && grokModels[1].isDefault, "Grok live model confirmation lost its default model");

console.log("PASS: Generic CLI Provider Manager behavior");
