import { useEffect, useState } from "react";
import {
  getLincoConnectorStatus,
  LINCO_AGENT_SLOTS,
  LINCO_STATE_LABELS,
  pairLincoConnector,
  unpairLincoConnector,
  type LincoConnectorStatus,
} from "../services/lincoConnector";

const POLL_MS = 3000;

export default function LincoPhoneAccess() {
  const [status, setStatus] = useState<LincoConnectorStatus>();
  const [commands, setCommands] = useState("");
  const [agentType, setAgentType] = useState<string>("openclaw");
  const [appId, setAppId] = useState("");
  const [appSecret, setAppSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let alive = true;
    const refresh = () =>
      getLincoConnectorStatus()
        .then((next) => alive && setStatus(next))
        .catch(() => undefined);
    void refresh();
    const timer = window.setInterval(refresh, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  async function pair() {
    setBusy(true);
    setError("");
    try {
      const pairing = appId.trim() && appSecret.trim()
        ? `App ID: ${appId.trim()}\nApp Secret: ${appSecret.trim()}`
        : commands;
      setStatus(await pairLincoConnector(pairing, agentType));
      setCommands("");
      setAppId("");
      setAppSecret("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function unpair() {
    setBusy(true);
    setError("");
    try {
      setStatus(await unpairLincoConnector());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  const state = status?.state ?? "not-paired";
  const paired = state !== "not-paired";

  return (
    <section className="linco-phone-access" aria-labelledby="linco-heading">
      <div className="provider-section-heading">
        <div>
          <h2 id="linco-heading">Phone access (Linco)</h2>
          <span>Use AI‑OS from the official Linco App · built in, no extra program</span>
        </div>
      </div>
      <article className="connection-row">
        <div>
          <strong>Linco App</strong>
          <small>
            {paired
              ? `${status?.deviceName ?? "This Mac"} · Linco slot: ${status?.agentType ?? "—"} · reconnects automatically when AI‑OS starts`
              : "Pair once; AI‑OS then connects every time it starts."}
          </small>
          {status?.lastError && state !== "connected" && <small>{status.lastError}</small>}
        </div>
        <span className={`connection-badge ${state === "connected" ? "connection-badge-ready" : ""}`}>
          {LINCO_STATE_LABELS[state]}
        </span>
        <div className="provider-actions">
          {paired && (
            <button type="button" className="provider-secondary" disabled={busy} onClick={() => void unpair()}>
              Unpair
            </button>
          )}
        </div>
      </article>
      {!paired && (
        <div className="linco-pairing">
          <p>
            In the Linco App: <strong>消息 → + → 创建机器人</strong>, choose the same Agent below
            (OpenClaw recommended), then copy its App ID and App Secret here. AI‑OS stores them in
            Keychain; nothing is installed or run.
          </p>
          <label>
            Linco Agent
            <select value={agentType} disabled={busy} onChange={(event) => setAgentType(event.target.value)}>
              {LINCO_AGENT_SLOTS.map((slot) => <option key={slot.id} value={slot.id}>{slot.label}</option>)}
            </select>
          </label>
          <div className="linco-pairing-fields">
            <label>
              App ID
              <input type="text" autoComplete="off" spellCheck={false} value={appId}
                onChange={(event) => setAppId(event.target.value)} />
            </label>
            <label>
              App Secret
              <input type="password" autoComplete="off" spellCheck={false} value={appSecret}
                onChange={(event) => setAppSecret(event.target.value)} />
            </label>
          </div>
          <label htmlFor="linco-commands">Or paste what “一键复制配置信息” copied</label>
          <textarea
            id="linco-commands"
            rows={3}
            spellCheck={false}
            autoComplete="off"
            value={commands}
            onChange={(event) => setCommands(event.target.value)}
          />
          <div className="provider-actions">
            <button type="button" className="provider-primary"
              disabled={busy || !((appId.trim() && appSecret.trim()) || commands.trim())}
              onClick={() => void pair()}>
              {busy ? "Pairing…" : "Pair"}
            </button>
          </div>
        </div>
      )}
      {error && <p className="provider-setup-error" role="alert">{error}</p>}
      <p className="connections-message">
        The pairing token is stored in macOS Keychain. Messages from the Linco App go through AI‑OS's normal
        orchestration; actions that change anything still ask for /approve.
      </p>
    </section>
  );
}
