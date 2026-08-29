import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";

import {
  addCustomConnectionProvider,
  connectCustomConnectionProvider,
  disconnectCustomConnectionProvider,
  listCustomConnectionProviders,
  refreshCustomConnectionProvider,
  removeCustomConnectionProvider,
  testCustomConnectionProvider,
} from "../services/externalConnectors";
import type {
  AddCustomProviderInput,
  CustomConnectionProvider,
  CustomProviderKind,
} from "../types/externalConnector";

const stateLabels: Record<string, string> = {
  DISCONNECTED: "Disconnected",
  WAITING_FOR_USER: "Waiting for you",
  CONNECTED: "Connected",
  AUTHORIZATION_REQUIRED: "Authorization Required",
  BACKEND_BROKER_REQUIRED: "Backend Broker Required",
  CAPABILITY_PARTIALLY_AVAILABLE: "Partially Available",
  VERIFICATION_ADAPTER_REQUIRED: "Verification Adapter Required",
  ADAPTER_REQUIRED: "Adapter Required",
  APP_NOT_INSTALLED: "App Not Installed",
  ERROR: "Error",
};

const emptyInput = (): AddCustomProviderInput => ({
  providerKind: "LOCAL_APPLICATION",
  displayName: "",
  environment: "DEVELOPMENT",
  requestedCapabilities: [],
  publicConfiguration: {},
});

