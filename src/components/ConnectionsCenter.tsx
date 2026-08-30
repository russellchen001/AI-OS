import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  cancelProviderOAuth,
  createProviderInstance,
  deleteProviderCredential,
  getProviderOAuthConfiguration,
  listProviderInstances,
  removeProviderInstance,
  saveProviderInstance,
  startProviderOAuth,
  type ProviderOAuthCompletedEvent,
  type ProviderOAuthErrorEvent,
} from "../services/providers";
import { getProviderAdapter } from "../services/providerAdapters";
import AddConnectionProvider from "./AddConnectionProvider";

type ConnectionMethod = "OFFICIAL_OAUTH" | "AUTHENTICATED_BROWSER" | "BACKEND_BROKER" | "NATIVE_APPLICATION";
type ConnectionState =
  | "NOT_CONFIGURED"
  | "DISCONNECTED"
  | "CONNECTING"
  | "WAITING_FOR_USER"
  | "CONNECTED"
  | "EXPIRED"
  | "LOGIN_REQUIRED"
  | "AUTHORIZATION_REQUIRED"
  | "DEVELOPER_APPROVAL_REQUIRED"
  | "BACKEND_BROKER_REQUIRED"
  | "APP_NOT_INSTALLED"
  | "ERROR";

type ConnectionCapability = {
  providerId: string;
  displayName: string;
  method: ConnectionMethod;
  state: ConnectionState;
  loginUrl?: string;
  profileRef?: string;
  developerApprovalRequired: boolean;
};

type BrowserLoginSession = {
  providerId: string;
  profileRef: string;
  startedAt: string;
  lastVerifiedAt?: string;
  state: ConnectionState;
  verifiedOrigin?: string;
  accountMarker?: string;
};

type BrowserSite = {
  providerId: string;
  displayName: string;
  loginUrl: string;
  accountUrl?: string;
  hosts: string[];
  evidence?: { signedInKeys: string[]; signedOutKeys: string[]; verifiedOrigin: string; learnedAt: string };
  createdAt: string;
};

const emptySite = { displayName: "", loginUrl: "", accountUrl: "" };

type LocalApplicationAvailability = {
  applicationId: string;
  displayName: string;
  installed: boolean;
};

type LocalApplicationAvailabilityReport = {
  applications: LocalApplicationAvailability[];
  iworkState: ConnectionState;
};

const order = [
  "microsoft-graph",
  "google-workspace",
  "wps-office",
  "ebay",
  "amazon-consumer",
  "taobao-consumer",
  "jd-consumer",
  "pinduoduo-consumer",
  "apple-iwork",
];

const labels: Record<ConnectionState, string> = {
  NOT_CONFIGURED: "Configuration Required",
  DISCONNECTED: "Disconnected",
  CONNECTING: "Connecting",
  WAITING_FOR_USER: "Waiting for you",
  CONNECTED: "Connected",
  EXPIRED: "Expired",
  LOGIN_REQUIRED: "Login Required",
  AUTHORIZATION_REQUIRED: "Authorization Required",
  DEVELOPER_APPROVAL_REQUIRED: "Developer Approval Required",
  BACKEND_BROKER_REQUIRED: "Backend Broker Required",
  APP_NOT_INSTALLED: "App Not Installed",
  ERROR: "Error",
};

