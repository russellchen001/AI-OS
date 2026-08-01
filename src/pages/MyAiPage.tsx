import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import type { OllamaModel } from "../types/index";
import type {
  ProviderAdapterDescriptor,
  ProviderModelEntry,
} from "../types/provider";
import {
  getProviderAdapter,
  listProviderAdapters,
} from "../services/providerAdapters";
import {
  createProviderInstance,
  cancelProviderOAuth,
  deleteProviderCredential,
  listProviderInstances,
  removeProviderInstance,
  saveProviderApiKey,
  saveProviderInstance,
  isProviderOAuthConfigured,
  startProviderOAuth,
  type ProviderOAuthCompletedEvent,
  type ProviderOAuthErrorEvent,
} from "../services/providers";

const OAUTH_FRONTEND_TIMEOUT_MS = 5 * 60 * 1000;

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
};

const cloudProviders: CloudProviderCard[] = [
  {
    id: "openai",
    mark: "O",
    name: "OpenAI",
    description: "GPT and Codex models",
    models: ["GPT‑5.6", "GPT‑5.6 Codex"],
    accountLabel: "Sign in with OpenAI",
  },
  {
    id: "anthropic",
    mark: "A",
    name: "Anthropic",
    description: "Claude models",
    models: ["Claude Opus", "Claude Sonnet"],
    accountLabel: "Sign in with Claude",
  },
  {
    id: "google",
    mark: "G",
    name: "Google",
    description: "Gemini models",
    models: ["Gemini Pro", "Gemini Flash"],
    accountLabel: "Sign in with Google",
  },
  {
    id: "grok",
    mark: "X",
    name: "xAI",
    description: "Grok models",
    models: ["Grok 4.5"],
    accountLabel: "Connect xAI account",
  },
  {
    id: "deepseek",
    mark: "D",
    name: "DeepSeek",
    description: "DeepSeek chat and reasoning models",
    models: ["DeepSeek V4 Flash", "DeepSeek Reasoner"],
    accountLabel: "",
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
  const [providerAdapters, setProviderAdapters] = useState<
    ProviderAdapterDescriptor[]
  >([]);
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
    credentialExpiresAt?: string;
    credentialRefreshable?: boolean;
  } | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [setupError, setSetupError] = useState("");
  const [isConnecting, setIsConnecting] = useState(false);
  const cancelOAuthRef = useRef<(() => void) | null>(null);

  function cancelOAuth() {
    cancelOAuthRef.current?.();
    cancelOAuthRef.current = null;
  }

  function closeSetup() {
    cancelOAuth();
    setIsConnecting(false);
    setSetup(null);
  }

  useEffect(() => () => cancelOAuth(), []);

  useEffect(() => {
    if (!setup) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeSetup();
    };

    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [setup]);

  useEffect(() => {
    let active = true;

    void listProviderAdapters()
      .then((descriptors) => {
        if (active) {
          setProviderAdapters(descriptors);
        }
      })
      .catch(() => {
        if (active) {
          setProviderAdapters([]);
        }
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    void getProviderAdapter("ollama")
      .then((adapter) =>
        adapter.testConnection("ollama-local"),
      )
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
          : instance.connectionState === "refresh-required"
            ? "This account token has expired and can be refreshed securely."
            : instance.connectionState === "expired"
              ? "This account token has expired. Sign in again to reconnect."
          : "The credential is saved. Live adapter testing is not enabled yet.",
      liveTested: instance.connectionState === "connected",
      credentialExpiresAt: instance.credential.expiresAt,
      credentialRefreshable: instance.credential.refreshable,
    });
  }

  async function refreshAccountCredential() {
    if (!setup || setup.phase !== "manage" || isConnecting) return;
    const instanceId = providerInstanceId(setup.providerId);
    setIsConnecting(true);
    setSetupError("");
    try {
      const adapter = await getProviderAdapter(setup.providerId);
      const refreshed = await adapter.refreshCredential(instanceId);
      setProviderInstances(listProviderInstances());
      setSetup({
        ...setup,
        verificationMessage: "Account access was refreshed successfully.",
        liveTested: refreshed.connectionState === "connected",
        credentialExpiresAt: refreshed.credential.expiresAt,
        credentialRefreshable: refreshed.credential.refreshable,
      });
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : "Provider account sign-in must be renewed.";
      const instance = providerInstances.find((candidate) => candidate.id === instanceId);
      if (instance) {
        await saveProviderInstance({
          ...instance,
          connectionState: message.includes("must be renewed")
            ? "expired"
            : "refresh-required",
          updatedAt: new Date().toISOString(),
        });
        setProviderInstances(listProviderInstances());
      }
      setSetupError(message);
    } finally {
      setIsConnecting(false);
    }
  }

  async function continueSetup() {
    if (!setup || isConnecting) return;

    if (setup.method === "account") {
      const instanceId = providerInstanceId(setup.providerId);
      setIsConnecting(true);
      setSetupError("");

      let completedUnlisten: UnlistenFn | undefined;
      let errorUnlisten: UnlistenFn | undefined;
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      let settled = false;
      let cancelled = false;
      let oauthState: string | undefined;
      let resolvePending: ((event: ProviderOAuthCompletedEvent) => void) | undefined;
      let rejectPending: ((reason: Error) => void) | undefined;
      const cleanup = () => {
        completedUnlisten?.();
        completedUnlisten = undefined;
        errorUnlisten?.();
        errorUnlisten = undefined;
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = undefined;
        }
      };

      try {
        const completion = new Promise<ProviderOAuthCompletedEvent>(
          (resolve, reject) => {
            resolvePending = resolve;
            rejectPending = reject;
          },
        );

        cancelOAuthRef.current = () => {
          cancelled = true;
          if (!settled) rejectPending?.(new Error("Account sign-in was cancelled."));
          cleanup();
          if (oauthState) {
            void cancelProviderOAuth({
              providerId: setup.providerId,
              state: oauthState,
            });
          }
        };

        const unlistenCompleted = await listen<ProviderOAuthCompletedEvent>(
          "provider-oauth://completed",
          ({ payload }) => {
            if (
              payload.providerId !== setup.providerId ||
              payload.providerInstanceId !== instanceId ||
              payload.state !== oauthState
            ) return;
            settled = true;
            resolvePending?.(payload);
          },
        );
        if (cancelled) {
          unlistenCompleted();
          return;
        }
        completedUnlisten = unlistenCompleted;

        const unlistenError = await listen<ProviderOAuthErrorEvent>(
          "provider-oauth://error",
          ({ payload }) => {
            if (
              payload.providerId !== setup.providerId ||
              payload.providerInstanceId !== instanceId ||
              payload.state !== oauthState
            ) return;
            settled = true;
            rejectPending?.(new Error(payload.message));
          },
        );
        if (cancelled) {
          unlistenError();
          return;
        }
        errorUnlisten = unlistenError;
        timeoutId = setTimeout(() => {
          settled = true;
          rejectPending?.(new Error("Account sign-in timed out. Please try again."));
        }, OAUTH_FRONTEND_TIMEOUT_MS);

        const oauth = await startProviderOAuth(setup.providerId, instanceId);
        oauthState = oauth.state;
        if (cancelled) {
          await cancelProviderOAuth({
            providerId: setup.providerId,
            state: oauth.state,
          });
          return;
        }
        await openUrl(oauth.authorizationUrl);
        const completed = await completion;

        const adapter = await getProviderAdapter(setup.providerId);
        const verification = await adapter.testConnection(instanceId);
        if (cancelled) return;
        const models = verification.discoveredModels;
        setSetup({
          ...setup,
          phase: "models",
          models,
          defaultModelId: models[0]?.id ?? "",
          verificationMessage: verification.message,
          liveTested: verification.level === "live" && verification.ok,
          credentialExpiresAt: completed.expiresAt ?? undefined,
          credentialRefreshable: completed.refreshable,
        });
      } catch (error) {
        if (oauthState) {
          await cancelProviderOAuth({
            providerId: setup.providerId,
            state: oauthState,
          }).catch(() => false);
        }
        if (!cancelled) {
          setSetupError(
            error instanceof Error
              ? error.message
              : "AI‑OS could not complete account sign-in.",
          );
        }
      } finally {
        cleanup();
        cancelOAuthRef.current = null;
        setIsConnecting(false);
      }
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
      const adapter =
        await getProviderAdapter(
          setup.providerId,
        );
      const verification =
        await adapter.testConnection(
          instanceId,
        );
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

  async function finishProviderSetup() {
    if (!setup || setup.phase !== "models" || !setup.defaultModelId || isConnecting) return;
    setIsConnecting(true);
    setSetupError("");
    try {
      const instance = createProviderInstance({
        id: providerInstanceId(setup.providerId),
        providerId: setup.providerId,
        displayName: setup.provider,
        credentialKind: setup.method === "account" ? "oauth" : "api-key",
        models: setup.models,
        defaultModelId: setup.defaultModelId,
        liveTested: setup.liveTested,
        credentialExpiresAt: setup.credentialExpiresAt,
        credentialRefreshable: setup.credentialRefreshable,
      });
      await saveProviderInstance(instance);
      setProviderInstances(listProviderInstances());
      onConnect(setup.provider, setup.method);
      setSetup(null);
    } catch (error) {
      setSetupError(
        typeof error === "string"
          ? error
          : error instanceof Error
          ? error.message
          : "AI‑OS could not save this Provider.",
      );
    } finally {
      setIsConnecting(false);
    }
  }

  async function disconnectProvider() {
    if (!setup || isConnecting) return;
    setIsConnecting(true);
    setSetupError("");
    const instanceId = providerInstanceId(setup.providerId);
    try {
      await deleteProviderCredential(instanceId);
      await removeProviderInstance(instanceId);
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
          const descriptor = providerAdapters.find(
            (candidate) => candidate.providerId === provider.id,
          );
          const supportsApiKey =
            descriptor?.authenticationMethods.includes("api-key") === true;
          const supportsAccountSignIn =
            isProviderOAuthConfigured(provider.id) &&
            descriptor?.authenticationMethods.some(
              (method) =>
                method === "oauth-pkce" ||
                method === "oauth-loopback" ||
                method === "device-code" ||
                method === "imported-credential",
            ) === true;
          const instance = providerInstances.find(
            (candidate) => candidate.providerId === provider.id,
          );
          const legacyConnected = configuredProviderIds.has(provider.id) && !instance;
          const statusLabel =
            instance?.connectionState === "connected"
              ? "Connected"
              : instance?.connectionState === "ready-for-test"
                ? "Saved"
                : instance?.connectionState === "refresh-required"
                  ? "Refresh needed"
                  : instance?.connectionState === "expired"
                    ? "Sign in again"
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
                  !supportsAccountSignIn
                }
                title={
                  !instance && !supportsAccountSignIn
                    ? "Account sign-in is not operational yet. API key connection is available."
                    : undefined
                }
                onClick={() => {
                  if (instance) {
                    openManage(instance.id);
                    return;
                  }

                  if (supportsAccountSignIn) {
                    openSetup(provider.name, "account", provider.id);
                    return;
                  }
                }}
              >
                {configuredProviderIds.has(provider.id)
                  ? "Manage connection"
                  : supportsAccountSignIn
                    ? provider.accountLabel
                    : "Account sign-in · Coming later"}
              </button>
              {supportsApiKey && (
                  <button
                    type="button"
                    className="provider-secondary"
                    onClick={() =>
                      openSetup(
                        provider.name,
                        "api-key",
                        provider.id,
                      )
                    }
                  >
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

      {setup && (() => {
        const setupDescriptor = providerAdapters.find(
          (descriptor) =>
            descriptor.providerId === setup.providerId,
        );
        const accountSignInAvailable =
          isProviderOAuthConfigured(setup.providerId) &&
          setupDescriptor?.authenticationMethods.some(
            (method) =>
              method === "oauth-pkce" ||
              method === "oauth-loopback" ||
              method === "device-code" ||
              method === "imported-credential",
          ) === true;

        return (
        <div className="provider-setup-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) closeSetup();
        }}>
          <section className="provider-setup-dialog" role="dialog" aria-modal="true" aria-labelledby="provider-setup-title">
            <header>
              <div className="provider-setup-mark">{setup.provider.slice(0, 1)}</div>
              <div>
                <p>Connect Provider</p>
                <h2 id="provider-setup-title">{setup.provider}</h2>
              </div>
              <button type="button" className="provider-setup-close" aria-label="Close Provider setup" onClick={closeSetup}>×</button>
            </header>

            {setup.phase === "credential" && <div className="provider-setup-methods" role="tablist" aria-label="Connection method">
              <button
                type="button"
                role="tab"
                aria-selected={setup.method === "account"}
                aria-disabled={!accountSignInAvailable}
                disabled={!accountSignInAvailable}
                title={
                  accountSignInAvailable
                    ? undefined
                    : "Account sign-in requires an operational Provider authentication adapter."
                }
                onClick={() =>
                  setSetup((current) =>
                    current && {
                      ...current,
                      method: "account",
                    },
                  )
                }
              >
                <span>Account sign-in</span>
                <small>
                  {accountSignInAvailable
                    ? "Connect your Provider account"
                    : "Coming later"}
                </small>
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
                    {setup.method === "account" && <div>
                      <span>Account access</span>
                      <strong>
                        {setup.credentialExpiresAt
                          ? new Date(setup.credentialExpiresAt).getTime() <= Date.now()
                            ? "Expired"
                            : `Until ${new Date(setup.credentialExpiresAt).toLocaleString()}`
                          : "No expiry reported"}
                      </strong>
                    </div>}
                  </div>
                  {setup.method === "account" && setup.credentialRefreshable && (
                    <button
                      type="button"
                      className="provider-refresh-button"
                      disabled={isConnecting}
                      onClick={() => void refreshAccountCredential()}
                    >
                      {isConnecting ? "Refreshing…" : "Refresh account access"}
                    </button>
                  )}
                  {setup.method === "account" && (
                    <button
                      type="button"
                      className="provider-refresh-button"
                      disabled={!isProviderOAuthConfigured(setup.providerId)}
                      onClick={() => openSetup(setup.provider, "account", setup.providerId)}
                    >
                      Sign in again
                    </button>
                  )}
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
                    <p><strong>Protected connection</strong><small>AI‑OS uses PKCE and a one-time local callback. The authorization code and tokens stay in the native security layer.</small></p>
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
              {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
              <button type="button" className="provider-setup-cancel" onClick={closeSetup}>Cancel</button>
              <button
                type="button"
                className="provider-setup-continue"
                disabled={isConnecting || (setup.phase === "credential" && setup.method === "api-key" && !apiKey.trim())}
                onClick={() => setup.phase === "models" ? void finishProviderSetup() : void continueSetup()}
              >
                {isConnecting
                  ? setup.phase === "models"
                    ? "Saving Provider…"
                    : setup.method === "account"
                      ? "Waiting for sign-in…"
                      : "Saving…"
                  : setup.phase === "models"
                    ? "Save Provider"
                    : setup.method === "account"
                      ? "Continue in browser"
                      : "Save and choose model"}
                <span>→</span>
              </button>
            </footer>}
          </section>
        </div>
        );
      })()}
    </section>
  );
}

export default MyAiPage;