export default function AddConnectionProvider() {
  const [providers, setProviders] = useState<CustomConnectionProvider[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [input, setInput] = useState<AddCustomProviderInput>(emptyInput);
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  const [message, setMessage] = useState("");

  async function reload() {
    try {
      setProviders(await listCustomConnectionProviders());
    } catch (error) {
      setMessage(`Custom Provider configuration could not be loaded: ${String(error)}`);
    }
  }

  useEffect(() => { void reload(); }, []);

  function updateProvider(provider: CustomConnectionProvider) {
    setProviders((current) => current.map((item) => item.providerInstanceId === provider.providerInstanceId ? provider : item));
  }

  async function chooseApplication() {
    const selected = await open({ directory: false, multiple: false, filters: [{ name: "macOS Application", extensions: ["app"] }] });
    if (typeof selected === "string") setInput((current) => ({ ...current, applicationPath: selected, displayName: current.displayName || "Local Application" }));
  }

  async function addProvider() {
    setBusy((current) => ({ ...current, add: true }));
    try {
      const created = await addCustomConnectionProvider(input);
      setProviders((current) => [...current, created]);
      setInput(emptyInput());
      setShowForm(false);
      setMessage(`${created.displayName} was added.`);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy((current) => ({ ...current, add: false }));
    }
  }

  async function connect(provider: CustomConnectionProvider) {
    setBusy((current) => ({ ...current, [provider.providerInstanceId]: true }));
    try {
      const updated = await connectCustomConnectionProvider(provider.providerInstanceId);
      updateProvider(updated);
      if (updated.providerKind === "WEBSITE_LOGIN" && updated.officialLoginUrl) await openUrl(updated.officialLoginUrl);
      if (updated.pendingAuthorizationUrl) await openUrl(updated.pendingAuthorizationUrl);
      setMessage(updated.connectionState === "CONNECTED" ? `${updated.displayName} identity was verified.` : `${updated.displayName} is waiting for authorization or a verification adapter.`);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy((current) => ({ ...current, [provider.providerInstanceId]: false }));
    }
  }

  async function refresh(provider: CustomConnectionProvider) {
    setBusy((current) => ({ ...current, [provider.providerInstanceId]: true }));
    try {
      updateProvider(await refreshCustomConnectionProvider(provider.providerInstanceId));
      setMessage(`${provider.displayName} status was refreshed from its runtime provider.`);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy((current) => ({ ...current, [provider.providerInstanceId]: false }));
    }
  }

  async function testConfiguration(provider: CustomConnectionProvider) {
    setBusy((current) => ({ ...current, [provider.providerInstanceId]: true }));
    try {
      updateProvider(await testCustomConnectionProvider(provider.providerInstanceId));
      setMessage(`${provider.displayName} Broker health and public configuration are valid.`);
    } catch (error) {
      setMessage(`Configuration test failed: ${String(error)}`);
    } finally {
      setBusy((current) => ({ ...current, [provider.providerInstanceId]: false }));
    }
  }

  async function disconnect(provider: CustomConnectionProvider) {
    setBusy((current) => ({ ...current, [provider.providerInstanceId]: true }));
    try {
      const result = await disconnectCustomConnectionProvider(provider.providerInstanceId);
      setMessage(result.message);
      if (result.localCleanupComplete) await reload();
    } catch (error) {
      setMessage(`Disconnect failed: ${String(error)}`);
    } finally {
      setBusy((current) => ({ ...current, [provider.providerInstanceId]: false }));
    }
  }

  async function removeProvider(provider: CustomConnectionProvider) {
    const confirmed = window.confirm(`Remove ${provider.displayName}? This removes its public configuration, local authorization reference, capability state and custom Provider definition. Remote authorization must be revoked first.`);
    if (!confirmed) return;
    setBusy((current) => ({ ...current, [provider.providerInstanceId]: true }));
    try {
      if (!["DISCONNECTED", "ADAPTER_REQUIRED", "VERIFICATION_ADAPTER_REQUIRED", "APP_NOT_INSTALLED"].includes(provider.connectionState)) {
        const disconnected = await disconnectCustomConnectionProvider(provider.providerInstanceId);
        if (!disconnected.localCleanupComplete || !disconnected.remoteRevokeComplete) {
          setMessage(disconnected.message);
          return;
        }
      }
      await removeCustomConnectionProvider(provider.providerInstanceId);
      setProviders((current) => current.filter((item) => item.providerInstanceId !== provider.providerInstanceId));
      setMessage(`${provider.displayName} was removed.`);
    } catch (error) {
      setMessage(`Remove failed: ${String(error)}`);
    } finally {
      setBusy((current) => ({ ...current, [provider.providerInstanceId]: false }));
    }
  }

  function setKind(providerKind: CustomProviderKind) {
    setInput({ ...emptyInput(), providerKind, environment: providerKind === "EXTERNAL_API_CONNECTOR" ? "SANDBOX" : "DEVELOPMENT" });
  }

  return (
    <section className="custom-connections" aria-labelledby="custom-connections-heading">
      <div className="provider-section-heading">
        <div><h3 id="custom-connections-heading">Other Providers</h3><span>Local apps, verified website sessions, trusted Broker connectors and provider requests.</span></div>
        <button type="button" className="provider-primary" onClick={() => setShowForm((shown) => !shown)}>Add Other Provider</button>
      </div>
      {showForm && <div className="custom-provider-form">
        <label>Provider type<select value={input.providerKind} onChange={(event) => setKind(event.target.value as CustomProviderKind)}><option value="LOCAL_APPLICATION">Local Application</option><option value="WEBSITE_LOGIN">Website Login</option><option value="EXTERNAL_API_CONNECTOR">External API Connector</option><option value="UNSUPPORTED_REQUEST">Unsupported Provider Request</option></select></label>
        <label>Display name<input value={input.displayName} onChange={(event) => setInput((current) => ({ ...current, displayName: event.target.value }))} /></label>
        {input.providerKind === "LOCAL_APPLICATION" && <label>Application<button type="button" className="provider-secondary" onClick={() => void chooseApplication()}>{input.applicationPath || "Choose .app…"}</button></label>}
        {input.providerKind === "WEBSITE_LOGIN" && <><label>Official website URL<input value={input.officialWebsiteUrl ?? ""} onChange={(event) => setInput((current) => ({ ...current, officialWebsiteUrl: event.target.value }))} placeholder="https://example.com" /></label><label>Official login URL<input value={input.officialLoginUrl ?? ""} onChange={(event) => setInput((current) => ({ ...current, officialLoginUrl: event.target.value }))} placeholder="https://example.com/login" /></label></>}
        {input.providerKind === "EXTERNAL_API_CONNECTOR" && <>
          <small>Reviewed built-in API Connectors are configured in their main Connections row. eBay is already built in and cannot be added again. No additional trusted Connector is currently available.</small>
        </>}
        {input.providerKind === "UNSUPPORTED_REQUEST" && <><label>Official website<input value={input.officialWebsiteUrl ?? ""} onChange={(event) => setInput((current) => ({ ...current, officialWebsiteUrl: event.target.value }))} /></label><label>Requested features<textarea value={input.requestedCapabilities.join("\n")} onChange={(event) => setInput((current) => ({ ...current, requestedCapabilities: event.target.value.split("\n").map((value) => value.trim()).filter(Boolean) }))} /></label></>}
        <div className="provider-actions"><button type="button" className="provider-secondary" onClick={() => setShowForm(false)}>Cancel</button><button type="button" className="provider-primary" disabled={busy.add || input.providerKind === "EXTERNAL_API_CONNECTOR"} onClick={() => void addProvider()}>{busy.add ? "Adding…" : "Add Provider"}</button></div>
      </div>}
      <div className="connections-list">
        {providers.map((provider) => <article className="connection-row custom-connection-row" key={provider.providerInstanceId}>
          <div><strong>{provider.displayName}</strong><small>{provider.providerKind.replaceAll("_", " ")} · {provider.environment}</small>{provider.providerKind === "EXTERNAL_API_CONNECTOR" && <small>Official OAuth/API via Backend Broker</small>}{provider.capabilities.length > 0 && <small>{provider.capabilities.filter((capability) => capability.available).length}/{provider.capabilities.length} capabilities available</small>}</div>
          <span className={`connection-badge ${provider.connectionState === "CONNECTED" ? "connection-badge-ready" : ""}`}>{stateLabels[provider.connectionState] ?? provider.connectionState}</span>
          <div className="provider-actions">{provider.providerKind === "EXTERNAL_API_CONNECTOR" && <button type="button" className="provider-secondary" disabled={busy[provider.providerInstanceId]} onClick={() => void testConfiguration(provider)}>Test</button>}<button type="button" className="provider-secondary" disabled={busy[provider.providerInstanceId]} onClick={() => void refresh(provider)}>Refresh</button>{provider.connectionState === "CONNECTED" || provider.opaqueAuthorizationReference ? <button type="button" className="provider-secondary" disabled={busy[provider.providerInstanceId]} onClick={() => void disconnect(provider)}>Disconnect Account</button> : <button type="button" className="provider-primary" disabled={busy[provider.providerInstanceId] || ["ADAPTER_REQUIRED", "UNTRUSTED_CONNECTOR", "CONFIGURATION_INVALID"].includes(provider.connectionState)} onClick={() => void connect(provider)}>Connect</button>}<button type="button" className="provider-secondary danger-button" disabled={busy[provider.providerInstanceId]} onClick={() => void removeProvider(provider)}>Remove Provider</button></div>
        </article>)}
      </div>
      {message && <p className="connections-message" role="status">{message}</p>}
    </section>
  );
}
