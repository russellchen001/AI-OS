import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import type {
  AsyncStatus,
  OpenClawConnectionResult,
  OpenClawRemoteStatus,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

type OpenClawPageProps = {
  servers: OpenClawServer[];
  activeServer: OpenClawServer | null;
  enabledCount: number;
  connectedCount: number;
  status: AsyncStatus;
  busyServerId: string | null;
  testingServerId: string | null;
  remoteStatus: OpenClawRemoteStatus | null;
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
  return value.trim().replace(
    /\/+$/,
    "",
  );
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

    case "error":
      return "Error";

    default:
      return "Not tested";
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
      return "🟠";

    case "unreachable":
    case "error":
      return "🔴";

    default:
      return "⚪";
  }
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

function OpenClawPage({
  servers,
  activeServer,
  enabledCount,
  connectedCount,
  status,
  busyServerId,
  testingServerId,
  remoteStatus,
  searchText,
  error,
  cardStyle,
  onSearchChange,
  onRefresh,
  onCreate,
  onUpdate,
  onDelete,
  onToggle,
  onActivate,
  onTestSaved,
  onTestUnsaved,
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

  const isLoading =
    status === "loading";

  const disabledCount =
    servers.length -
    enabledCount;

  const activeStatusText =
    useMemo(() => {
      if (!activeServer) {
        return "No active server";
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
    setForm(EMPTY_FORM);
    setEditingId(null);
    setFormError("");
    setTestMessage("");
    setShowToken(false);
    setFormOpen(false);
  }

  function openCreateForm() {
    setForm(EMPTY_FORM);
    setEditingId(null);
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
      name: server.name,
      serverUrl:
        server.serverUrl,

      // 留空代表保留后端已有 Token
      gatewayToken: "",

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
        parsed.protocol !==
          "http:" &&
        parsed.protocol !==
          "https:"
      ) {
        throw new Error(
          "Unsupported protocol",
        );
      }
    } catch {
      setFormError(
        "Enter a valid HTTP or HTTPS server URL.",
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

      if (!result.success) {
        setFormError(
          result.message,
        );
      }
    } catch (nextError) {
      const message =
        String(nextError);

      setFormError(message);
      setTestMessage("");
    }
  }

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
    } catch (nextError) {
      setFormError(
        String(nextError),
      );
    }
  }

  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>
            OpenClaw Servers
          </h2>

          <p>
            Connect AI OS to local or
            remote OpenClaw gateways.
          </p>
        </div>

        <div className="openclaw-header-actions">
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
            onClick={
              openCreateForm
            }
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
            Enabled
          </span>

          <strong>
            {enabledCount}
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
            Disabled
          </span>

          <strong>
            {disabledCount}
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
                Active OpenClaw
              </h3>

              <p>
                The active server is
                used by AI OS remote
                operations.
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
                Name
              </span>

              <strong>
                {activeServer.name}
              </strong>
            </div>

            <div>
              <span>
                Server URL
              </span>

              <strong>
                {
                  activeServer.serverUrl
                }
              </strong>
            </div>

            <div>
              <span>
                Token
              </span>

              <strong>
                {activeServer
                  .hasGatewayToken
                  ? "Configured"
                  : "Missing"}
              </strong>
            </div>

            <div>
              <span>
                Last Checked
              </span>

              <strong>
                {formatDate(
                  activeServer
                    .lastCheckedAt,
                )}
              </strong>
            </div>
          </div>
        ) : (
          <div className="openclaw-no-active">
            No active OpenClaw server.
            Add a server or mark an
            existing server as active.
          </div>
        )}

        {remoteStatus?.rawResponse && (
          <details className="openclaw-raw-status">
            <summary>
              Remote Response
            </summary>

            <pre>
              {
                remoteStatus.rawResponse
              }
            </pre>
          </details>
        )}
      </div>

      <div className="openclaw-toolbar">
        <input
          type="search"
          className="openclaw-search"
          value={searchText}
          placeholder="Search OpenClaw servers..."
          onChange={(event) =>
            onSearchChange(
              event.target.value,
            )
          }
        />
      </div>

      {error && (
        <div
          className="openclaw-error"
          role="alert"
        >
          {error}
        </div>
      )}

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
            OpenClaw gateway using its
            Server URL and Gateway
            Token.
          </p>

          <button
            type="button"
            className="action-button backup-button"
            onClick={
              openCreateForm
            }
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

              const deleting =
                confirmDelete ===
                server.id;

              return (
                <article
                  key={server.id}
                  className={[
                    "openclaw-card",
                    server.active
                      ? "openclaw-card-active"
                      : "",
                    server.enabled
                      ? "openclaw-card-enabled"
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
                            {
                              server.name
                            }
                          </h3>

                          {server.active && (
                            <span className="openclaw-active-badge">
                              Active
                            </span>
                          )}
                        </div>

                        <p>
                          {
                            server.serverUrl
                          }
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
                        onChange={(
                          event,
                        ) =>
                          onToggle(
                            server.id,
                            event.target
                              .checked,
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
                      {server
                        .connectionMessage ||
                        "Connection has not been tested."}
                    </small>
                  </div>

                  <div className="openclaw-meta-grid">
                    <div>
                      <span>
                        Gateway Token
                      </span>

                      <strong>
                        {server
                          .hasGatewayToken
                          ? "Configured"
                          : "Missing"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Auto Connect
                      </span>

                      <strong>
                        {server
                          .autoConnect
                          ? "On"
                          : "Off"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Last Checked
                      </span>

                      <strong>
                        {formatDate(
                          server
                            .lastCheckedAt,
                        )}
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
                      onClick={() =>
                        onTestSaved(
                          server.id,
                        )
                      }
                    >
                      {testing
                        ? "Testing..."
                        : "Test Connection"}
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

                    {deleting ? (
                      <>
                        <button
                          type="button"
                          className="danger-button"
                          disabled={busy}
                          onClick={() => {
                            onDelete(
                              server.id,
                            );

                            setConfirmDelete(
                              null,
                            );
                          }}
                        >
                          Confirm
                        </button>

                        <button
                          type="button"
                          className="secondary-button"
                          onClick={() =>
                            setConfirmDelete(
                              null,
                            )
                          }
                        >
                          Cancel
                        </button>
                      </>
                    ) : (
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
                    )}
                  </div>
                </article>
              );
            },
          )}
        </div>
      )}

      {formOpen && (
        <div
          className="openclaw-modal-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (
              event.target ===
                event.currentTarget &&
              !isLoading &&
              testingServerId !==
                "__new__"
            ) {
              resetForm();
            }
          }}
        >
          <div
            className="openclaw-modal"
            style={cardStyle}
            role="dialog"
            aria-modal="true"
            aria-label={
              editingId
                ? "Edit OpenClaw server"
                : "Add OpenClaw server"
            }
          >
            <div className="openclaw-modal-header">
              <div>
                <h3>
                  {editingId
                    ? "Edit OpenClaw Server"
                    : "Add OpenClaw Server"}
                </h3>

                <p>
                  Configure a local or
                  remote OpenClaw
                  gateway.
                </p>
              </div>

              <button
                type="button"
                className="secondary-button"
                disabled={
                  isLoading ||
                  testingServerId ===
                    "__new__"
                }
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
                disabled={isLoading}
                placeholder="Home OpenClaw"
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      name:
                        event.target
                          .value,
                    }),
                  )
                }
              />
            </label>

            <label className="setting-field">
              <span>
                Server URL
              </span>

              <small>
                Example:
                http://127.0.0.1:18789
                or
                https://openclaw.example.com
              </small>

              <input
                type="url"
                value={
                  form.serverUrl
                }
                disabled={isLoading}
                placeholder="http://127.0.0.1:18789"
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      serverUrl:
                        event.target
                          .value,
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
                  : "The Token is saved locally by the Rust backend."}
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
                  disabled={isLoading}
                  autoComplete="off"
                  placeholder={
                    editingId
                      ? "Leave blank to keep existing Token"
                      : "Paste Gateway Token"
                  }
                  onChange={(event) =>
                    setForm(
                      (current) => ({
                        ...current,
                        gatewayToken:
                          event.target
                            .value,
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
                disabled={isLoading}
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      enabled:
                        event.target
                          .checked,
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
                disabled={isLoading}
                onChange={(event) =>
                  setForm(
                    (current) => ({
                      ...current,
                      autoConnect:
                        event.target
                          .checked,
                    }),
                  )
                }
              />

              Automatically monitor this
              server when active
            </label>

            {testMessage && (
              <div className="openclaw-test-message">
                {testMessage}
              </div>
            )}

            {formError && (
              <div
                className="openclaw-error"
                role="alert"
              >
                {formError}
              </div>
            )}

            <div className="openclaw-modal-actions">
              <button
                type="button"
                className="secondary-button"
                disabled={
                  isLoading ||
                  testingServerId ===
                    "__new__"
                }
                onClick={resetForm}
              >
                Cancel
              </button>

              <button
                type="button"
                className="secondary-button"
                disabled={
                  isLoading ||
                  testingServerId ===
                    "__new__"
                }
                onClick={testForm}
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
                  testingServerId ===
                    "__new__"
                }
                onClick={submitForm}
              >
                {isLoading
                  ? "Saving..."
                  : editingId
                    ? "Save Changes"
                    : "Add Server"}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

export default OpenClawPage;