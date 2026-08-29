import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  configureBuiltinEbayConnection,
  connectCustomConnectionProvider,
  disconnectCustomConnectionProvider,
  getBuiltinEbayConnection,
  refreshCustomConnectionProvider,
  testCustomConnectionProvider,
} from "../services/externalConnectors";
import type {
  ConfigureBuiltinEbayInput,
  CustomConnectionProvider,
  ExternalCapabilityState,
} from "../types/externalConnector";

const stateLabel: Record<string, string> = {
  NOT_CONFIGURED: "Configuration Required",
  CONFIGURATION_INVALID: "Configuration Invalid",
  CONFIGURATION_VALIDATING: "Validating Configuration",
  DISCONNECTED: "Disconnected",
  CONNECTING: "Connecting",
  WAITING_FOR_USER: "Waiting for you",
  CONNECTED: "Connected",
  LOGIN_REQUIRED: "Login Required",
  BACKEND_BROKER_REQUIRED: "Managed Broker Not Provisioned",
  CAPABILITY_PARTIALLY_AVAILABLE: "Partially Available",
  ERROR: "Error",
};

function groupState(capabilities: ExternalCapabilityState[], prefix: string) {
  const items = capabilities.filter((capability) => capability.capabilityId.startsWith(`ebay.${prefix}.`));
  if (items.length === 0) return "Not verified";
  const available = items.filter((capability) => capability.available).length;
  if (available === items.length) return "Available";
  if (available > 0) return "Partially available";
  if (items.some((capability) => capability.approvalState === "REQUIRED")) return "Developer approval required";
  return items[0].unavailableReason ?? "Unavailable";
}

