import { invoke } from "@tauri-apps/api/core";

import type {
  AddCustomProviderInput,
  ConfigureBuiltinEbayInput,
  CustomConnectionProvider,
  DisconnectExternalProviderResult,
} from "../types/externalConnector";

export function getBuiltinEbayConnection() {
  return invoke<CustomConnectionProvider>("get_builtin_ebay_connection");
}

export function configureBuiltinEbayConnection(input: ConfigureBuiltinEbayInput) {
  return invoke<CustomConnectionProvider>("configure_builtin_ebay_connection", { input });
}

export function listCustomConnectionProviders() {
  return invoke<CustomConnectionProvider[]>("list_custom_connection_providers");
}

export function addCustomConnectionProvider(input: AddCustomProviderInput) {
  return invoke<CustomConnectionProvider>("add_custom_connection_provider", { input });
}

export function connectCustomConnectionProvider(providerInstanceId: string) {
  return invoke<CustomConnectionProvider>("connect_custom_connection_provider", { providerInstanceId });
}

export function testCustomConnectionProvider(providerInstanceId: string) {
  return invoke<CustomConnectionProvider>("test_custom_connection_provider", { providerInstanceId });
}

export function refreshCustomConnectionProvider(providerInstanceId: string) {
  return invoke<CustomConnectionProvider>("refresh_custom_connection_provider", { providerInstanceId });
}

export function disconnectCustomConnectionProvider(providerInstanceId: string) {
  return invoke<DisconnectExternalProviderResult>("disconnect_custom_connection_provider", { providerInstanceId });
}

export function removeCustomConnectionProvider(providerInstanceId: string) {
  return invoke<boolean>("remove_custom_connection_provider", { providerInstanceId });
}
