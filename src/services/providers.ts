import { invoke } from "@tauri-apps/api/core";
import type {
  ProviderCredentialKind,
  ProviderId,
  ProviderInstance,
  ProviderModelEntry,
} from "../types/provider";

const PROVIDER_INSTANCES_KEY = "ai-os.provider-instances.v1";
export const PROVIDERS_CHANGED_EVENT = "ai-os:providers-changed";

let providerInstanceCache: ProviderInstance[] = [];
let providerInitialization: Promise<ProviderInstance[]> | undefined;

const KNOWN_MODELS: Record<string, Array<{ id: string; name: string }>> = {
  openai: [
    { id: "gpt-5.6", name: "GPT‑5.6" },
    { id: "gpt-5.6-codex", name: "GPT‑5.6 Codex" },
  ],
  anthropic: [
    { id: "claude-opus", name: "Claude Opus" },
    { id: "claude-sonnet", name: "Claude Sonnet" },
  ],
  google: [
    { id: "gemini-pro", name: "Gemini Pro" },
    { id: "gemini-flash", name: "Gemini Flash" },
  ],
  grok: [{ id: "grok-4.5", name: "Grok 4.5" }],
  deepseek: [
    { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash" },
    { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro" },
  ],
  openrouter: [{ id: "openrouter/auto", name: "OpenRouter Auto" }],
  doubao: [{ id: "doubao-seed", name: "Doubao Seed" }],
  kimi: [{ id: "kimi-for-coding", name: "Kimi for Coding" }],
  meta: [{ id: "muse-spark-1.1", name: "Muse Spark 1.1" }],
  compatible: [{ id: "default", name: "Default model" }],
};

export type ProviderCredentialStatus = {
  providerInstanceId: string;
  hasCredential: boolean;
};

export type OAuthProviderConfiguration = {
  providerId: string;
  providerInstanceId: string;
  clientId: string;
  authorizationUrl: string;
  tokenUrl: string;
  scopes: string[];
  resourceProjectId?: string;
  callbackPort?: number;
  callbackPath?: string;
  authorizationParams?: Record<string, string>;
};

export type BeginOAuthResult = {
  authorizationUrl: string;
  redirectUri: string;
  state: string;
};

export type CompleteOAuthResult = {
  providerInstanceId: string;
  expiresAt: string | null;
  refreshable: boolean;
};

export type RefreshOAuthResult = {
  providerInstanceId: string;
  expiresAt: string | null;
  refreshable: boolean;
};

export type ProviderOAuthCompletedEvent = {
  providerId: string;
  providerInstanceId: string;
  state: string;
  expiresAt: string | null;
  refreshable: boolean;
};

export type ProviderOAuthErrorEvent = {
  providerId: string;
  providerInstanceId: string;
  state: string;
  message: string;
};

const PROVIDER_OAUTH_CONFIGURATION_REGISTRY: Record<string, string> = {
  google: "GOOGLE",
  "google-workspace": "GOOGLE",
  "microsoft-graph": "MICROSOFT",
};

const OPENAI_CODEX_OAUTH_CONFIGURATION = {
  clientId: "app_EMoamEEZ73f0CkXaXp7hrann",
  authorizationUrl: "https://auth.openai.com/oauth/authorize",
  tokenUrl: "https://auth.openai.com/oauth/token",
  scopes: [
    "openid",
    "profile",
    "email",
    "offline_access",
    "api.connectors.read",
    "api.connectors.invoke",
  ],
  callbackPort: 1455,
  callbackPath: "/auth/callback",
  authorizationParams: {
    id_token_add_organizations: "true",
    codex_cli_simplified_flow: "true",
    originator: "ai-os",
  },
} satisfies Omit<OAuthProviderConfiguration, "providerId" | "providerInstanceId">;

const OPENROUTER_OAUTH_CONFIGURATION = {
  // OpenRouter's PKCE exchange does not require a registered client ID.
  // This local identifier is retained only in AI-OS session metadata.
  clientId: "ai-os-openrouter-pkce",
  authorizationUrl: "https://openrouter.ai/auth",
  tokenUrl: "https://openrouter.ai/api/v1/auth/keys",
  scopes: [],
} satisfies Omit<OAuthProviderConfiguration, "providerId" | "providerInstanceId">;

export function getProviderOAuthConfiguration(
  providerId: string,
  providerInstanceId: string,
): OAuthProviderConfiguration | undefined {
  if (providerId === "microsoft-graph") {
    const environment = import.meta.env as Record<string, unknown>;
    const developmentClientId = typeof window === "undefined"
      ? ""
      : window.localStorage.getItem("ai-os.microsoft.client-id") ?? "";
    const clientId =
      String(environment.VITE_AI_OS_MICROSOFT_OAUTH_CLIENT_ID ?? "").trim() ||
      developmentClientId.trim();
    if (!clientId) return undefined;
    const tenant = String(
      environment.VITE_AI_OS_MICROSOFT_OAUTH_TENANT ?? "common",
    ).trim();
    if (!/^(common|organizations|consumers|[0-9a-f-]{36})$/i.test(tenant)) {
      return undefined;
    }
    const authority = `https://login.microsoftonline.com/${tenant}/oauth2/v2.0`;
    return {
      providerId,
      providerInstanceId,
      clientId,
      authorizationUrl: `${authority}/authorize`,
      tokenUrl: `${authority}/token`,
      scopes: [
        "offline_access",
        "User.Read",
        "Files.ReadWrite",
      ],
      authorizationParams: { prompt: "select_account" },
    };
  }

  if (providerId === "google-workspace") {
    const environment = import.meta.env as Record<string, unknown>;
    const clientId = String(environment.VITE_AI_OS_GOOGLE_OAUTH_CLIENT_ID ?? "").trim();
    if (!clientId) return undefined;
    return {
      providerId,
      providerInstanceId,
      clientId,
      authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth",
      tokenUrl: "https://oauth2.googleapis.com/token",
      scopes: [
        "openid",
        "email",
        "https://www.googleapis.com/auth/drive.file",
        "https://www.googleapis.com/auth/documents",
        "https://www.googleapis.com/auth/spreadsheets",
        "https://www.googleapis.com/auth/presentations",
      ],
      authorizationParams: {
        access_type: "offline",
        include_granted_scopes: "true",
        prompt: "consent select_account",
      },
    };
  }

  if (providerId === "openai") {
    return {
      providerId,
      providerInstanceId,
      ...OPENAI_CODEX_OAUTH_CONFIGURATION,
    };
  }

  if (providerId === "openrouter") {
    return {
      providerId,
      providerInstanceId,
      ...OPENROUTER_OAUTH_CONFIGURATION,
    };
  }

  const prefix = PROVIDER_OAUTH_CONFIGURATION_REGISTRY[providerId];
  if (!prefix) return undefined;

  const environment = import.meta.env as Record<string, unknown>;
  const read = (suffix: string) => {
    const value = environment[`VITE_AI_OS_${prefix}_OAUTH_${suffix}`];
    return typeof value === "string" ? value.trim() : "";
  };
  const clientId = read("CLIENT_ID");
  const authorizationUrl = read("AUTHORIZATION_URL");
  const tokenUrl = read("TOKEN_URL");
  const resourceProjectId = read("RESOURCE_PROJECT_ID");
  const scopes = read("SCOPES")
    .split(/[ ,]+/)
    .map((scope) => scope.trim())
    .filter(Boolean);

  if (
    !clientId ||
    !authorizationUrl ||
    !tokenUrl ||
    scopes.length === 0 ||
    (providerId === "google" && !resourceProjectId)
  ) {
    return undefined;
  }

  return {
    providerId,
    providerInstanceId,
    clientId,
    authorizationUrl,
    tokenUrl,
    scopes,
    resourceProjectId: resourceProjectId || undefined,
  };
}

export function isProviderOAuthConfigured(providerId: string): boolean {
  return getProviderOAuthConfiguration(providerId, "configuration-check") !== undefined;
}

export async function startProviderOAuth(
  providerId: string,
  providerInstanceId: string,
): Promise<BeginOAuthResult> {
  const configuration = getProviderOAuthConfiguration(
    providerId,
    providerInstanceId,
  );
  if (!configuration) {
    throw new Error("Account sign-in is not configured for this Provider.");
  }

  return beginProviderOAuth(configuration);
}

export async function saveProviderApiKey(
  providerInstanceId: string,
  secret: string,
): Promise<ProviderCredentialStatus> {
  return invoke<ProviderCredentialStatus>("set_provider_credential", {
    input: { providerInstanceId, secret },
  });
}

export async function getProviderCredentialStatus(
  providerInstanceId: string,
): Promise<ProviderCredentialStatus> {
  return invoke<ProviderCredentialStatus>("get_provider_credential_status", {
    query: { providerInstanceId },
  });
}

export async function deleteProviderCredential(
  providerInstanceId: string,
): Promise<ProviderCredentialStatus> {
  return invoke<ProviderCredentialStatus>("delete_provider_credential", {
    query: { providerInstanceId },
  });
}

export async function beginProviderOAuth(
  input: OAuthProviderConfiguration,
): Promise<BeginOAuthResult> {
  return invoke<BeginOAuthResult>("begin_provider_oauth", { input });
}

export async function completeProviderOAuth(input: {
  providerId: string;
  state: string;
  code: string;
}): Promise<CompleteOAuthResult> {
  return invoke<CompleteOAuthResult>("complete_provider_oauth", { input });
}

export async function cancelProviderOAuth(input: {
  providerId: string;
  state: string;
}): Promise<boolean> {
  return invoke<boolean>("cancel_provider_oauth", { input });
}

export async function refreshProviderOAuth(
  providerInstanceId: string,
): Promise<RefreshOAuthResult> {
  return invoke<RefreshOAuthResult>("refresh_provider_oauth", {
    query: { providerInstanceId },
  });
}

function isProviderInstance(value: unknown): value is ProviderInstance {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ProviderInstance>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.providerId === "string" &&
    typeof candidate.displayName === "string" &&
    Array.isArray(candidate.models)
  );
}

function readLegacyProviderInstances(): ProviderInstance[] {
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(PROVIDER_INSTANCES_KEY) ?? "[]",
    );

    return Array.isArray(parsed)
      ? parsed.filter(isProviderInstance)
      : [];
  } catch {
    return [];
  }
}

