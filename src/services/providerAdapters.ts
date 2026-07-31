import type {
  ProviderAdapterContract,
  ProviderAdapterDescriptor,
  ProviderConnectionTest,
  ProviderId,
  ProviderInstance,
  ProviderModelEntry,
} from "../types/provider";
import { invoke } from "@tauri-apps/api/core";
import {
  discoverKnownModels,
  getProviderCredentialStatus,
  listProviderInstances,
} from "./providers";

const DESCRIPTORS: ProviderAdapterDescriptor[] = [
  {
    providerId: "openai",
    displayName: "OpenAI",
    credentialKinds: ["oauth", "api-key"],
    capabilities: ["chat", "reasoning", "vision", "tool-use"],
    supportsModelDiscovery: true,
    supportsTokenRefresh: true,
  },
  {
    providerId: "anthropic",
    displayName: "Anthropic",
    credentialKinds: ["oauth", "api-key"],
    capabilities: ["chat", "reasoning", "vision", "tool-use"],
    supportsModelDiscovery: true,
    supportsTokenRefresh: true,
  },
  {
    providerId: "google",
    displayName: "Google",
    credentialKinds: ["oauth", "api-key"],
    capabilities: ["chat", "reasoning", "vision", "tool-use"],
    supportsModelDiscovery: true,
    supportsTokenRefresh: true,
  },
  {
    providerId: "grok",
    displayName: "xAI",
    credentialKinds: ["oauth", "api-key"],
    capabilities: ["chat", "reasoning", "vision", "tool-use"],
    supportsModelDiscovery: true,
    supportsTokenRefresh: true,
  },
  ...["deepseek", "doubao", "kimi", "meta", "compatible"].map(
    (providerId): ProviderAdapterDescriptor => ({
      providerId,
      displayName:
        providerId === "deepseek"
          ? "DeepSeek"
          : providerId === "doubao"
          ? "Doubao"
          : providerId === "kimi"
            ? "Kimi"
            : providerId === "meta"
              ? "Meta"
              : "Other AI",
      credentialKinds: ["api-key"],
      capabilities: ["chat", "tool-use"],
      supportsModelDiscovery: providerId !== "compatible",
      supportsTokenRefresh: false,
    }),
  ),
  {
    providerId: "ollama",
    displayName: "Ollama",
    credentialKinds: ["local"],
    capabilities: ["chat", "tool-use"],
    supportsModelDiscovery: true,
    supportsTokenRefresh: false,
  },
];

type NativeAdapterResult = {
  providerId: string;
  level: "credential" | "live";
  message: string;
  models: Array<{ id: string; displayName: string }>;
};

class NativeProviderAdapter implements ProviderAdapterContract {
  constructor(private readonly adapterDescriptor: ProviderAdapterDescriptor) {}

  descriptor(): ProviderAdapterDescriptor {
    return this.adapterDescriptor;
  }

  async connect(instance: ProviderInstance): Promise<ProviderInstance> {
    return instance;
  }

  async disconnect(): Promise<void> {
    throw new Error("Disconnect is not available until this Provider Adapter is enabled.");
  }

  async refreshCredential(): Promise<ProviderInstance> {
    throw new Error("Token refresh is not available until OAuth is enabled.");
  }

  async discoverModels(instanceId: string): Promise<ProviderModelEntry[]> {
    const result = await invoke<NativeAdapterResult>("discover_provider_models", {
      query: {
        providerId: this.adapterDescriptor.providerId,
        providerInstanceId: instanceId,
      },
    });
    return result.models.map((model, index) => ({
      id: `${instanceId}:${model.id}`,
      providerInstanceId: instanceId,
      remoteModelId: model.id,
      displayName: model.displayName,
      capabilities: this.adapterDescriptor.capabilities,
      enabled: true,
      isDefault: index === 0,
    }));
  }

  async testConnection(instanceId: string): Promise<ProviderConnectionTest> {
    const result = await invoke<NativeAdapterResult>("test_provider_connection", {
      query: {
        providerId: this.adapterDescriptor.providerId,
        providerInstanceId: instanceId,
      },
    });
    const models = result.models.map((model, index) => ({
      id: `${instanceId}:${model.id}`,
      providerInstanceId: instanceId,
      remoteModelId: model.id,
      displayName: model.displayName,
      capabilities: this.adapterDescriptor.capabilities,
      enabled: true,
      isDefault: index === 0,
    }));
    return {
      ok: true,
      level: result.level,
      checkedAt: new Date().toISOString(),
      message: result.message,
      discoveredModels: models,
    };
  }
}

class CatalogProviderAdapter implements ProviderAdapterContract {
  constructor(private readonly adapterDescriptor: ProviderAdapterDescriptor) {}

  descriptor(): ProviderAdapterDescriptor {
    return this.adapterDescriptor;
  }

  async connect(instance: ProviderInstance): Promise<ProviderInstance> {
    return instance;
  }

  async disconnect(): Promise<void> {}

  async refreshCredential(): Promise<ProviderInstance> {
    throw new Error("Token refresh is not available for this Provider.");
  }

  async discoverModels(instanceId: string): Promise<ProviderModelEntry[]> {
    return discoverKnownModels(this.adapterDescriptor.providerId, instanceId);
  }

  async testConnection(instanceId: string): Promise<ProviderConnectionTest> {
    const credential = await getProviderCredentialStatus(instanceId);
    const models = await this.discoverModels(instanceId);
    return {
      ok: credential.hasCredential,
      level: "credential",
      checkedAt: new Date().toISOString(),
      message: credential.hasCredential
        ? "Credential saved. Add a native Adapter to enable live testing."
        : "No credential is stored for this Provider.",
      discoveredModels: models,
    };
  }
}

const NATIVE_PROVIDER_IDS = new Set([
  "openai",
  "anthropic",
  "google",
  "grok",
  "deepseek",
  "ollama",
]);

const adapters = new Map(
  DESCRIPTORS.map((descriptor) => [
    descriptor.providerId,
    NATIVE_PROVIDER_IDS.has(descriptor.providerId)
      ? new NativeProviderAdapter(descriptor)
      : new CatalogProviderAdapter(descriptor),
  ]),
);

export function listProviderAdapters(): ProviderAdapterDescriptor[] {
  return DESCRIPTORS;
}

export function getProviderAdapter(providerId: ProviderId): ProviderAdapterContract {
  return (
    adapters.get(providerId) ??
    new CatalogProviderAdapter({
      providerId,
      displayName: "Custom Provider",
      credentialKinds: ["api-key"],
      capabilities: ["chat"],
      supportsModelDiscovery: false,
      supportsTokenRefresh: false,
    })
  );
}

export function getProviderInstance(instanceId: string): ProviderInstance | undefined {
  return listProviderInstances().find((instance) => instance.id === instanceId);
}