export default function ConnectionsCenter() {
  const [capabilities, setCapabilities] = useState<ConnectionCapability[]>([]);
  const [states, setStates] = useState<Record<string, ConnectionState>>({});
  const [sessions, setSessions] = useState<Record<string, BrowserLoginSession>>({});
  const [connectAllIndex, setConnectAllIndex] = useState<number | null>(null);
  const [message, setMessage] = useState("");
  const [microsoftClientId, setMicrosoftClientId] = useState("");
  const [localApplications, setLocalApplications] = useState<LocalApplicationAvailability[]>([]);
  const [sites, setSites] = useState<BrowserSite[]>([]);
  const [siteForm, setSiteForm] = useState(emptySite);
  const [showSiteForm, setShowSiteForm] = useState(false);
  const [rescanning, setRescanning] = useState(false);
  const [scanStatus, setScanStatus] = useState("");
  const running = useRef(new Set<string>());

  async function refreshConnections(showMessage = false) {
    setRescanning(true);
    setScanStatus(showMessage ? "Checking installed apps…" : "");
    try {
      const [items, availability, addedSites] = await Promise.all([
        invoke<ConnectionCapability[]>("list_connection_capabilities"),
        invoke<LocalApplicationAvailabilityReport>("rescan_local_application_availability"),
        invoke<BrowserSite[]>("list_browser_sites"),
      ]);
      setCapabilities(items);
      setSites(addedSites);
      setLocalApplications(availability.applications);
      const instances = listProviderInstances();
      setStates(Object.fromEntries(items.map((item) => {
        const instance = instances.find((candidate) => candidate.providerId === item.providerId);
        const state: ConnectionState = instance?.connectionState === "connected"
          ? "CONNECTED"
          : instance?.connectionState === "expired" || instance?.connectionState === "refresh-required"
            ? "EXPIRED"
            : item.providerId === "apple-iwork"
              ? availability.iworkState
              : (item.providerId === "microsoft-graph" || item.providerId === "google-workspace") && !getProviderOAuthConfiguration(item.providerId, "configuration-check")
              ? "NOT_CONFIGURED"
              : item.state;
        return [item.providerId, state];
      })));
      if (showMessage) {
        setScanStatus("Installed app status updated.");
        setMessage("Local application availability was rescanned from this Mac.");
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      setScanStatus("App rescan failed.");
      setMessage(`App rescan failed: ${detail}`);
    } finally {
      setRescanning(false);
    }
  }

  useEffect(() => {
    void refreshConnections();
  }, []);

  // Restart recovery re-verifies persisted accounts in the background. The
  // backend is the single authority, so a refresh landing later reads the same
  // answer and cannot overwrite this with a stale Expired.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<{ providerId: string; state: ConnectionState }>(
      "browser-connection://recovered",
      ({ payload }) => {
        setStates((current) => ({ ...current, [payload.providerId]: payload.state }));
      },
    ).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten?.();
  }, []);

  const currentProvider = connectAllIndex === null ? undefined : order[connectAllIndex];

  useEffect(() => {
    if (!currentProvider || running.current.has(currentProvider)) return;
    const state = states[currentProvider];
    if (state === "CONNECTED" || state === "NOT_CONFIGURED" || state === "DEVELOPER_APPROVAL_REQUIRED" || state === "BACKEND_BROKER_REQUIRED" || state === "APP_NOT_INSTALLED" || state === "AUTHORIZATION_REQUIRED") {
      setConnectAllIndex((index) => index === null || index + 1 >= order.length ? null : index + 1);
      return;
    }
    if (state === "CONNECTING" || state === "WAITING_FOR_USER") return;
    void connect(currentProvider);
  }, [currentProvider, states]);

  useEffect(() => {
    const waiting = Object.values(sessions).filter((session) => session.state === "WAITING_FOR_USER");
    if (waiting.length === 0) return;
    const timer = window.setInterval(() => {
      for (const session of waiting) void verifyBrowser(session);
    }, 2500);
    return () => window.clearInterval(timer);
  }, [sessions]);

  async function connectOfficial(providerId: "microsoft-graph" | "google-workspace") {
    const instanceId = `${providerId}-default`;
    const displayName = providerId === "microsoft-graph" ? "Microsoft Graph" : "Google Workspace";
    setStates((current) => ({ ...current, [providerId]: "CONNECTING" }));
    let completedUnlisten: UnlistenFn | undefined;
    let errorUnlisten: UnlistenFn | undefined;
    let oauthState = "";
    try {
      if (!getProviderOAuthConfiguration(providerId, instanceId)) {
        setStates((current) => ({ ...current, [providerId]: "NOT_CONFIGURED" }));
        return;
      }
      const completion = new Promise<ProviderOAuthCompletedEvent>(async (resolve, reject) => {
        completedUnlisten = await listen<ProviderOAuthCompletedEvent>("provider-oauth://completed", ({ payload }) => {
          if (payload.providerId === providerId && payload.state === oauthState) resolve(payload);
        });
        errorUnlisten = await listen<ProviderOAuthErrorEvent>("provider-oauth://error", ({ payload }) => {
          if (payload.providerId === providerId && payload.state === oauthState) reject(new Error(payload.message));
        });
      });
      const oauth = await startProviderOAuth(providerId, instanceId);
      oauthState = oauth.state;
      setStates((current) => ({ ...current, [providerId]: "WAITING_FOR_USER" }));
      await openUrl(oauth.authorizationUrl);
      const completed = await completion;
      await (await getProviderAdapter(providerId)).testConnection(instanceId);
      await invoke(providerId === "microsoft-graph" ? "get_microsoft_graph_identity" : "get_google_workspace_identity");
      await saveProviderInstance(createProviderInstance({
        id: instanceId,
        providerId,
        displayName,
        credentialKind: "oauth",
        models: [],
        defaultModelId: "",
        liveTested: true,
        credentialExpiresAt: completed.expiresAt ?? undefined,
        credentialRefreshable: completed.refreshable,
      }));
      setStates((current) => ({ ...current, [providerId]: "CONNECTED" }));
      setMessage(`${displayName} account verified. Continuing onboarding.`);
    } catch (error) {
      if (oauthState) await cancelProviderOAuth({ providerId, state: oauthState }).catch(() => false);
      setStates((current) => ({ ...current, [providerId]: "ERROR" }));
      setMessage(String(error));
    } finally {
      completedUnlisten?.();
      errorUnlisten?.();
    }
  }

  async function connectBrowser(capability: ConnectionCapability) {
    setStates((current) => ({ ...current, [capability.providerId]: "CONNECTING" }));
    setMessage(`Opening the AI-OS managed browser for ${capability.displayName}…`);
    // begin_browser_login opens the AI-OS managed browser itself. The system
    // default browser is deliberately not used: AI-OS can only verify an
    // authenticated session it owns.
    const session = await invoke<BrowserLoginSession>("begin_browser_login", {
      providerId: capability.providerId,
    });
    setSessions((current) => ({ ...current, [capability.providerId]: session }));
    setStates((current) => ({ ...current, [capability.providerId]: "WAITING_FOR_USER" }));
    setMessage(`AI-OS opened its own browser window for ${capability.displayName}. Sign in there on your own regional site; AI-OS verifies the account state itself.`);
  }

  async function verifyBrowser(session: BrowserLoginSession) {
    if (running.current.has(`verify:${session.providerId}`)) return;
    running.current.add(`verify:${session.providerId}`);
    try {
      const checked = await invoke<BrowserLoginSession>("verify_browser_login", { session });
      setSessions((current) => ({ ...current, [session.providerId]: checked }));
      setStates((current) => ({ ...current, [session.providerId]: checked.state }));
      if (checked.state === "CONNECTED") {
        setMessage(`${session.providerId} account verified on ${checked.verifiedOrigin ?? "the signed-in site"}. Continuing onboarding.`);
      }
      if (checked.state === "EXPIRED") {
        setMessage(`${session.providerId} is no longer signed in. Reconnect to sign in again.`);
      }
    } finally {
      running.current.delete(`verify:${session.providerId}`);
    }
  }

  async function connect(providerId: string) {
    if (running.current.has(providerId)) {
      // Opening a managed browser takes seconds. Say so, rather than letting
      // the click look like it did nothing.
      setMessage("Still finishing the previous attempt for this provider. Give it a moment.");
      return;
    }
    const capability = capabilities.find((item) => item.providerId === providerId);
    if (!capability) {
      setMessage(`${providerId} is not available in this Connections list. Rescan and try again.`);
      return;
    }
    running.current.add(providerId);
    try {
      await refreshConnections();
      if (providerId === "microsoft-graph" || providerId === "google-workspace") await connectOfficial(providerId);
      else if (providerId === "apple-iwork") {
        setStates((current) => ({ ...current, [providerId]: "CONNECTING" }));
        await invoke<LocalApplicationAvailabilityReport>("connect_apple_iwork");
        setStates((current) => ({ ...current, [providerId]: "CONNECTED" }));
        setMessage("Apple iWork applications are installed and authorized.");
      }
      else if (capability.developerApprovalRequired) {
        setStates((current) => ({ ...current, [providerId]: "DEVELOPER_APPROVAL_REQUIRED" }));
      } else await connectBrowser(capability);
    } catch (error) {
      setStates((current) => ({ ...current, [providerId]: "ERROR" }));
      setMessage(String(error));
    } finally {
      running.current.delete(providerId);
    }
  }

  const addedSite = (providerId: string) => sites.find((site) => site.providerId === providerId);

  async function addSite() {
    try {
      await invoke<BrowserSite>("add_browser_site", {
        displayName: siteForm.displayName,
        loginUrl: siteForm.loginUrl,
        accountUrl: siteForm.accountUrl.trim() === "" ? null : siteForm.accountUrl,
      });
      setSiteForm(emptySite);
      setShowSiteForm(false);
      await refreshConnections();
      setMessage(`${siteForm.displayName} added. Connect it to sign in.`);
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function removeSite(providerId: string) {
    try {
      await invoke<boolean>("remove_browser_site", { providerId });
      await refreshConnections();
      setMessage("Site removed, along with its stored browser session.");
    } catch (error) {
      setMessage(String(error));
    }
  }

  // A site AI-OS has never seen cannot be checked from its markup, so the user
  // says when they are signed in and AI-OS learns the difference from the page.
  async function confirmSignedIn(providerId: string) {
    setMessage("Checking the page…");
    try {
      const checked = await invoke<BrowserLoginSession>("confirm_browser_login", { providerId });
      setSessions((current) => ({ ...current, [providerId]: checked }));
      setStates((current) => ({ ...current, [providerId]: checked.state }));
      setMessage(`${providerId} verified on ${checked.verifiedOrigin ?? "the signed-in page"}.`);
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function disconnect(providerId: string) {
    try {
      if (providerId === "microsoft-graph" || providerId === "google-workspace") {
        const instanceId = `${providerId}-default`;
        await deleteProviderCredential(instanceId);
        await removeProviderInstance(instanceId);
      }
      await invoke<boolean>("disconnect_connection_provider", { providerId });
      setSessions((current) => {
        const next = { ...current };
        delete next[providerId];
        return next;
      });
      setStates((current) => ({ ...current, [providerId]: "DISCONNECTED" }));
      setMessage(`${providerId} disconnected. The stored browser session was removed.`);
    } catch (error) {
      // A failed disconnect must not look like a successful one.
      setStates((current) => ({ ...current, [providerId]: "ERROR" }));
      setMessage(String(error));
    }
  }

  function skipCurrent() {
    if (connectAllIndex === null) return;
    setConnectAllIndex(connectAllIndex + 1 >= order.length ? null : connectAllIndex + 1);
  }

  function saveMicrosoftDevelopmentConfig() {
    const value = microsoftClientId.trim();
    if (!/^[0-9a-f-]{36}$/i.test(value)) {
      setMessage("Enter the public Microsoft Application (client) ID in UUID format.");
      return;
    }
    window.localStorage.setItem("ai-os.microsoft.client-id", value);
    setStates((current) => ({ ...current, "microsoft-graph": "DISCONNECTED" }));
    setMicrosoftClientId("");
    setMessage("Microsoft application identity configured for this development build.");
  }

  const complete = useMemo(() => capabilities.length > 0 && capabilities.every((item) => {
    const state = states[item.providerId];
    return state === "CONNECTED" || state === "NOT_CONFIGURED" || state === "DEVELOPER_APPROVAL_REQUIRED" || state === "BACKEND_BROKER_REQUIRED" || state === "APP_NOT_INSTALLED" || state === "AUTHORIZATION_REQUIRED";
  }), [capabilities, states]);

  return (
    <section className="connections-center" aria-labelledby="connections-heading">
      <div className="provider-section-heading">
        <div>
          <h2 id="connections-heading">Connections</h2>
          <span>AI-OS opens official sign-in pages and verifies every connected account.</span>
        </div>
        <div className="provider-actions">
          <button type="button" className="provider-secondary" disabled={rescanning} onClick={() => void refreshConnections(true)}>
            {rescanning ? "Rescanning…" : "Rescan Apps"}
          </button>
          {scanStatus && <span className="connection-scan-status" role="status">{scanStatus}</span>}
          <button type="button" className="provider-primary" disabled={connectAllIndex !== null} onClick={() => setConnectAllIndex(0)}>
            {connectAllIndex === null ? "Connect All" : "Connecting…"}
          </button>
        </div>
      </div>
      <div className="connections-list">
        {capabilities.map((capability) => {
          const state = states[capability.providerId] ?? capability.state;
          const active = currentProvider === capability.providerId;
          return (
            <article key={capability.providerId} className="connection-row">
              <div>
                <strong>{capability.displayName}</strong>
                <small>{capability.method === "OFFICIAL_OAUTH" ? "Official OAuth/API" : capability.method === "AUTHENTICATED_BROWSER" ? "Authenticated Browser" : capability.method === "BACKEND_BROKER" ? "Official API via Backend Broker" : "Native Application"}</small>
                {capability.providerId === "apple-iwork" && <small>{localApplications.filter((app) => ["pages", "numbers", "keynote"].includes(app.applicationId)).map((app) => `${app.displayName}: ${app.installed ? "Installed" : "Not installed"}`).join(" · ")}</small>}
                {capability.providerId === "wps-office" && <small>Local app: {localApplications.find((app) => app.applicationId === "wps-office")?.installed ? "Installed" : "Not installed"}</small>}
                {capability.providerId === "microsoft-graph" && <small>Microsoft Excel: {localApplications.find((app) => app.applicationId === "microsoft-excel")?.installed ? "Installed" : "Not installed"}</small>}
              </div>
              <span className={`connection-badge ${state === "CONNECTED" ? "connection-badge-ready" : ""}`}>{labels[state]}</span>
              <div className="provider-actions">
                {state === "CONNECTED" ? (
                  <button type="button" className="provider-secondary" onClick={() => void disconnect(capability.providerId)}>Disconnect</button>
                ) : (
                  <button type="button" className="provider-primary" disabled={state === "CONNECTING" || state === "DEVELOPER_APPROVAL_REQUIRED" || state === "BACKEND_BROKER_REQUIRED" || state === "APP_NOT_INSTALLED"} onClick={() => void connect(capability.providerId)}>
                    {state === "EXPIRED" || state === "LOGIN_REQUIRED" || state === "ERROR" ? "Reconnect" : "Connect"}
                  </button>
                )}
                {(addedSite(capability.providerId) || capability.providerId === "ebay") && state === "WAITING_FOR_USER" && (
                  <button type="button" className="provider-secondary" onClick={() => void confirmSignedIn(capability.providerId)}>
                    I&apos;ve signed in
                  </button>
                )}
                {addedSite(capability.providerId) && (
                  <button type="button" className="provider-secondary" onClick={() => void removeSite(capability.providerId)}>Remove</button>
                )}
                {active && <button type="button" className="provider-secondary" onClick={skipCurrent}>Skip</button>}
              </div>
            </article>
          );
        })}
      </div>
      <AddConnectionProvider />
      <details className="connection-development-settings">
        <summary>Advanced developer settings</summary>
        <div className="connection-development-config">
          <p>Application-level public configuration only. Never enter an account password, token, or client secret here.</p>
          <label>
            <span>Microsoft Application (client) ID</span>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={microsoftClientId}
              onChange={(event) => setMicrosoftClientId(event.target.value)}
              placeholder="00000000-0000-0000-0000-000000000000"
            />
          </label>
          <button type="button" className="provider-secondary" onClick={saveMicrosoftDevelopmentConfig}>Save configuration</button>
        </div>
      </details>
      <div className="connections-add-site">
        {showSiteForm ? (
          <>
            <input
              type="text"
              placeholder="Site name, e.g. My Shop"
              value={siteForm.displayName}
              onChange={(event) => setSiteForm({ ...siteForm, displayName: event.target.value })}
            />
            <input
              type="url"
              placeholder="Sign-in address, e.g. https://example.com/login"
              value={siteForm.loginUrl}
              onChange={(event) => setSiteForm({ ...siteForm, loginUrl: event.target.value })}
            />
            <input
              type="url"
              placeholder="Optional: your account page, e.g. https://example.com/my"
              value={siteForm.accountUrl}
              onChange={(event) => setSiteForm({ ...siteForm, accountUrl: event.target.value })}
            />
            <button type="button" className="provider-primary" onClick={() => void addSite()}>Add site</button>
            <button type="button" className="provider-secondary" onClick={() => { setShowSiteForm(false); setSiteForm(emptySite); }}>Cancel</button>
          </>
        ) : (
          <button type="button" className="provider-secondary" onClick={() => setShowSiteForm(true)}>Add a site</button>
        )}
      </div>
      {message && <p className="connections-message" role="status">{message}</p>}
      <p className="connections-message">Enter credentials only on the official login page. AI-OS never asks for an account password.</p>
      {complete && <p className="connections-message">Connection onboarding finished. Items requiring application configuration or developer approval remain clearly marked.</p>}
    </section>
  );
}
