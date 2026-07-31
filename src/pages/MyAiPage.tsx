import { useEffect, useState } from "react";
import type { OllamaModel } from "../types/index";
import type { ProviderModelEntry } from "../types/provider";
import { getProviderAdapter } from "../services/providerAdapters";
import {
  createProviderInstance,
  deleteProviderCredential,
  listProviderInstances,
  removeProviderInstance,
  saveProviderApiKey,
  saveProviderInstance,
} from "../services/providers";

type MyAiPageProps = {
  localModels: OllamaModel[];
  onConnect: (provider: string, method: "account" | "api-key") => void;
  onManageLocalModels: () => void;
};

type CloudProviderCard = {
  id: string;
  mark: string;
  name: string;
  description: string;
  models: string[];
  accountLabel: string;
  apiKeyOnly?: boolean;
  accountSignInAvailable?: boolean;
};

const cloudProviders: CloudProviderCard[] = [
  {
    id: "openai",
    mark: "O",
    name: "OpenAI",
    description: "GPT and Codex models",
    models: ["GPT‑5.6", "GPT‑5.6 Codex"],
    accountLabel: "Sign in with OpenAI",
    accountSignInAvailable: false,
  },
  {
    id: "anthropic",
    mark: "A",
    name: "Anthropic",
    description: "Claude models",
    models: ["Claude Opus", "Claude Sonnet"],
    accountLabel: "Sign in with Claude",
    accountSignInAvailable: false,
  },
  {
    id: "google",
    mark: "G",
    name: "Google",
    description: "Gemini models",
    models: ["Gemini Pro", "Gemini Flash"],
    accountLabel: "Sign in with Google",
    accountSignInAvailable: false,
  },
  {
    id: "grok",
    mark: "X",
    name: "xAI",
    description: "Grok models",
    models: ["Grok 4.5"],
    accountLabel: "Connect xAI account",
    accountSignInAvailable: false,
  },
  {
    id: "deepseek",
    mark: "D",
    name: "DeepSeek",
    description: "DeepSeek chat and reasoning models",
    models: ["DeepSeek V4 Flash", "DeepSeek Reasoner"],
    accountLabel: "",
    apiKeyOnly: true,
  },
];

const providerCatalog = [
  {
    id: "doubao",
    mark: "豆",
    name: "Doubao",
    description: "Doubao and Seed models",
    connection: "API key",
  },
  {
    id: "kimi",
    mark: "K",
    name: "Kimi",
    description: "Moonshot and Kimi models",
    connection: "API key or account",
  },
  {
    id: "meta",
    mark: "M",
    name: "Meta",
    description: "Llama models from your chosen host",
    connection: "Hosted or local",
  },
  {
    id: "compatible",
    mark: "+",
    name: "Other AI",
    description: "Connect another compatible provider",
    connection: "Guided setup",
  },
];

function loadConfiguredProviderIds(): Set<string> {
  try {
    const raw = localStorage.getItem("ai-os.multillm.providers.v1");
    if (!raw) return new Set();

    const providers: unknown = JSON.parse(raw);
    if (!Array.isArray(providers)) return new Set();

    return new Set(
      providers
        .filter((provider) => {
          if (!provider || typeof provider !== "object") return false;
          const value = provider as Record<string, unknown>;
          return value.enabled === true && typeof value.id === "string";
        })
        .map((provider) => String((provider as Record<string, unknown>).id)),
    );
  } catch {
    return new Set();
  }
}