function replaceProviderCache(
  instances: ProviderInstance[],
): ProviderInstance[] {
  providerInstanceCache = [...instances];

  window.dispatchEvent(
    new Event(PROVIDERS_CHANGED_EVENT),
  );

  return listProviderInstances();
}

export function oauthConnectionState(
  instance: ProviderInstance,
  now = Date.now(),
): ProviderInstance["connectionState"] {
  if (
    instance.connectionState === "connected" &&
    (instance.providerId === "microsoft-graph" || instance.providerId === "google-workspace")
  ) {
    return "connected";
  }
  if (instance.credential.kind !== "oauth" || !instance.credential.expiresAt) {
    return instance.connectionState;
  }

  const expiresAt = Date.parse(instance.credential.expiresAt);
  if (!Number.isFinite(expiresAt) || expiresAt > now) {
    return instance.connectionState;
  }

  return instance.credential.refreshable ? "refresh-required" : "expired";
}

export function withCurrentOAuthState(
  instance: ProviderInstance,
  now = Date.now(),
): ProviderInstance {
  const connectionState = oauthConnectionState(instance, now);
  return connectionState === instance.connectionState
    ? instance
    : { ...instance, connectionState };
}

export function listProviderInstances(): ProviderInstance[] {
  return providerInstanceCache.map((instance) => withCurrentOAuthState(instance));
}

