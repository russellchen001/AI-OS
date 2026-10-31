import {
  CLI_ACCOUNT_PROVIDER_IDS,
  describeCliInstallPlan,
  listCliProviderAdapters,
  type CliInstallPlan,
} from "../src/services/cliProviderManager";
import { availableUpdates, isUpdateCheckDue, UPDATE_CHECK_INTERVAL_MS } from "../src/services/softwareUpdateMonitor";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const adapters = listCliProviderAdapters();
assert(adapters.map((adapter) => adapter.id).join(",") === "anthropic,grok,google", "CLI adapters are not registered deterministically");
assert([...CLI_ACCOUNT_PROVIDER_IDS].join(",") === "anthropic,grok,google", "CLI account providers are not derived from the adapter registry");
assert(adapters.every((adapter) => adapter.setupTitle && adapter.setupInstructions && adapter.protectionNote), "CLI adapter lacks setup copy");
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

// Gemini signs in with the user's own Google account through the official CLI.
const gemini = adapters.find((adapter) => adapter.id === "google")!;
assert(gemini.officialPackage === "@google/gemini-cli", "Gemini is not routed to Google's official CLI package");
assert(gemini.verify, "Gemini sign-in is not confirmed with a real request");
assert(/own Google quota/.test(gemini.setupInstructions) && /Use API key/.test(gemini.setupInstructions), "Gemini setup does not explain the user's own quota and the API key route");
const geminiModels = gemini.models("google-default", {
  installed: true, authenticated: true, message: "ready", models: ["gemini-cli-default"], defaultModel: "gemini-cli-default",
});
assert(geminiModels.length === 1 && geminiModels[0].isDefault && geminiModels[0].remoteModelId === "gemini-cli-default", "Gemini CLI account default model missing");
assert(gemini.models("google-default", { installed: true, authenticated: false, message: "", models: [] }).length === 0, "Signed-out Gemini CLI offered models");

// The approval text names the change and follows how the CLI was installed.
const plan = (method: CliInstallPlan["method"], installedVersion?: string): CliInstallPlan => ({
  providerId: "google", displayName: "Gemini CLI", packageName: "gemini-cli", version: "0.66.0", integrity: "",
  installedVersion, action: installedVersion ? "upgrade" : "install", method, sourceLabel: method, requiresApproval: true,
});
assert(/from 0\.46\.0 to 0\.66\.0/.test(describeCliInstallPlan(plan("homebrew", "0.46.0"))) && /Homebrew/.test(describeCliInstallPlan(plan("homebrew", "0.46.0"))), "Homebrew update text is wrong");
assert(/SHA-512/.test(describeCliInstallPlan(plan("ai-os-npm"))) && /install Gemini CLI 0\.66\.0/.test(describeCliInstallPlan(plan("ai-os-npm"))), "Managed install text lost integrity guidance");
assert(/vendor's own updater/.test(describeCliInstallPlan(plan("official-installer", "1.0.0"))), "Official installer update text is wrong");
assert(/npm/.test(describeCliInstallPlan(plan("npm-global", "1.0.0"))), "npm global update text is wrong");

// One daily check covers CLIs and agents.
const now = 10 * UPDATE_CHECK_INTERVAL_MS;
assert(isUpdateCheckDue(undefined, now), "First launch does not check for updates");
assert(!isUpdateCheckDue({ checkedAt: now - 1000, cli: [], agents: [] }, now), "Update check repeats within a day");
assert(isUpdateCheckDue({ checkedAt: now - UPDATE_CHECK_INTERVAL_MS, cli: [], agents: [] }, now), "Update check skipped after a day");
const updates = availableUpdates({
  checkedAt: now,
  cli: [
    { providerId: "google", displayName: "Gemini CLI", installed: true, installedVersion: "0.46.0", latestVersion: "0.66.0", updateAvailable: true, method: "homebrew", sourceLabel: "Homebrew", canUpdate: true, message: "" },
    { providerId: "grok", displayName: "Grok CLI", installed: true, installedVersion: "1.0.70", latestVersion: "1.0.70", updateAvailable: false, method: "official-installer", sourceLabel: "", canUpdate: false, message: "" },
  ],
  agents: [{ agentId: "openclaw", installedVersion: "1.0.0", latestVersion: "1.1.0", integrity: "", updateAvailable: true, requiresApproval: true }],
});
assert(updates.join(",") === "Gemini CLI 0.66.0,openclaw 1.1.0", "Available updates are not summarised across CLIs and agents");

console.log("PASS: Generic CLI Provider Manager behavior");
