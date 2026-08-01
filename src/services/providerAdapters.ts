import { invoke } from "@tauri-apps/api/core";

import type {
  ProviderAdapterContract,
  ProviderAdapterDescriptor,
  ProviderConnectionTest,
  ProviderId,
  ProviderInstance,
  ProviderModelEntry,
} from "../types/provider";

import {
  discoverKnownModels,
  getProviderCredentialStatus,
  listProviderInstances,
} from "./providers";


type NativeAdapterResult = {
  providerId: string;
  level: "credential" | "live";
  message: string;
  models: Array<{
    id: string;
    displayName: string;
  }>;
};


let descriptorCache:
  | ProviderAdapterDescriptor[]
  | undefined;

let descriptorRequest:
  | Promise<ProviderAdapterDescriptor[]>
  | undefined;


async function loadProviderAdapterDescriptors():
Promise<ProviderAdapterDescriptor[]> {

  descriptorRequest ??=
    invoke<ProviderAdapterDescriptor[]>(
      "list_provider_adapters",
    ).then((descriptors) => {
      descriptorCache = descriptors;
      return descriptors;
    }).finally(() => {
      descriptorRequest = undefined;
    });

  return descriptorRequest;
}


function modelsFromNativeResult(
  instanceId: string,
  descriptor: ProviderAdapterDescriptor,
  result: NativeAdapterResult,
): ProviderModelEntry[] {

  return result.models.map(
    (model, index) => ({
      id: `${instanceId}:${model.id}`,
      providerInstanceId: instanceId,
      remoteModelId: model.id,
      displayName: model.displayName,
      capabilities: descriptor.capabilities,
      enabled: true,
      isDefault: index === 0,
    }),
  );
}


class NativeProviderAdapter
implements ProviderAdapterContract {

  constructor(
    private readonly adapterDescriptor:
      ProviderAdapterDescriptor,
  ) {}


  descriptor():
  ProviderAdapterDescriptor {
    return this.adapterDescriptor;
  }


  async connect(
    instance: ProviderInstance,
  ): Promise<ProviderInstance> {
    return instance;
  }


  async disconnect():
  Promise<void> {
    throw new Error(
      "Disconnect is not available until this Provider Adapter is enabled.",
    );
  }


  async refreshCredential():
  Promise<ProviderInstance> {
    throw new Error(
      "Token refresh is not available until OAuth is enabled.",
    );
  }


  async discoverModels(
    instanceId: string,
  ): Promise<ProviderModelEntry[]> {

    const result =
      await invoke<NativeAdapterResult>(
        "discover_provider_models",
        {
          query: {
            providerId:
              this.adapterDescriptor.providerId,
            providerInstanceId:
              instanceId,
          },
        },
      );

    return modelsFromNativeResult(
      instanceId,
      this.adapterDescriptor,
      result,
    );
  }


  async testConnection(
    instanceId: string,
  ): Promise<ProviderConnectionTest> {

    const result =
      await invoke<NativeAdapterResult>(
        "test_provider_connection",
        {
          query: {
            providerId:
              this.adapterDescriptor.providerId,
            providerInstanceId:
              instanceId,
          },
        },
      );

    return {
      ok: true,
      level: result.level,
      checkedAt:
        new Date().toISOString(),
      message: result.message,
      discoveredModels:
        modelsFromNativeResult(
          instanceId,
          this.adapterDescriptor,
          result,
        ),
    };
  }
}


class CatalogProviderAdapter
implements ProviderAdapterContract {

  constructor(
    private readonly adapterDescriptor:
      ProviderAdapterDescriptor,
  ) {}


  descriptor():
  ProviderAdapterDescriptor {
    return this.adapterDescriptor;
  }


  async connect(
    instance: ProviderInstance,
  ): Promise<ProviderInstance> {
    return instance;
  }


  async disconnect():
  Promise<void> {}


  async refreshCredential():
  Promise<ProviderInstance> {
    throw new Error(
      "Token refresh is not available for this Provider.",
    );
  }


  async discoverModels(
    instanceId: string,
  ): Promise<ProviderModelEntry[]> {
    return discoverKnownModels(
      this.adapterDescriptor.providerId,
      instanceId,
    );
  }


  async testConnection(
    instanceId: string,
  ): Promise<ProviderConnectionTest> {

    const credential =
      await getProviderCredentialStatus(
        instanceId,
      );

    const models =
      await this.discoverModels(
        instanceId,
      );

    return {
      ok: credential.hasCredential,
      level: "credential",
      checkedAt:
        new Date().toISOString(),
      message:
        credential.hasCredential
          ? "Credential saved. A native Adapter is required for live testing."
          : "No credential is stored for this Provider.",
      discoveredModels: models,
    };
  }
}


function fallbackDescriptor(
  providerId: ProviderId,
): ProviderAdapterDescriptor {

  return {
    providerId,
    displayName:
      "Custom Provider",
    adapterKind: "catalog",
    credentialKinds: [
      "api-key",
    ],
    authenticationMethods: [
      "api-key",
    ],
    capabilities: [
      "chat",
    ],
    supportsModelDiscovery:
      false,
    supportsTokenRefresh:
      false,
    supportsMultipleCredentials:
      false,
  };
}


function createAdapter(
  descriptor:
    ProviderAdapterDescriptor,
): ProviderAdapterContract {

  return descriptor.adapterKind ===
    "native"
    ? new NativeProviderAdapter(
        descriptor,
      )
    : new CatalogProviderAdapter(
        descriptor,
      );
}


export async function listProviderAdapters():
Promise<ProviderAdapterDescriptor[]> {

  return [
    ...(descriptorCache ??
      await loadProviderAdapterDescriptors()),
  ];
}


export async function getProviderAdapter(
  providerId: ProviderId,
): Promise<ProviderAdapterContract> {

  const descriptors =
    descriptorCache ??
    await loadProviderAdapterDescriptors();

  const descriptor =
    descriptors.find(
      (candidate) =>
        candidate.providerId ===
        providerId,
    ) ??
    fallbackDescriptor(
      providerId,
    );

  return createAdapter(
    descriptor,
  );
}


export function getProviderInstance(
  instanceId: string,
): ProviderInstance | undefined {

  return listProviderInstances().find(
    (instance) =>
      instance.id === instanceId,
  );
}