export function initializeProviderInstances():
Promise<ProviderInstance[]> {
  providerInitialization ??= (async () => {
    const legacy = readLegacyProviderInstances();

    try {
      let native = (
        await invoke<unknown[]>(
          "list_provider_instances",
        )
      ).filter(isProviderInstance);

      if (
        native.length === 0 &&
        legacy.length > 0
      ) {
        const migrated: ProviderInstance[] = [];

        for (const instance of legacy) {
          const saved =
            await invoke<ProviderInstance>(
              "save_provider_instance",
              {
                instance,
              },
            );

          migrated.push(saved);
        }

        native = migrated;

        localStorage.removeItem(
          PROVIDER_INSTANCES_KEY,
        );
      }

      return replaceProviderCache(native);
    } catch {
      return replaceProviderCache(legacy);
    }
  })();

  return providerInitialization;
}

export async function saveProviderInstance(
  instance: ProviderInstance,
): Promise<ProviderInstance> {
  const saved =
    await invoke<ProviderInstance>(
      "save_provider_instance",
      {
        instance,
      },
    );

  replaceProviderCache([
    ...providerInstanceCache.filter(
      (candidate) =>
        candidate.id !== saved.id,
    ),
    saved,
  ]);

  return saved;
}

export async function removeProviderInstance(
  instanceId: string,
): Promise<boolean> {
  const removed =
    await invoke<boolean>(
      "remove_provider_instance",
      {
        instanceId,
      },
    );

  if (removed) {
    replaceProviderCache(
      providerInstanceCache.filter(
        (instance) =>
          instance.id !== instanceId,
      ),
    );
  }

  return removed;
}

export function discoverKnownModels(
  providerId: ProviderId,
  providerInstanceId: string,
): ProviderModelEntry[] {
  return (KNOWN_MODELS[providerId] ?? KNOWN_MODELS.compatible).map(
    (model, index) => ({
      id: `${providerInstanceId}:${model.id}`,
      providerInstanceId,
      remoteModelId: model.id,
      displayName: model.name,
      capabilities: ["chat", "tool-use"],
      enabled: true,
      isDefault: index === 0,
    }),
  );
}

export function createProviderInstance(input: {
  id: string;
  providerId: ProviderId;
  displayName: string;
  credentialKind: ProviderCredentialKind;
  models: ProviderModelEntry[];
  defaultModelId: string;
  liveTested?: boolean;
  credentialExpiresAt?: string;
  credentialRefreshable?: boolean;
}): ProviderInstance {
  const now = new Date().toISOString();
  return {
    id: input.id,
    providerId: input.providerId,
    displayName: input.displayName,
    credential: {
      kind: input.credentialKind,
      keychainAccount:
        input.credentialKind === "local" ? undefined : input.id,
      expiresAt:
        input.credentialKind === "oauth"
          ? input.credentialExpiresAt
          : undefined,
      refreshable:
        input.credentialKind === "oauth" &&
        input.credentialRefreshable === true,
    },
    connectionState: input.liveTested ? "connected" : "ready-for-test",
    models: input.models.map((model) => ({
      ...model,
      isDefault: model.id === input.defaultModelId,
    })),
    createdAt: now,
    updatedAt: now,
    lastTestedAt: now,
  };
}