function MyAiPage({
  localModels,
  onConnect,
  onManageLocalModels,
}: MyAiPageProps) {
  const [providerInstances, setProviderInstances] = useState(() => listProviderInstances());
  const [ollamaAdapterModels, setOllamaAdapterModels] = useState<ProviderModelEntry[]>([]);
  const configuredProviderIds = new Set([
    ...loadConfiguredProviderIds(),
    ...providerInstances.map((instance) => instance.providerId),
  ]);
  const [setup, setSetup] = useState<{
    providerId: string;
    provider: string;
    method: "account" | "api-key";
    phase: "credential" | "models" | "manage";
    models: ProviderModelEntry[];
    defaultModelId: string;
    verificationMessage: string;
    liveTested: boolean;
  } | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [setupError, setSetupError] = useState("");
  const [isConnecting, setIsConnecting] = useState(false);

  useEffect(() => {
    if (!setup) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSetup(null);
    };

    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [setup]);

  useEffect(() => {
    let active = true;
    void getProviderAdapter("ollama")
      .testConnection("ollama-local")
      .then((result) => {
        if (active) setOllamaAdapterModels(result.discoveredModels);
      })
      .catch(() => {
        if (active) setOllamaAdapterModels([]);
      });
    return () => {
      active = false;
    };
  }, []);

  function openSetup(provider: string, method: "account" | "api-key", providerId?: string) {
    setApiKey("");
    setSetupError("");
    setSetup({
      providerId: providerId ?? provider.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
      provider,
      method,
      phase: "credential",
      models: [],
      defaultModelId: "",
      verificationMessage: "",
      liveTested: false,
    });
  }

  function providerInstanceId(provider: string): string {
    return `${provider.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "custom"}-default`;
  }

  function openManage(instanceId: string) {
    const instance = providerInstances.find((candidate) => candidate.id === instanceId);
    if (!instance) return;
    setApiKey("");
    setSetupError("");
    setSetup({
      providerId: instance.providerId,
      provider: instance.displayName,
      method: instance.credential.kind === "oauth" ? "account" : "api-key",
      phase: "manage",
      models: instance.models,
      defaultModelId:
        instance.models.find((model) => model.isDefault)?.id ??
        instance.models[0]?.id ??
        "",
      verificationMessage:
        instance.connectionState === "connected"
          ? "This Provider passed a live adapter test."
          : "The credential is saved. Live adapter testing is not enabled yet.",
      liveTested: instance.connectionState === "connected",
    });
  }

  async function continueSetup() {
    if (!setup || isConnecting) return;

    if (setup.method === "account") {
      onConnect(setup.provider, setup.method);
      setSetup(null);
      return;
    }

    if (!apiKey.trim()) {
      setSetupError("Enter an API key to continue.");
      return;
    }

    setIsConnecting(true);
    setSetupError("");
    try {
      const instanceId = providerInstanceId(setup.providerId);
      await saveProviderApiKey(instanceId, apiKey);
      const adapter = getProviderAdapter(setup.providerId);
      const verification = await adapter.testConnection(instanceId);
      const models = verification.discoveredModels;
      setApiKey("");
      setSetup({
        ...setup,
        phase: "models",
        models,
        defaultModelId: models[0]?.id ?? "",
        verificationMessage: verification.message,
        liveTested: verification.level === "live" && verification.ok,
      });
    } catch {
      setSetupError("AI‑OS could not store this key in macOS Keychain.");
    } finally {
      setIsConnecting(false);
    }
  }

  function finishProviderSetup() {
    if (!setup || setup.phase !== "models" || !setup.defaultModelId) return;
    const instance = createProviderInstance({
      id: providerInstanceId(setup.providerId),
      providerId: setup.providerId,
      displayName: setup.provider,
      credentialKind: "api-key",
      models: setup.models,
      defaultModelId: setup.defaultModelId,
      liveTested: setup.liveTested,
    });
    saveProviderInstance(instance);
    setProviderInstances(listProviderInstances());
    onConnect(setup.provider, setup.method);
    setSetup(null);
  }

  async function disconnectProvider() {
    if (!setup || isConnecting) return;
    setIsConnecting(true);
    setSetupError("");
    const instanceId = providerInstanceId(setup.providerId);
    try {
      await deleteProviderCredential(instanceId);
      removeProviderInstance(instanceId);
      setProviderInstances(listProviderInstances());
      setSetup(null);
    } catch {
      setSetupError("AI‑OS could not remove this Provider credential from macOS Keychain.");
    } finally {
      setIsConnecting(false);
    }
  }

  return (
    <section className="my-ai-page">
      <header className="my-ai-header">
        <div>
          <p className="settings-kicker">Intelligence</p>
          <h1>My AI</h1>
          <p>Connect the AI accounts and local models you want AI‑OS to use.</p>
        </div>
        <button type="button" className="add-provider-button" onClick={() => openSetup("Other AI", "api-key", "compatible")}>
          <span>+</span> Add AI
        </button>
      </header>

      <div className="provider-section-heading">
        <h2>Cloud AI</h2>
        <span>Choose account sign-in or an API key</span>
      </div>

      <div className="provider-grid">
        {cloudProviders.map((provider) => {
          const instance = providerInstances.find(
            (candidate) => candidate.providerId === provider.id,
          );
          const legacyConnected = configuredProviderIds.has(provider.id) && !instance;
          const statusLabel =
            instance?.connectionState === "connected"
              ? "Connected"
              : instance?.connectionState === "ready-for-test"
                ? "Saved"
                : legacyConnected
                  ? "Connected"
                  : "Not connected";
          return (
          <article key={provider.id} className="provider-card">
            <div className="provider-card-heading">
              <div className={`provider-mark provider-mark-${provider.id}`}>{provider.mark}</div>
              <div>
                <h3>{provider.name}</h3>
                <p>{provider.description}</p>
              </div>
              <span className={`connection-badge ${configuredProviderIds.has(provider.id) ? "connection-badge-ready" : ""}`}>
                {statusLabel}
              </span>
            </div>

            <div className="provider-models">
              {provider.models.map((model) => <span key={model}>{model}</span>)}
            </div>

            <div className="provider-actions">
              <button
                type="button"
                className="provider-primary"
                disabled={
                  !instance &&
                  !provider.apiKeyOnly &&
                  provider.accountSignInAvailable !== true
                }
                title={
                  !instance &&
                  !provider.apiKeyOnly &&
                  provider.accountSignInAvailable !== true
                    ? "Account sign-in is not configured yet. Use an API key."
                    : undefined
                }
                onClick={() => {
                  if (instance) {
                    openManage(instance.id);
                    return;
                  }

                  if (provider.apiKeyOnly) {
                    openSetup(provider.name, "api-key", provider.id);
                    return;
                  }

                  if (provider.accountSignInAvailable === true) {
                    openSetup(provider.name, "account", provider.id);
                  }
                }}
              >
                {configuredProviderIds.has(provider.id)
                  ? "Manage connection"
                  : provider.apiKeyOnly
                    ? "Connect DeepSeek"
                    : provider.accountSignInAvailable === true
                      ? provider.accountLabel
                      : "Account sign-in · Coming later"}
              </button>
              {!provider.apiKeyOnly && (
                <button type="button" className="provider-secondary" onClick={() => openSetup(provider.name, "api-key", provider.id)}>
                  Use API key
                </button>
              )}
            </div>
          </article>
          );
        })}
      </div>

      <div className="provider-section-heading local-heading">
        <h2>On this Mac</h2>
        <span>Private models that run locally</span>
      </div>

      <article className="local-provider-card">
        <div className="provider-card-heading">
          <div className="provider-mark provider-mark-ollama">O</div>
          <div>
            <h3>Ollama</h3>
            <p>{(ollamaAdapterModels.length || localModels.length) ? `${ollamaAdapterModels.length || localModels.length} local model${(ollamaAdapterModels.length || localModels.length) === 1 ? "" : "s"} available` : "No local models installed"}</p>
          </div>
          <span className={`connection-badge ${(ollamaAdapterModels.length || localModels.length) ? "connection-badge-ready" : ""}`}>
            {(ollamaAdapterModels.length || localModels.length) ? "Available" : "Set up"}
          </span>
        </div>
        {(ollamaAdapterModels.length > 0 || localModels.length > 0) && (
          <div className="local-model-list">
            {(ollamaAdapterModels.length
              ? ollamaAdapterModels.map((model) => ({ name: model.displayName }))
              : localModels).slice(0, 4).map((model, index) => (
              <div key={model.name}>
                <span>{model.name}</span>
                {index === 0 && <small>Suggested</small>}
              </div>
            ))}
          </div>
        )}
        <button type="button" className="manage-models-button" onClick={onManageLocalModels}>
          Manage local models <span>→</span>
        </button>
      </article>

      <div className="provider-section-heading catalog-heading">
        <h2>Add another Provider</h2>
        <span>Pick a service — AI‑OS will guide the setup</span>
      </div>

      <div className="provider-catalog">
        {providerCatalog.map((provider) => (
          <article key={provider.id} className="catalog-provider">
            <div className={`catalog-provider-mark catalog-provider-mark-${provider.id}`}>
              {provider.mark}
            </div>
            <div>
              <h3>{provider.name}</h3>
              <p>{provider.description}</p>
              <small>{provider.connection}</small>
            </div>
            <button
              type="button"
              aria-label={`Add ${provider.name}`}
              onClick={() => openSetup(provider.name, "api-key", provider.id)}
            >
              Add
            </button>
          </article>
        ))}
      </div>

      {setup && (
        <div className="provider-setup-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setSetup(null);
        }}>
          <section className="provider-setup-dialog" role="dialog" aria-modal="true" aria-labelledby="provider-setup-title">
            <header>
              <div className="provider-setup-mark">{setup.provider.slice(0, 1)}</div>
              <div>
                <p>Connect Provider</p>
                <h2 id="provider-setup-title">{setup.provider}</h2>
              </div>
              <button type="button" className="provider-setup-close" aria-label="Close Provider setup" onClick={() => setSetup(null)}>×</button>
            </header>

            {setup.phase === "credential" && <div className="provider-setup-methods" role="tablist" aria-label="Connection method">
              <button
                type="button"
                role="tab"
                aria-selected={false}
                aria-disabled="true"
                disabled
                title="Account sign-in requires a configured OAuth Provider Adapter."
              >
                <span>Account sign-in</span>
                <small>Not configured yet</small>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={setup.method === "api-key"}
                onClick={() => setSetup((current) => current && { ...current, method: "api-key" })}
              >
                <span>API key</span>
                <small>Connect a developer account</small>
              </button>
            </div>}

            <div className="provider-setup-body">
              {setup.phase === "manage" ? (
                <>
                  <h3>Provider details</h3>
                  <p>{setup.verificationMessage}</p>
                  <div className="provider-instance-summary">
                    <div>
                      <span>Default model</span>
                      <strong>{setup.models.find((model) => model.id === setup.defaultModelId)?.displayName ?? "Not selected"}</strong>
                    </div>
                    <div>
                      <span>Credential</span>
                      <strong>{setup.method === "account" ? "Account sign-in" : "macOS Keychain"}</strong>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="provider-disconnect-button"
                    disabled={isConnecting}
                    onClick={() => void disconnectProvider()}
                  >
                    {isConnecting ? "Disconnecting…" : "Disconnect Provider"}
                  </button>
                  {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
                </>
              ) : setup.phase === "models" ? (
                <>
                  <h3>Choose the default model</h3>
                  <p>The credential is safely stored. These models come from the AI‑OS adapter catalog; live Provider discovery will replace this list when that adapter is enabled.</p>
                  <div className="provider-model-choice">
                    {setup.models.map((model) => (
                      <label key={model.id}>
                        <input
                          type="radio"
                          name="default-provider-model"
                          value={model.id}
                          checked={setup.defaultModelId === model.id}
                          onChange={() => setSetup({ ...setup, defaultModelId: model.id })}
                        />
                        <span>{model.displayName}</span>
                        <small>{setup.defaultModelId === model.id ? "Default" : "Available"}</small>
                      </label>
                    ))}
                  </div>
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p>
                      <strong>{setup.liveTested ? "Connection tested" : "Credential verified"}</strong>
                      <small>{setup.verificationMessage}</small>
                    </p>
                  </div>
                </>
              ) : setup.method === "account" ? (
                <>
                  <h3>Sign in in your browser</h3>
                  <p>AI‑OS will open the official {setup.provider} sign-in page. Your password is never entered into AI‑OS.</p>
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p><strong>Protected connection</strong><small>Account sign-in will become available after this Provider’s OAuth configuration and callback flow are enabled.</small></p>
                  </div>
                </>
              ) : (
                <>
                  <h3>Add an API key securely</h3>
                  <p>The key is sent directly to the native AI‑OS security layer and is never stored in browser or local app settings.</p>
                  <label className="secure-key-field">
                    <span>API key</span>
                    <input
                      type="password"
                      autoComplete="off"
                      spellCheck={false}
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      placeholder="Paste your API key"
                    />
                  </label>
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p><strong>Stored in macOS Keychain</strong><small>AI‑OS can check whether a key exists, but the native layer never returns the secret to the interface.</small></p>
                  </div>
                </>
              )}
              {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
            </div>

            {setup.phase !== "manage" && <footer>
              <button type="button" className="provider-setup-cancel" onClick={() => setSetup(null)}>Cancel</button>
              <button
                type="button"
                className="provider-setup-continue"
                disabled={isConnecting || (setup.phase === "credential" && setup.method === "api-key" && !apiKey.trim())}
                onClick={() => setup.phase === "models" ? finishProviderSetup() : void continueSetup()}
              >
                {isConnecting ? "Saving…" : setup.phase === "models" ? "Save Provider" : setup.method === "account" ? "Continue in browser" : "Save and choose model"}
                <span>→</span>
              </button>
            </footer>}
          </section>
        </div>
      )}
    </section>
  );
}

export default MyAiPage;
