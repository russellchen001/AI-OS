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
    { id: "deepseek-reasoner", name: "DeepSeek Reasoner" },
  ],
  doubao: [{ id: "doubao-seed", name: "Doubao Seed" }],
  kimi: [{ id: "kimi-k2", name: "Kimi K2" }],
  meta: [{ id: "llama", name: "Llama (host model)" }],
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
};

export type BeginOAuthResult = {
  authorizationUrl: string;
  redirectUri: string;
  state: string;
};

export type CompleteOAuthResult = {
  providerInstanceId: string;
  expiresAt?: string;
  refreshable: boolean;
};

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

export function listProviderInstances(): ProviderInstance[] {
  return [...providerInstanceCache];
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
      refreshable: input.credentialKind === "oauth",
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
