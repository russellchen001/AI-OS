export type ConnectorEnvironment = "SANDBOX" | "PRODUCTION" | "DEVELOPMENT";
export type CustomProviderKind = "LOCAL_APPLICATION" | "WEBSITE_LOGIN" | "EXTERNAL_API_CONNECTOR" | "UNSUPPORTED_REQUEST";

export type ExternalConnectionState =
  | "NOT_CONFIGURED"
  | "CONFIGURATION_INVALID"
  | "CONFIGURATION_VALIDATING"
  | "DISCONNECTED"
  | "CONNECTING"
  | "WAITING_FOR_USER"
  | "CONNECTED"
  | "EXPIRED"
  | "LOGIN_REQUIRED"
  | "AUTHORIZATION_REQUIRED"
  | "DEVELOPER_APPROVAL_REQUIRED"
  | "BACKEND_BROKER_REQUIRED"
  | "CAPABILITY_PARTIALLY_AVAILABLE"
  | "VERIFICATION_ADAPTER_REQUIRED"
  | "ADAPTER_REQUIRED"
  | "UNTRUSTED_CONNECTOR"
  | "APP_NOT_INSTALLED"
  | "ERROR";

export type ExternalCapabilityState = {
  capabilityId: string;
  available: boolean;
  environment: ConnectorEnvironment;
  authorizationState: string;
  approvalState: string;
  requiredScopes: string[];
  confirmationRequired: boolean;
  unavailableReason?: string;
  lastVerifiedAt?: string;
  evidenceReference?: string;
};

export type CustomConnectionProvider = {
  providerInstanceId: string;
  providerDefinitionId: string;
  source: "BUILT_IN" | "CUSTOM";
  providerKind: CustomProviderKind;
  displayName: string;
  environment: ConnectorEnvironment;
  configurationState: ExternalConnectionState;
  connectionState: ExternalConnectionState;
  authorizationKind: string;
  opaqueAuthorizationReference?: string;
  pendingAuthorizationUrl?: string;
  browserProfileReference?: string;
  capabilities: ExternalCapabilityState[];
  publicConfiguration: Record<string, string>;
  officialWebsiteUrl?: string;
  officialLoginUrl?: string;
  brokerUrl?: string;
  localBundleId?: string;
  localBundlePath?: string;
  localBundleVersion?: string;
  createdAt: string;
  updatedAt: string;
  lastCheckedAt?: string;
};

export type AddCustomProviderInput = {
  providerKind: CustomProviderKind;
  displayName: string;
  environment: ConnectorEnvironment;
  officialWebsiteUrl?: string;
  officialLoginUrl?: string;
  brokerUrl?: string;
  applicationPath?: string;
  requestedCapabilities: string[];
  publicConfiguration: Record<string, string>;
};

export type ConfigureBuiltinEbayInput = {
  environment: ConnectorEnvironment;
  brokerMode: "MANAGED" | "SELF_HOSTED";
  clientId: string;
  ruName: string;
  brokerUrl?: string;
};

export type DisconnectExternalProviderResult = {
  localCleanupComplete: boolean;
  remoteRevokeComplete: boolean;
  connectionState: ExternalConnectionState;
  message: string;
};
