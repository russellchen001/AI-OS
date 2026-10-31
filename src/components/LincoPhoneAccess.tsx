import { useEffect, useState } from "react";
import {
  getLincoConnectorStatus,
  LINCO_STATE_LABELS,
  pairLincoConnector,
  unpairLincoConnector,
  type LincoConnectorStatus,
} from "../services/lincoConnector";

const POLL_MS = 3000;

export default function LincoPhoneAccess() {
  const [status, setStatus] = useState<LincoConnectorStatus>();
  const [commands, setCommands] = useState("");
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
      setStatus(await pairLincoConnector(commands));
      setCommands("");
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
          <label htmlFor="linco-commands">
            In the Linco App open <strong>Bridge</strong>, choose an Agent, copy the generated commands and paste them here.
            AI‑OS reads only the pairing token and Agent slot; nothing is installed or run.
          </label>
          <textarea
            id="linco-commands"
            rows={4}
            spellCheck={false}
            autoComplete="off"
            value={commands}
            placeholder={'linco-connect init --token "<app-id>:<app-secret>" --agent codex'}
            onChange={(event) => setCommands(event.target.value)}
          />
          <div className="provider-actions">
            <button type="button" className="provider-primary" disabled={busy || !commands.trim()} onClick={() => void pair()}>
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
