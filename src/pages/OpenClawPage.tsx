import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import InlineAlert from "../components/InlineAlert";
import ConfirmDialog from "../components/ConfirmDialog";
import type {
  AsyncStatus,
  OpenClawConnectionResult,
  OpenClawRemoteStatus,
  OpenClawRuntimeConfig,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

type OpenClawPageProps = {
  servers: OpenClawServer[];
  activeServer: OpenClawServer | null;

  enabledCount: number;
  connectedCount: number;
  autoConnectCount: number;
  averageLatencyMs: number | null;

  status: AsyncStatus;
  busyServerId: string | null;
  testingServerId: string | null;

  isTestingAll: boolean;
  isImporting: boolean;
  isExporting: boolean;

  remoteStatus: OpenClawRemoteStatus | null;
  runtimeConfig: OpenClawRuntimeConfig | null;

  searchText: string;
  error: string;
  cardStyle: CSSProperties;

  onSearchChange: (
    value: string,
  ) => void;

  onRefresh: () => void;

  onCreate: (
    server: OpenClawServerInput,
  ) => Promise<OpenClawServer | null>;

  onUpdate: (
    id: string,
    server: OpenClawServerInput,
  ) => Promise<OpenClawServer | null>;

  onDelete: (
    id: string,
  ) => void;

  onDuplicate: (
    id: string,
  ) => Promise<OpenClawServer | null>;

  onToggle: (
    id: string,
    enabled: boolean,
  ) => void;

  onActivate: (
    id: string,
  ) => void;

  onTestSaved: (
    id: string,
  ) => Promise<OpenClawConnectionResult>;

  onTestUnsaved: (
    server: OpenClawServerInput,
  ) => Promise<OpenClawConnectionResult>;

  onTestAll: () =>
    Promise<OpenClawConnectionResult[]>;

  onCopyUrl: (
    server: OpenClawServer,
  ) => void;

  onExport: (
    includeSecrets: boolean,
  ) => Promise<string>;

  onImport: (
    options: {
      json: string;
      replaceExisting: boolean;
    },
  ) => Promise<unknown>;
};

const EMPTY_FORM: OpenClawServerInput = {
  name: "",
  serverUrl: "",
  gatewayToken: "",
  enabled: true,
  autoConnect: true,
};

function normalizeServerUrl(
  value: string,
): string {
  return value
    .trim()
    .replace(/\/+$/, "");
}

function formatDate(
  value?: string,
): string {
  if (!value) {
    return "Never";
  }

  const date =
    new Date(value);

  if (
    Number.isNaN(
      date.getTime(),
    )
  ) {
    return value;
  }

  return date.toLocaleString();
}

function connectionLabel(
  server: OpenClawServer,
): string {
  switch (
    server.connectionState
  ) {
    case "testing":
      return "Testing";

    case "connected":
      return "Connected";

    case "unauthorized":
      return "Unauthorized";

    case "unreachable":
      return "Unreachable";

    case "pairing-required":
      return "Pairing Required";

    case "error":
      return "Error";

    default:
      return "Not Tested";
  }
}

function connectionIcon(
  server: OpenClawServer,
): string {
  switch (
    server.connectionState
  ) {
    case "testing":
      return "🟡";

    case "connected":
      return "🟢";

    case "unauthorized":
    case "pairing-required":
      return "🟠";

    case "unreachable":
    case "error":
      return "🔴";

    default:
      return "⚪";
  }
}

function OpenClawPage({
  servers,
  activeServer,
  enabledCount,
  connectedCount,
  autoConnectCount,
  averageLatencyMs,
  status,
  busyServerId,
  testingServerId,
  isTestingAll,
  isImporting,
  isExporting,
  remoteStatus,
  runtimeConfig,
  searchText,
  error,
  cardStyle,
  onSearchChange,
  onRefresh,
  onCreate,
  onUpdate,
  onDelete,
  onDuplicate,
  onToggle,
  onActivate,
  onTestSaved,
  onTestUnsaved,
  onTestAll,
  onCopyUrl,
  onExport,
  onImport,
}: OpenClawPageProps) {
  const [
    formOpen,
    setFormOpen,
  ] = useState(false);

  const [
    editingId,
    setEditingId,
  ] = useState<string | null>(
    null,
  );

  const [
    form,
    setForm,
  ] = useState<OpenClawServerInput>(
    EMPTY_FORM,
  );

  const [
    formError,
    setFormError,
  ] = useState("");

  const [
    testMessage,
    setTestMessage,
  ] = useState("");

  const [
    showToken,
    setShowToken,
  ] = useState(false);

  const [
    confirmDelete,
    setConfirmDelete,
  ] = useState<string | null>(
    null,
  );

  const [
    transferOpen,
    setTransferOpen,
  ] = useState(false);

  const [
    transferMode,
    setTransferMode,
  ] = useState<
    "import" | "export"
  >("export");

  const [
    transferJson,
    setTransferJson,
  ] = useState("");

  const [
    transferError,
    setTransferError,
  ] = useState("");

  const [
    includeSecrets,
    setIncludeSecrets,
  ] = useState(false);

  const [
    replaceExisting,
    setReplaceExisting,
  ] = useState(false);

  const isLoading =
    status === "loading";

  const disabledCount =
    servers.length -
    enabledCount;

  const activeStatusText =
    useMemo(() => {
      if (!activeServer) {
        return "No Active Server";
      }

      if (
        remoteStatus?.connected
      ) {
        return "Connected";
      }

      return connectionLabel(
        activeServer,
      );
    }, [
      activeServer,
      remoteStatus,
    ]);

  function resetForm() {
    setForm(
      EMPTY_FORM,
    );

    setEditingId(
      null,
    );

    setFormError("");
    setTestMessage("");
    setShowToken(false);
    setFormOpen(false);
  }

  function openCreateForm() {
    setForm(
      EMPTY_FORM,
    );

    setEditingId(
      null,
    );

    setFormError("");
    setTestMessage("");
    setShowToken(false);
    setFormOpen(true);
  }

  function openEditForm(
    server: OpenClawServer,
  ) {
    setEditingId(
      server.id,
    );

    setForm({
      name:
        server.name,

      serverUrl:
        server.serverUrl,

      gatewayToken:
        "",

      enabled:
        server.enabled,

      autoConnect:
        server.autoConnect,
    });

    setFormError("");
    setTestMessage("");
    setShowToken(false);
    setFormOpen(true);
  }

  function validateForm():
    OpenClawServerInput | null {
    const name =
      form.name.trim();

    const serverUrl =
      normalizeServerUrl(
        form.serverUrl,
      );

    if (!name) {
      setFormError(
        "Server name is required.",
      );

      return null;
    }

    if (!serverUrl) {
      setFormError(
        "Server URL is required.",
      );

      return null;
    }

    try {
      const parsed =
        new URL(serverUrl);

      if (
        ![
          "http:",
          "https:",
          "ws:",
          "wss:",
        ].includes(
          parsed.protocol,
        )
      ) {
        throw new Error(
          "Unsupported protocol",
        );
      }
    } catch {
      setFormError(
        "Enter a valid HTTP, HTTPS, WS or WSS URL.",
      );

      return null;
    }

    if (
      !editingId &&
      !form.gatewayToken.trim()
    ) {
      setFormError(
        "Gateway Token is required for a new server.",
      );

      return null;
    }

    return {
      name,
      serverUrl,

      gatewayToken:
        form.gatewayToken.trim(),

      enabled:
        form.enabled,

      autoConnect:
        form.autoConnect,
    };
  }

  async function testForm() {
    const payload =
      validateForm();

    if (!payload) {
      return;
    }

    try {
      setFormError("");

      setTestMessage(
        "Testing connection...",
      );

      const result =
        await onTestUnsaved(
          payload,
        );

      setTestMessage(
        result.message,
      );

      if (
        !result.success
      ) {
        setFormError(
          result.message,
        );
      }
    } catch (
      nextError
    ) {
      setFormError(
        String(nextError),
      );

      setTestMessage("");
    }
  }

  const isFormValid =
    form.name.trim().length > 0 &&
    normalizeServerUrl(
      form.serverUrl,
    ).length > 0;

  async function submitForm() {
    const payload =
      validateForm();

    if (!payload) {
      return;
    }

    try {
      setFormError("");

      if (editingId) {
        await onUpdate(
          editingId,
          payload,
        );
      } else {
        await onCreate(
          payload,
        );
      }

      resetForm();
    } catch (
      nextError
    ) {
      setFormError(
        String(nextError),
      );
    }
  }

  function closeTransfer() {
    if (
      isImporting ||
      isExporting
    ) {
      return;
    }

    setTransferOpen(
      false,
    );

    setTransferJson("");
    setTransferError("");
    setIncludeSecrets(false);
    setReplaceExisting(false);
  }

  function openExport() {
    setTransferMode(
      "export",
    );

    setTransferJson("");
    setTransferError("");
    setIncludeSecrets(false);
    setTransferOpen(true);
  }

  function openImport() {
    setTransferMode(
      "import",
    );

    setTransferJson("");
    setTransferError("");
    setReplaceExisting(false);
    setTransferOpen(true);
  }

  async function createExport() {
    try {
      setTransferError("");

      const json =
        await onExport(
          includeSecrets,
        );

      setTransferJson(
        json,
      );
    } catch (
      nextError
    ) {
      setTransferError(
        String(nextError),
      );
    }
  }

  async function runImport() {
    if (
      !transferJson.trim()
    ) {
      setTransferError(
        "Paste an OpenClaw export document first.",
      );

      return;
    }

    try {
      setTransferError("");

      await onImport({
        json:
          transferJson,

        replaceExisting,
      });

      closeTransfer();
    } catch (
      nextError
    ) {
      setTransferError(
        String(nextError),
      );
    }
  }

  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>
            OpenClaw Manager
          </h2>

          <p>
            Manage local and remote
            OpenClaw Gateway endpoints.
          </p>
        </div>

        <div className="openclaw-header-actions">
          <button
            type="button"
            className="secondary-button"
            disabled={
              isLoading ||
              isTestingAll
            }
            onClick={() => {
              void onTestAll();
            }}
          >
            {isTestingAll
              ? "Testing All..."
              : "Test All"}
          </button>

          <button
            type="button"
            className="secondary-button"
            disabled={isLoading}
            onClick={openImport}
          >
            Import
          </button>

          <button
            type="button"
            className="secondary-button"
            disabled={isLoading}
            onClick={openExport}
          >
            Export
          </button>

          <button
            type="button"
            className="secondary-button"
            disabled={isLoading}
            onClick={onRefresh}
          >
            {isLoading
              ? "Refreshing..."
              : "↻ Refresh"}
          </button>

          <button
            type="button"
            className="action-button backup-button"
            disabled={isLoading}
            onClick={openCreateForm}
          >
            ＋ Add Server
          </button>
        </div>
      </div>

      <div className="openclaw-summary-grid">
        <div
          className="openclaw-summary-card"
          style={cardStyle}
        >
          <span>
            Total Servers
          </span>

          <strong>
            {servers.length}
          </strong>
        </div>

        <div
          className="openclaw-summary-card"
          style={cardStyle}
        >
          <span>
            Connected
          </span>

          <strong>
            {connectedCount}
          </strong>
        </div>

        <div
          className="openclaw-summary-card"
          style={cardStyle}
        >
          <span>
            Auto Connect
          </span>

          <strong>
            {autoConnectCount}
          </strong>
        </div>

        <div
          className="openclaw-summary-card"
          style={cardStyle}
        >
          <span>
            Average Latency
          </span>

          <strong>
            {averageLatencyMs === null
              ? "—"
              : `${averageLatencyMs} ms`}
          </strong>
        </div>
      </div>

      <div
        className="openclaw-active-card"
        style={cardStyle}
      >
        <div className="openclaw-active-heading">
          <div>
            <span className="openclaw-card-icon">
              🦞
            </span>

            <div>
              <h3>
                Active Gateway
              </h3>

              <p>
                All unified OpenClaw
                requests use this
                endpoint.
              </p>
            </div>
          </div>

          <span
            className={[
              "openclaw-active-status",

              remoteStatus?.connected
                ? "openclaw-status-connected"
                : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            {remoteStatus?.connected
              ? "🟢"
              : "⚪"}{" "}
            {activeStatusText}
          </span>
        </div>

        {activeServer ? (
          <div className="openclaw-active-details">
            <div>
              <span>
                Server
              </span>

              <strong>
                {activeServer.name}
              </strong>
            </div>

            <div>
              <span>
                Runtime Mode
              </span>

              <strong>
                {runtimeConfig?.mode
                  ?? "Unknown"}
              </strong>
            </div>

            <div>
              <span>
                Version
              </span>

              <strong>
                {remoteStatus?.version
                  ?? activeServer.version
                  ?? "Unknown"}
              </strong>
            </div>

            <div>
              <span>
                Latency
              </span>

              <strong>
                {remoteStatus?.latencyMs
                  ?? activeServer.latencyMs
                  ?? "—"}{" "}
                {(
                  remoteStatus?.latencyMs
                  ?? activeServer.latencyMs
                ) !== undefined
                  ? "ms"
                  : ""}
              </strong>
            </div>
          </div>
        ) : (
          <div className="openclaw-no-active">
            No active OpenClaw server.
            Add or enable a server to
            continue.
          </div>
        )}
      </div>

      <div className="openclaw-toolbar">
        <div>
          <strong>
            {enabledCount}
          </strong>{" "}
          enabled ·{" "}
          <strong>
            {disabledCount}
          </strong>{" "}
          disabled
        </div>

        <input
          type="search"
          className="openclaw-search"
          value={searchText}
          placeholder="Search name, URL, version or state..."
          onChange={(event) =>
            onSearchChange(
              event.target.value,
            )
          }
        />
      </div>

      <InlineAlert message={error} />

      {servers.length === 0 ? (
        <div
          className="openclaw-empty-state"
          style={cardStyle}
        >
          <span>
            🦞
          </span>

          <h3>
            No OpenClaw servers
          </h3>

          <p>
            Add a local or remote
            Gateway using its URL and
            Token.
          </p>

          <button
            type="button"
            className="action-button backup-button"
            onClick={openCreateForm}
          >
            Add First Server
          </button>
        </div>
      ) : (
        <div className="openclaw-grid">
          {servers.map(
            (server) => {
              const busy =
                busyServerId ===
                server.id;

              const testing =
                testingServerId ===
                server.id;

              return (
                <article
                  key={server.id}
                  className={[
                    "openclaw-card",

                    server.active
                      ? "openclaw-card-active"
                      : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  style={cardStyle}
                >
                  <div className="openclaw-card-header">
                    <div className="openclaw-card-title">
                      <span className="openclaw-card-icon">
                        🦞
                      </span>

                      <div>
                        <div className="openclaw-name-row">
                          <h3>
                            {server.name}
                          </h3>

                          {server.active && (
                            <span className="openclaw-active-badge">
                              Active
                            </span>
                          )}
                        </div>

                        <p>
                          {server.serverUrl}
                        </p>
                      </div>
                    </div>

                    <label className="openclaw-switch">
                      <input
                        type="checkbox"
                        checked={
                          server.enabled
                        }
                        disabled={busy}
                        onChange={(event) =>
                          onToggle(
                            server.id,
                            event.target.checked,
                          )
                        }
                      />

                      <span>
                        {busy
                          ? "Updating..."
                          : server.enabled
                            ? "Enabled"
                            : "Disabled"}
                      </span>
                    </label>
                  </div>

                  <div className="openclaw-connection-row">
                    <span>
                      {connectionIcon(
                        server,
                      )}{" "}
                      {testing
                        ? "Testing..."
                        : connectionLabel(
                            server,
                          )}
                    </span>

                    <small>
                      {server.connectionMessage ||
                        "Connection has not been tested."}
                    </small>
                  </div>

                  <div className="openclaw-meta-grid">
                    <div>
                      <span>
                        Version
                      </span>

                      <strong>
                        {server.version
                          ?? "Unknown"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Latency
                      </span>

                      <strong>
                        {typeof server.latencyMs ===
                        "number"
                          ? `${server.latencyMs} ms`
                          : "—"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Last Checked
                      </span>

                      <strong>
                        {formatDate(
                          server.lastCheckedAt,
                        )}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Gateway Token
                      </span>

                      <strong>
                        {server.hasGatewayToken
                          ? "Configured"
                          : "Missing"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Auto Connect
                      </span>

                      <strong>
                        {server.autoConnect
                          ? "On"
                          : "Off"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Gateway ID
                      </span>

                      <strong>
                        {server.gatewayId
                          ?? "Unknown"}
                      </strong>
                    </div>
                  </div>

                  <div className="openclaw-card-actions">
                    {!server.active && (
                      <button
                        type="button"
                        className="action-button health-button"
                        disabled={
                          busy ||
                          !server.enabled
                        }
                        onClick={() =>
                          onActivate(
                            server.id,
                          )
                        }
                      >
                        Set Active
                      </button>
                    )}

                    <button
                      type="button"
                      className="secondary-button"
                      disabled={
                        busy ||
                        testing ||
                        !server.enabled
                      }
                      onClick={() => {
                        void onTestSaved(
                          server.id,
                        );
                      }}
                    >
                      {testing
                        ? "Testing..."
                        : "Test"}
                    </button>

                    <button
                      type="button"
                      className="secondary-button"
                      disabled={busy}
                      onClick={() =>
                        onCopyUrl(
                          server,
                        )
                      }
                    >
                      Copy URL
                    </button>

                    <button
                      type="button"
                      className="secondary-button"
                      disabled={busy}
                      onClick={() => {
                        void onDuplicate(
                          server.id,
                        );
                      }}
                    >
                      Duplicate
                    </button>

                    <button
                      type="button"
                      className="secondary-button"
                      disabled={busy}
                      onClick={() =>
                        openEditForm(
                          server,
                        )
                      }
                    >
                      Edit
                    </button>

                    <button
                      type="button"
                      className="danger-button"
                      disabled={busy}
                      onClick={() =>
                        setConfirmDelete(
                          server.id,
                        )
                      }
                    >
                      Delete
                    </button>
                  </div>
                </article>
              );
            },
          )}
        </div>
      )}

      {formOpen && (
        <div className="openclaw-modal-backdrop">
          <div
            className="openclaw-modal"
            style={cardStyle}
            role="dialog"
            aria-modal="true"
          >
            <div className="openclaw-modal-header">
              <div>
                <h3>
                  {editingId
                    ? "Edit OpenClaw Server"
                    : "Add OpenClaw Server"}
                </h3>

                <p>
                  Configure the Gateway
                  endpoint and
                  authentication.
                </p>
              </div>

              <button
                type="button"
                className="secondary-button"
                onClick={resetForm}
              >
                Close
              </button>
            </div>

            <label className="setting-field">
              <span>
                Name
              </span>

              <input
                type="text"
                value={form.name}
                aria-invalid={
                  formOpen &&
                  !form.name.trim()
                }
                placeholder="Home OpenClaw"
                onChange={(event) => {
                  setForm(
                    (current) => ({
                      ...current,
                      name:
                        event.target.value,
                    }),
                  );

                  setFormError("");
                }}
              />
            </label>

            <label className="setting-field">
              <span>
                Server URL
              </span>

              <input
                type="url"
                value={form.serverUrl}
                placeholder="http://127.0.0.1:18789"
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      serverUrl:
                        event.target.value,
                    }),
                  )
                }
              />
            </label>

            <label className="setting-field">
              <span>
                Gateway Token
              </span>

              <small>
                {editingId
                  ? "Leave blank to keep the existing Token."
                  : "The Token is stored by the Rust backend."}
              </small>

              <div className="openclaw-token-field">
                <input
                  type={
                    showToken
                      ? "text"
                      : "password"
                  }
                  value={
                    form.gatewayToken
                  }
                  autoComplete="off"
                  onChange={(event) =>
                    setForm(
                      (current) => ({
                        ...current,
                        gatewayToken:
                          event.target.value,
                      }),
                    )
                  }
                />

                <button
                  type="button"
                  className="secondary-button"
                  onClick={() =>
                    setShowToken(
                      (current) =>
                        !current,
                    )
                  }
                >
                  {showToken
                    ? "Hide"
                    : "Show"}
                </button>
              </div>
            </label>

            <label className="openclaw-option-row">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      enabled:
                        event.target.checked,
                    }),
                  )
                }
              />

              Enable this server
            </label>

            <label className="openclaw-option-row">
              <input
                type="checkbox"
                checked={
                  form.autoConnect
                }
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      autoConnect:
                        event.target.checked,
                    }),
                  )
                }
              />

              Automatically monitor this
              server
            </label>

            {testMessage && (
              <div className="openclaw-test-message">
                {testMessage}
              </div>
            )}

            {formError && (
              <div className="openclaw-error">
                {formError}
              </div>
            )}

            <div className="openclaw-modal-actions">
              <button
                type="button"
                className="secondary-button"
                onClick={resetForm}
              >
                Cancel
              </button>

              <button
                type="button"
                className="secondary-button"
                disabled={
                  testingServerId ===
                  "__new__"
                }
                onClick={() => {
                  void testForm();
                }}
              >
                {testingServerId ===
                "__new__"
                  ? "Testing..."
                  : "Test Connection"}
              </button>

              <button
                type="button"
                className="action-button backup-button"
                disabled={
                  isLoading ||
                  !isFormValid
                }
                onClick={() => {
                  void submitForm();
                }}
              >
                {editingId
                  ? "Save Changes"
                  : "Add Server"}
              </button>
            </div>
          </div>
        </div>
      )}

      {transferOpen && (
        <div className="openclaw-modal-backdrop">
          <div
            className="openclaw-modal"
            style={cardStyle}
            role="dialog"
            aria-modal="true"
          >
            <div className="openclaw-modal-header">
              <div>
                <h3>
                  {transferMode ===
                  "export"
                    ? "Export Servers"
                    : "Import Servers"}
                </h3>

                <p>
                  {transferMode ===
                  "export"
                    ? "Create a portable JSON configuration."
                    : "Paste an exported OpenClaw JSON document."}
                </p>
              </div>

              <button
                type="button"
                className="secondary-button"
                onClick={closeTransfer}
              >
                Close
              </button>
            </div>

            {transferMode ===
            "export" ? (
              <>
                <label className="openclaw-option-row">
                  <input
                    type="checkbox"
                    checked={
                      includeSecrets
                    }
                    onChange={(event) =>
                      setIncludeSecrets(
                        event.target.checked,
                      )
                    }
                  />

                  Include Gateway Tokens
                  in export
                </label>

                <button
                  type="button"
                  className="secondary-button"
                  disabled={isExporting}
                  onClick={() => {
                    void createExport();
                  }}
                >
                  {isExporting
                    ? "Exporting..."
                    : "Generate JSON"}
                </button>
              </>
            ) : (
              <label className="openclaw-option-row">
                <input
                  type="checkbox"
                  checked={
                    replaceExisting
                  }
                  onChange={(event) =>
                    setReplaceExisting(
                      event.target.checked,
                    )
                  }
                />

                Replace all existing
                servers
              </label>
            )}

            <label className="setting-field">
              <span>
                JSON
              </span>

              <textarea
                rows={15}
                value={transferJson}
                readOnly={
                  transferMode ===
                  "export"
                }
                placeholder={
                  transferMode ===
                  "export"
                    ? "Click Generate JSON."
                    : "Paste export JSON here..."
                }
                onChange={(event) =>
                  setTransferJson(
                    event.target.value,
                  )
                }
              />
            </label>

            {transferError && (
              <div className="openclaw-error">
                {transferError}
              </div>
            )}

            <div className="openclaw-modal-actions">
              <button
                type="button"
                className="secondary-button"
                onClick={closeTransfer}
              >
                Cancel
              </button>

              {transferMode ===
                "import" && (
                <button
                  type="button"
                  className="action-button backup-button"
                  disabled={isImporting}
                  onClick={() => {
                    void runImport();
                  }}
                >
                  {isImporting
                    ? "Importing..."
                    : "Import Servers"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
      <ConfirmDialog
        open={confirmDelete !== null}
        title="Delete OpenClaw server?"
        message={
          confirmDelete
            ? `This will permanently delete "${
                servers.find(
                  (server) =>
                    server.id ===
                    confirmDelete,
                )?.name ??
                "this server"
              }". This action cannot be undone.`
            : ""
        }
        confirmLabel="Confirm Delete"
        busy={
          confirmDelete !== null &&
          busyServerId ===
            confirmDelete
        }
        onCancel={() =>
          setConfirmDelete(null)
        }
        onConfirm={() => {
          if (!confirmDelete) {
            return;
          }

          onDelete(confirmDelete);
          setConfirmDelete(null);
        }}
      />
    </section>
  );
}

export default OpenClawPage;