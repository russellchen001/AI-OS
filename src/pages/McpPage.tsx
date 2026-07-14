import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import ConfirmDialog from "../components/ConfirmDialog";
import type {
  AsyncStatus,
  McpServer,
  McpServerInput,
  McpTransport,
} from "../types/index";

type McpPageProps = {
  servers: McpServer[];
  enabledCount: number;
  status: AsyncStatus;
  activeServerId: string | null;
  searchText: string;
  error: string;
  cardStyle: CSSProperties;

  onSearchChange: (
    value: string,
  ) => void;

  onRefresh: () => void;

  onCreate: (
    server: McpServerInput,
  ) => Promise<McpServer | null>;

  onUpdate: (
    id: string,
    server: McpServerInput,
  ) => Promise<McpServer | null>;

  onToggle: (
    id: string,
    enabled: boolean,
  ) => void;

  onDelete: (
    id: string,
  ) => void;
};

const EMPTY_FORM: McpServerInput = {
  name: "",
  description: "",
  enabled: true,
  transport: "stdio",
  command: "",
  args: [],
  url: "",
  environment: {},
};

function argsToText(
  args: string[],
): string {
  return args.join("\n");
}

function textToArgs(
  value: string,
): string[] {
  return value
    .split("\n")
    .map(
      (item) =>
        item.trim(),
    )
    .filter(Boolean);
}

function environmentToText(
  environment:
    Record<string, string>,
): string {
  return Object.entries(
    environment,
  )
    .map(
      ([key, value]) =>
        `${key}=${value}`,
    )
    .join("\n");
}

function textToEnvironment(
  value: string,
): Record<string, string> {
  const result:
    Record<string, string> = {};

  for (
    const line of
      value.split("\n")
  ) {
    const trimmed =
      line.trim();

    if (!trimmed) {
      continue;
    }

    const separator =
      trimmed.indexOf("=");

    if (separator <= 0) {
      continue;
    }

    const key =
      trimmed
        .slice(
          0,
          separator,
        )
        .trim();

    const environmentValue =
      trimmed
        .slice(
          separator + 1,
        )
        .trim();

    if (key) {
      result[key] =
        environmentValue;
    }
  }

  return result;
}