export default function ExternalConnectorConfiguration() {
  const [provider, setProvider] = useState<CustomConnectionProvider>();
  const [configuration, setConfiguration] = useState<ConfigureBuiltinEbayInput>({
    environment: "SANDBOX",
    brokerMode: "MANAGED",
    clientId: "",
    ruName: "",
    brokerUrl: "",
  });
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const polling = useRef<number | undefined>(undefined);

  async function load() {
    try {
      const current = await getBuiltinEbayConnection();
      setProvider(current);
      setConfiguration({
        environment: current.environment,
        brokerMode: current.publicConfiguration.brokerMode === "SELF_HOSTED" ? "SELF_HOSTED" : "MANAGED",
        clientId: current.publicConfiguration.clientId ?? "",
        ruName: current.publicConfiguration.ruName ?? "",
        brokerUrl: current.brokerUrl ?? "",
      });
    } catch (error) {
      setMessage(String(error));
    }
  }

  useEffect(() => {
    void load();
    return () => { if (polling.current) window.clearInterval(polling.current); };
  }, []);

  async function saveConfiguration() {
    setBusy("configure");
    try {
      const updated = await configureBuiltinEbayConnection(configuration);
      setProvider(updated);
      setMessage(updated.publicConfiguration.managedBrokerStatus === "NOT_PROVISIONED"
        ? "Managed Broker is not provisioned for this build. Choose Self-hosted or configure the product-managed Broker."
        : "eBay public configuration and Broker manifest were validated.");
    } catch (error) {
      setMessage(`Configuration validation failed: ${String(error)}`);
      await load();
    } finally {
      setBusy("");
    }
  }

  async function testConnection() {
    if (!provider) return;
    setBusy("test");
    try {
      const updated = await testCustomConnectionProvider(provider.providerInstanceId);
      setProvider(updated);
      setMessage("Broker health, reviewed manifest and eBay server configuration are valid.");
    } catch (error) {
      setMessage(`Test failed: ${String(error)}`);
    } finally {
      setBusy("");
    }
  }

  async function pollStatus() {
    if (!provider) return;
    try {
      const updated = await refreshCustomConnectionProvider(provider.providerInstanceId);
      setProvider(updated);
      if (["CONNECTED", "CAPABILITY_PARTIALLY_AVAILABLE"].includes(updated.connectionState)) {
        if (polling.current) window.clearInterval(polling.current);
        polling.current = undefined;
        setMessage("eBay authorization and identity were verified by the Broker.");
      }
    } catch (error) {
      setMessage(`Authorization status check failed: ${String(error)}`);
    }
  }

  async function connect() {
    if (!provider) return;
    setBusy("connect");
    try {
      const updated = await connectCustomConnectionProvider(provider.providerInstanceId);
      setProvider(updated);
      if (updated.pendingAuthorizationUrl) await openUrl(updated.pendingAuthorizationUrl);
      if (polling.current) window.clearInterval(polling.current);
      polling.current = window.setInterval(() => { void pollStatus(); }, 2500);
      setMessage("Complete sign-in and consent on the official eBay page. AI-OS will verify the Broker status automatically.");
    } catch (error) {
      setMessage(`Connect failed: ${String(error)}`);
    } finally {
      setBusy("");
    }
  }

  async function disconnect() {
    if (!provider) return;
    setBusy("disconnect");
    try {
      const result = await disconnectCustomConnectionProvider(provider.providerInstanceId);
      setMessage(result.message);
      if (result.localCleanupComplete) await load();
    } catch (error) {
      setMessage(`Disconnect failed: ${String(error)}`);
    } finally {
      setBusy("");
    }
  }

  const canConnect = provider?.configurationState === "DISCONNECTED"
    && !["BACKEND_BROKER_REQUIRED", "CONFIGURATION_INVALID", "NOT_CONFIGURED"].includes(provider.connectionState);
  const groups = useMemo(() => provider ? ["browse", "cart", "checkout", "order"].map((group) => [group, groupState(provider.capabilities, group)] as const) : [], [provider]);

  return <article className="connection-row ebay-connector-row">
    <div className="ebay-connector-summary">
      <strong>eBay</strong>
      <small>Official OAuth/API via Backend Broker</small>
      <small>{provider?.environment ?? "SANDBOX"} · {configuration.brokerMode === "MANAGED" ? "Managed by AI-OS" : "Self-hosted / Bring Your Own App"}</small>
      <div className="connector-capability-grid">{groups.map(([group, status]) => <span key={group}><b>{group[0].toUpperCase() + group.slice(1)}</b>{status}</span>)}</div>
    </div>
    <span className={`connection-badge ${provider?.connectionState === "CONNECTED" ? "connection-badge-ready" : ""}`}>{stateLabel[provider?.connectionState ?? "NOT_CONFIGURED"] ?? provider?.connectionState}</span>
    <div className="provider-actions">
      <details className="ebay-configuration-details">
        <summary className="provider-secondary">Configure</summary>
        <div className="ebay-configuration-panel">
          <label>Broker deployment<select value={configuration.brokerMode} onChange={(event) => setConfiguration((current) => ({ ...current, brokerMode: event.target.value as "MANAGED" | "SELF_HOSTED" }))}><option value="MANAGED">Managed by AI-OS</option><option value="SELF_HOSTED">Self-hosted / Bring Your Own App</option></select></label>
          <label>Environment<select value={configuration.environment} onChange={(event) => setConfiguration((current) => ({ ...current, environment: event.target.value as "SANDBOX" | "PRODUCTION" }))}><option value="SANDBOX">Sandbox</option><option value="PRODUCTION">Production</option></select></label>
          {configuration.brokerMode === "SELF_HOSTED" && <><label>App ID / Client ID<input value={configuration.clientId} onChange={(event) => setConfiguration((current) => ({ ...current, clientId: event.target.value }))} /></label><label>RuName<input value={configuration.ruName} onChange={(event) => setConfiguration((current) => ({ ...current, ruName: event.target.value }))} /></label><label>Backend Broker URL<input value={configuration.brokerUrl ?? ""} onChange={(event) => setConfiguration((current) => ({ ...current, brokerUrl: event.target.value }))} placeholder="https://broker.example.com/" /></label></>}
          <small>Secrets and tokens are configured only on the Broker. They are never entered or returned here.</small>
          <button type="button" className="provider-primary" disabled={Boolean(busy)} onClick={() => void saveConfiguration()}>{busy === "configure" ? "Validating…" : "Save and Validate"}</button>
        </div>
      </details>
      <button type="button" className="provider-secondary" disabled={Boolean(busy) || !provider || provider.configurationState === "NOT_CONFIGURED"} onClick={() => void testConnection()}>{busy === "test" ? "Testing…" : "Test"}</button>
      {provider && ["CONNECTED", "CAPABILITY_PARTIALLY_AVAILABLE", "WAITING_FOR_USER"].includes(provider.connectionState)
        ? <button type="button" className="provider-secondary" disabled={Boolean(busy)} onClick={() => void disconnect()}>{busy === "disconnect" ? "Disconnecting…" : "Disconnect Account"}</button>
        : <button type="button" className="provider-primary" disabled={Boolean(busy) || !canConnect} onClick={() => void connect()}>{busy === "connect" ? "Connecting…" : "Connect"}</button>}
    </div>
    {message && <p className="ebay-connector-message" role="status">{message}</p>}
  </article>;
}
