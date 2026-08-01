export type ProviderId =
  | "openai"
  | "anthropic"
  | "google"
  | "grok"
  | "deepseek"
  | "doubao"
  | "kimi"
  | "meta"
  | "ollama"
  | (string & {});

export type ProviderCredentialKind =
  | "oauth"
  | "api-key"
  | "local";

export type ProviderAuthenticationMethod =
  | "api-key"
  | "oauth-pkce"
  | "oauth-loopback"
  | "device-code"
  | "imported-credential"
  | "local";

export type ProviderConnectionState =
  | "not-configured"
  | "connecting"
  | "ready-for-test"
  | "connected"
  | "refresh-required"
  | "expired"
  | "error";

export type ProviderCapability =
  | "chat"
  | "reasoning"
  | "vision"
  | "audio"
  | "image-generation"
  | "tool-use"
  | "embeddings";

export type ProviderModelEntry = {
  id: string;
  providerInstanceId: string;
  remoteModelId: string;
  displayName: string;
  capabilities: ProviderCapability[];
  contextWindow?: number;
  enabled: boolean;
  isDefault: boolean;
};

export type ProviderCredentialRef = {
  kind: ProviderCredentialKind;
  keychainAccount?: string;
  expiresAt?: string;
  refreshable: boolean;
};

export type ProviderInstance = {
  id: string;
  providerId: ProviderId;
  displayName: string;
  credential: ProviderCredentialRef;
  connectionState: ProviderConnectionState;
  models: ProviderModelEntry[];
  createdAt: string;
  updatedAt: string;
  lastTestedAt?: string;
};

export type ProviderAdapterKind =
  | "native"
  | "catalog";

export type ProviderAdapterDescriptor = {
  providerId: ProviderId;
  displayName: string;
  adapterKind: ProviderAdapterKind;
  credentialKinds: ProviderCredentialKind[];
  authenticationMethods: ProviderAuthenticationMethod[];
  capabilities: ProviderCapability[];
  supportsModelDiscovery: boolean;
  supportsTokenRefresh: boolean;
  supportsMultipleCredentials: boolean;
};

export type ProviderConnectionTest = {
  ok: boolean;
  level: "credential" | "live";
  checkedAt: string;
  message: string;
  discoveredModels: ProviderModelEntry[];
};

export interface ProviderAdapterContract {
  descriptor(): ProviderAdapterDescriptor;
  connect(instance: ProviderInstance): Promise<ProviderInstance>;
  disconnect(instanceId: string): Promise<void>;
  refreshCredential(instanceId: string): Promise<ProviderInstance>;
  discoverModels(instanceId: string): Promise<ProviderModelEntry[]>;
  testConnection(instanceId: string): Promise<ProviderConnectionTest>;
}