function McpPage({
  servers,
  enabledCount,
  status,
  activeServerId,
  searchText,
  error,
  cardStyle,
  onSearchChange,
  onRefresh,
  onCreate,
  onUpdate,
  onToggle,
  onDelete,
}: McpPageProps) {
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
  ] = useState<McpServerInput>(
    EMPTY_FORM,
  );

  const [
    argsText,
    setArgsText,
  ] = useState("");

  const [
    environmentText,
    setEnvironmentText,
  ] = useState("");

  const [
    formError,
    setFormError,
  ] = useState("");

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

  const transportCounts =
    useMemo(() => {
      return servers.reduce(
        (
          counts,
          server,
        ) => {
          counts[
            server.transport
          ] += 1;

          return counts;
        },
        {
          stdio: 0,
          http: 0,
          sse: 0,
        } as Record<
          McpTransport,
          number
        >,
      );
    }, [servers]);

  function resetForm() {
    setForm(EMPTY_FORM);
    setArgsText("");
    setEnvironmentText("");
    setEditingId(null);
    setFormError("");
    setFormOpen(false);
  }

  function openCreateForm() {
    setForm(EMPTY_FORM);
    setArgsText("");
    setEnvironmentText("");
    setEditingId(null);
    setFormError("");
    setFormOpen(true);
  }

  function openEditForm(
    server: McpServer,
  ) {
    setEditingId(
      server.id,
    );

    setForm({
      name: server.name,
      description:
        server.description,
      enabled:
        server.enabled,
      transport:
        server.transport,
      command:
        server.command ?? "",
      args: server.args,
      url: server.url ?? "",
      environment:
        server.environment,
    });

    setArgsText(
      argsToText(
        server.args,
      ),
    );

    setEnvironmentText(
      environmentToText(
        server.environment,
      ),
    );

    setFormError("");
    setFormOpen(true);
  }

  async function submitForm() {
    const name =
      form.name.trim();

    if (!name) {
      setFormError(
        "Server name is required.",
      );
      return;
    }

    if (
      form.transport ===
        "stdio" &&
      !form.command?.trim()
    ) {
      setFormError(
        "A command is required for stdio servers.",
      );
      return;
    }

    if (
      form.transport !==
        "stdio" &&
      !form.url?.trim()
    ) {
      setFormError(
        "A URL is required for HTTP and SSE servers.",
      );
      return;
    }

    const payload:
      McpServerInput = {
      ...form,
      name,
      description:
        form.description.trim(),
      command:
        form.transport ===
        "stdio"
          ? form.command?.trim()
          : undefined,
      url:
        form.transport ===
        "stdio"
          ? undefined
          : form.url?.trim(),
      args:
        form.transport ===
        "stdio"
          ? textToArgs(
              argsText,
            )
          : [],
      environment:
        textToEnvironment(
          environmentText,
        ),
    };

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
            MCP Servers
          </h2>

          <p>
            Manage Model Context
            Protocol integrations for
            local AI tools.
          </p>
        </div>

        <div className="mcp-header-actions">
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

      <div className="mcp-summary-grid">
        <div
          className="mcp-summary-card"
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
          className="mcp-summary-card mcp-summary-enabled"
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
          className="mcp-summary-card"
          style={cardStyle}
        >
          <span>
            Disabled
          </span>

          <strong>
            {disabledCount}
          </strong>
        </div>

        <div
          className="mcp-summary-card"
          style={cardStyle}
        >
          <span>
            Transports
          </span>

          <strong>
            {transportCounts.stdio} /
            {transportCounts.http} /
            {transportCounts.sse}
          </strong>

          <small>
            stdio / http / sse
          </small>
        </div>
      </div>

      <div className="mcp-toolbar">
        <input
          type="search"
          className="mcp-search"
          value={searchText}
          placeholder="Search MCP servers..."
          onChange={(
            event,
          ) =>
            onSearchChange(
              event.target.value,
            )
          }
        />
      </div>

      {error && (
        <div
          className="mcp-error"
          role="alert"
        >
          {error}
        </div>
      )}

      {servers.length === 0 ? (
        <div
          className="mcp-empty-state"
          style={cardStyle}
        >
          <span>
            🔌
          </span>

          <h3>
            No MCP servers
          </h3>

          <p>
            Add a server to connect AI
            OS with local tools and
            external services.
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
        <div className="mcp-grid">
          {servers.map(
            (server) => {
              const busy =
                activeServerId ===
                server.id;

              return (
                <article
                  key={server.id}
                  className={[
                    "mcp-card",
                    server.enabled
                      ? "mcp-card-enabled"
                      : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  style={cardStyle}
                >
                  <div className="mcp-card-header">
                    <div className="mcp-card-title">
                      <span className="mcp-card-icon">
                        🔌
                      </span>

                      <div>
                        <h3>
                          {server.name}
                        </h3>

                        <p>
                          {
                            server.description
                          }
                        </p>
                      </div>
                    </div>

                    <label className="mcp-switch">
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

                  <div className="mcp-meta-grid">
                    <div>
                      <span>
                        Transport
                      </span>

                      <strong>
                        {
                          server.transport
                        }
                      </strong>
                    </div>

                    <div>
                      <span>
                        Endpoint
                      </span>

                      <strong>
                        {server.transport ===
                        "stdio"
                          ? server.command
                          : server.url}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Arguments
                      </span>

                      <strong>
                        {
                          server.args
                            .length
                        }
                      </strong>
                    </div>

                    <div>
                      <span>
                        Environment
                      </span>

                      <strong>
                        {
                          Object.keys(
                            server.environment,
                          ).length
                        }
                      </strong>
                    </div>
                  </div>

                  {server.transport ===
                    "stdio" &&
                    server.args
                      .length > 0 && (
                      <div className="mcp-command-preview">
                        <code
                          title={[
                            server.command,
                            ...server.args,
                          ]
                            .filter(Boolean)
                            .join(" ")}
                        >
                          {server.command}{" "}
                          {server.args.join(
                            " ",
                          )}
                        </code>
                      </div>
                    )}

                  <div className="mcp-card-actions">
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
        <div
          className="mcp-modal-backdrop"
          role="presentation"
          onMouseDown={(
            event,
          ) => {
            if (
              event.target ===
              event.currentTarget &&
              !isLoading
            ) {
              resetForm();
            }
          }}
        >
          <div
            className="mcp-modal"
            style={cardStyle}
            role="dialog"
            aria-modal="true"
            aria-label={
              editingId
                ? "Edit MCP server"
                : "Add MCP server"
            }
          >
            <div className="mcp-modal-header">
              <div>
                <h3>
                  {editingId
                    ? "Edit MCP Server"
                    : "Add MCP Server"}
                </h3>

                <p>
                  Configure a stdio,
                  HTTP or SSE MCP
                  integration.
                </p>
              </div>

              <button
                type="button"
                className="secondary-button"
                disabled={isLoading}
                onClick={resetForm}
              >
                Close
              </button>
            </div>

            <div className="mcp-form-grid">
              <label className="setting-field">
                <span>
                  Name
                </span>

                <input
                  type="text"
                  value={form.name}
                  disabled={isLoading}
                  placeholder="Filesystem"
                  onChange={(
                    event,
                  ) =>
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
                  Transport
                </span>

                <select
                  value={
                    form.transport
                  }
                  disabled={isLoading}
                  onChange={(
                    event,
                  ) =>
                    setForm(
                      (current) => ({
                        ...current,
                        transport:
                          event.target
                            .value as McpTransport,
                      }),
                    )
                  }
                >
                  <option value="stdio">
                    stdio
                  </option>

                  <option value="http">
                    HTTP
                  </option>

                  <option value="sse">
                    SSE
                  </option>
                </select>
              </label>
            </div>

            <label className="setting-field">
              <span>
                Description
              </span>

              <textarea
                rows={3}
                value={
                  form.description
                }
                disabled={isLoading}
                placeholder="Describe what this MCP server provides."
                onChange={(
                  event,
                ) =>
                  setForm(
                    (current) => ({
                      ...current,
                      description:
                        event.target
                          .value,
                    }),
                  )
                }
              />
            </label>

            {form.transport ===
            "stdio" ? (
              <>
                <label className="setting-field">
                  <span>
                    Command
                  </span>

                  <input
                    type="text"
                    value={
                      form.command ??
                      ""
                    }
                    disabled={
                      isLoading
                    }
                    placeholder="npx"
                    onChange={(
                      event,
                    ) =>
                      setForm(
                        (
                          current,
                        ) => ({
                          ...current,
                          command:
                            event.target
                              .value,
                        }),
                      )
                    }
                  />
                </label>

                <label className="setting-field">
                  <span>
                    Arguments
                  </span>

                  <small>
                    Enter one argument
                    per line.
                  </small>

                  <textarea
                    rows={5}
                    value={argsText}
                    disabled={
                      isLoading
                    }
                    placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/Users/name/Documents"}
                    onChange={(
                      event,
                    ) =>
                      setArgsText(
                        event.target
                          .value,
                      )
                    }
                  />
                </label>
              </>
            ) : (
              <label className="setting-field">
                <span>
                  Server URL
                </span>

                <input
                  type="url"
                  value={
                    form.url ?? ""
                  }
                  disabled={isLoading}
                  placeholder="http://localhost:3001/mcp"
                  onChange={(
                    event,
                  ) =>
                    setForm(
                      (current) => ({
                        ...current,
                        url:
                          event.target
                            .value,
                      }),
                    )
                  }
                />
              </label>
            )}

            <label className="setting-field">
              <span>
                Environment Variables
              </span>

              <small>
                Enter one KEY=value
                pair per line. Secrets
                are stored locally.
              </small>

              <textarea
                rows={5}
                value={
                  environmentText
                }
                disabled={isLoading}
                placeholder={"API_KEY=\nGITHUB_TOKEN="}
                onChange={(
                  event,
                ) =>
                  setEnvironmentText(
                    event.target
                      .value,
                  )
                }
              />
            </label>

            <label className="mcp-enabled-option">
              <input
                type="checkbox"
                checked={
                  form.enabled
                }
                disabled={isLoading}
                onChange={(
                  event,
                ) =>
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

            {formError && (
              <div
                className="mcp-error"
                role="alert"
              >
                {formError}
              </div>
            )}

            <div className="mcp-modal-actions">
              <button
                type="button"
                className="secondary-button"
                disabled={isLoading}
                onClick={resetForm}
              >
                Cancel
              </button>

              <button
                type="button"
                className="action-button backup-button"
                disabled={isLoading}
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
      <ConfirmDialog
        open={confirmDelete !== null}
        title="Delete MCP server?"
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
          activeServerId ===
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

export default McpPage;