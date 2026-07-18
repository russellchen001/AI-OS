import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import {
  invokeActiveOpenClawGateway,
} from "../services/openclaw";

import {
  listMcpServers,
} from "../services/mcp";

import type {
  McpServer,
  OpenClawServer,
} from "../types/index";

type JsonObject =
  Record<string, unknown>;

type OpenClawSession = {
  key: string;
  sessionId?: string;
  displayName?: string;
  kind?: string;
  status?: string;
  updatedAt?: number;
  modelProvider?: string;
  model?: string;
  inputTokens?: number;
  outputTokens?: number;
  runtimeMs?: number;
  lastChannel?: string;
  hasActiveRun?: boolean;
};

type OpenClawMessage = {
  role?: string;
  content?:
    | string
    | Array<{
        type?: string;
        text?: string;
      }>;
  timestamp?: number;
  provider?: string;
  model?: string;
};

type OpenClawTool = {
  id: string;
  label?: string;
  description?: string;
  source?: string;
  pluginId?: string;
};

type ToolGroup = {
  id: string;
  label?: string;
  source?: string;
  tools?: OpenClawTool[];
};

type IntegrationData = {
  config?: JsonObject;
  configHash?: string;
  models: JsonObject[];
  agents: JsonObject[];
  sessions: OpenClawSession[];
  catalogGroups: ToolGroup[];
};

type Props = {
  activeServer:
    OpenClawServer | null;
  cardStyle:
    CSSProperties;
};

function errorText(
  value: unknown,
): string {
  return value instanceof Error
    ? value.message
    : String(value);
}

function objectValue(
  value: unknown,
): JsonObject {
  return value &&
    typeof value === "object" &&
    !Array.isArray(value)
    ? value as JsonObject
    : {};
}

function arrayValue<T>(
  value: unknown,
): T[] {
  return Array.isArray(value)
    ? value as T[]
    : [];
}

function messageText(
  message: OpenClawMessage,
): string {
  if (
    typeof message.content ===
    "string"
  ) {
    return message.content;
  }

  if (
    Array.isArray(
      message.content,
    )
  ) {
    return message.content
      .map(
        (part) =>
          part.type === "text"
            ? part.text ?? ""
            : "",
      )
      .filter(Boolean)
      .join("\n");
  }

  return "";
}

function formatTime(
  value?: number,
): string {
  if (!value) {
    return "—";
  }

  return new Date(
    value,
  ).toLocaleString();
}

function IntegrationCard({
  title,
  children,
  cardStyle,
}: {
  title: string;
  children:
    React.ReactNode;
  cardStyle:
    CSSProperties;
}) {
  return (
    <section
      style={{
        ...cardStyle,
        padding: 18,
        minWidth: 0,
      }}
    >
      <h3
        style={{
          margin:
            "0 0 14px",
        }}
      >
        {title}
      </h3>

      {children}
    </section>
  );
}

export default function OpenClawIntegrationPanel({
  activeServer,
  cardStyle,
}: Props) {
  const [
    data,
    setData,
  ] = useState<IntegrationData>({
    models: [],
    agents: [],
    sessions: [],
    catalogGroups: [],
  });

  const [
    selectedSessionKey,
    setSelectedSessionKey,
  ] = useState("");

  const [
    selectedSessionId,
    setSelectedSessionId,
  ] = useState("");

  const [
    messages,
    setMessages,
  ] = useState<
    OpenClawMessage[]
  >([]);

  const [
    effectiveGroups,
    setEffectiveGroups,
  ] = useState<
    ToolGroup[]
  >([]);

  const [
    mcpServers,
    setMcpServers,
  ] = useState<
    McpServer[]
  >([]);

  const [
    draft,
    setDraft,
  ] = useState("");

  const [
    loading,
    setLoading,
  ] = useState(false);

  const [
    sending,
    setSending,
  ] = useState(false);

  const [
    error,
    setError,
  ] = useState("");

  const activeServerId =
    activeServer?.id ?? "";

  const gatewayCall =
    useCallback(
      async <T,>(
        method: string,
        params:
          Record<
            string,
            unknown
          > = {},
      ): Promise<T> => {
        const response =
          await Promise.race([
            invokeActiveOpenClawGateway<T>({
              method,
              params,
            }),
            new Promise<never>(
              (_, reject) => {
                window.setTimeout(
                  () =>
                    reject(
                      new Error(
                        `${method} timed out after 12 seconds`,
                      ),
                    ),
                  12_000,
                );
              },
            ),
          ]);

        if (
          !response.success ||
          response.data ===
            undefined
        ) {
          throw new Error(
            response.message,
          );
        }

        return response.data;
      },
      [],
    );

  const refreshOverview =
    useCallback(
      async () => {
        if (!activeServerId) {
          setData({
            models: [],
            agents: [],
            sessions: [],
            catalogGroups: [],
          });
          return;
        }

        setLoading(true);
        setError("");

        try {
          const [
            configResult,
            modelsResult,
            agentsResult,
            sessionsResult,
            localMcp,
          ] =
            await Promise.all([
              gatewayCall<JsonObject>(
                "config.get",
              ),
              gatewayCall<JsonObject>(
                "models.list",
              ),
              gatewayCall<JsonObject>(
                "agents.list",
              ),
              gatewayCall<JsonObject>(
                "sessions.list",
                {
                  limit: 100,
                },
              ),
              listMcpServers(),
            ]);

          let catalogGroups:
            ToolGroup[] = [];

          try {
            const toolsResult =
              await gatewayCall<JsonObject>(
                "tools.catalog",
              );

            catalogGroups =
              arrayValue<ToolGroup>(
                toolsResult.groups,
              );
          } catch {
            /*
             * tools.catalog is optional.
             * tools.effective still provides
             * the usable tools for the session.
             */
          }

          const configObject =
            objectValue(
              configResult,
            );

          const nextSessions =
            arrayValue<OpenClawSession>(
              sessionsResult.sessions,
            );

          setData({
            config:
              objectValue(
                configObject.config ??
                  configObject.resolved,
              ),
            configHash:
              typeof configObject.hash ===
              "string"
                ? configObject.hash
                : undefined,
            models:
              arrayValue<JsonObject>(
                modelsResult.models,
              ),
            agents:
              arrayValue<JsonObject>(
                agentsResult.agents,
              ),
            sessions:
              nextSessions,
            catalogGroups,
          });

          setMcpServers(
            localMcp,
          );

          setSelectedSessionKey(
            (current) => {
              if (
                current &&
                nextSessions.some(
                  (session) =>
                    session.key ===
                    current,
                )
              ) {
                return current;
              }

              return (
                nextSessions[0]
                  ?.key ?? ""
              );
            },
          );
        } catch (
          nextError
        ) {
          setError(
            errorText(
              nextError,
            ),
          );
        } finally {
          setLoading(false);
        }
      },
      [
        activeServerId,
        gatewayCall,
      ],
    );

  const refreshSession =
    useCallback(
      async (
        sessionKey: string,
      ) => {
        if (!sessionKey) {
          setMessages([]);
          setEffectiveGroups([]);
          return;
        }

        setLoading(true);
        setError("");

        try {
          const [
            historyResult,
            effectiveResult,
          ] =
            await Promise.all([
              gatewayCall<JsonObject>(
                "chat.history",
                {
                  sessionKey,
                  limit: 100,
                  maxChars:
                    100_000,
                },
              ),
              gatewayCall<JsonObject>(
                "tools.effective",
                {
                  sessionKey,
                },
              ),
            ]);

          setMessages(
            arrayValue<OpenClawMessage>(
              historyResult.messages,
            ),
          );

          setSelectedSessionId(
            typeof historyResult.sessionId ===
            "string"
              ? historyResult.sessionId
              : "",
          );

          setEffectiveGroups(
            arrayValue<ToolGroup>(
              effectiveResult.groups,
            ),
          );
        } catch (
          nextError
        ) {
          setError(
            errorText(
              nextError,
            ),
          );
        } finally {
          setLoading(false);
        }
      },
      [
        gatewayCall,
      ],
    );

  useEffect(() => {
    void refreshOverview();
  }, [
    refreshOverview,
  ]);

  useEffect(() => {
    void refreshSession(
      selectedSessionKey,
    );
  }, [
    refreshSession,
    selectedSessionKey,
  ]);

  const workspace =
    useMemo(() => {
      const firstAgent =
        data.agents[0];

      return typeof firstAgent
        ?.workspace === "string"
        ? firstAgent.workspace
        : "—";
    }, [
      data.agents,
    ]);

  const providers =
    useMemo(() => {
      const config =
        objectValue(
          data.config,
        );

      const models =
        objectValue(
          config.models,
        );

      return Object.entries(
        objectValue(
          models.providers,
        ),
      );
    }, [
      data.config,
    ]);

  const effectiveToolCount =
    useMemo(
      () =>
        effectiveGroups.reduce(
          (
            total,
            group,
          ) =>
            total +
            (
              group.tools
                ?.length ?? 0
            ),
          0,
        ),
      [
        effectiveGroups,
      ],
    );

  async function sendMessage() {
    const message =
      draft.trim();

    if (
      !message ||
      !selectedSessionKey ||
      sending
    ) {
      return;
    }

    setSending(true);
    setError("");

    try {
      await gatewayCall(
        "chat.send",
        {
          sessionKey:
            selectedSessionKey,
          sessionId:
            selectedSessionId ||
            undefined,
          message,
          idempotencyKey:
            crypto.randomUUID(),
        },
      );

      setDraft("");

      const refreshDelays = [
        500,
        1500,
        3000,
        5000,
        8000,
        12000,
        20000,
        30000,
      ];

      refreshDelays.forEach(
        (delay) => {
          window.setTimeout(
            () => {
              void refreshSession(
                selectedSessionKey,
              );
            },
            delay,
          );
        },
      );

      window.setTimeout(
        () => {
          void refreshOverview();
        },
        1500,
      );
    } catch (
      nextError
    ) {
      setError(
        errorText(
          nextError,
        ),
      );
    } finally {
      setSending(false);
    }
  }

  return (
    <section
      style={{
        marginTop: 24,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems:
            "center",
          justifyContent:
            "space-between",
          gap: 12,
          marginBottom: 14,
        }}
      >
        <div>
          <h2
            style={{
              margin: 0,
            }}
          >
            OpenClaw Integration
          </h2>

          <p
            style={{
              margin:
                "6px 0 0",
              opacity: 0.72,
            }}
          >
            Providers, sessions,
            tools, MCP and workspace
            from the active Gateway.
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          disabled={
            loading ||
            !activeServer
          }
          onClick={() => {
            void refreshOverview();
          }}
        >
          {loading
            ? "Refreshing..."
            : "Refresh Integration"}
        </button>
      </div>

      {!activeServer && (
        <div
          style={{
            ...cardStyle,
            padding: 18,
          }}
        >
          Set an enabled OpenClaw
          Gateway as active first.
        </div>
      )}

      {error && (
        <div
          className="openclaw-error"
          style={{
            marginBottom: 14,
          }}
        >
          {error}
        </div>
      )}

      {activeServer && (
        <>
          <div
            style={{
              display: "grid",
              gridTemplateColumns:
                "repeat(auto-fit,minmax(250px,1fr))",
              gap: 14,
            }}
          >
            <IntegrationCard
              title="Provider / Models"
              cardStyle={cardStyle}
            >
              <div>
                Gateway models:{" "}
                <strong>
                  {data.models.length}
                </strong>
              </div>

              <div>
                Configured providers:{" "}
                <strong>
                  {providers.length}
                </strong>
              </div>

              <div
                style={{
                  marginTop: 12,
                  display:
                    "grid",
                  gap: 8,
                }}
              >
                {data.models.map(
                  (
                    model,
                    index,
                  ) => (
                    <div
                      key={`${String(
                        model.provider,
                      )}-${String(
                        model.id,
                      )}-${index}`}
                      style={{
                        padding: 10,
                        borderRadius: 10,
                        background:
                          "rgba(15,23,42,.22)",
                      }}
                    >
                      <strong>
                        {String(
                          model.provider ??
                            "unknown",
                        )}
                        /
                        {String(
                          model.id ??
                            model.name ??
                            "model",
                        )}
                      </strong>

                      <div
                        style={{
                          opacity:
                            0.7,
                          marginTop:
                            4,
                        }}
                      >
                        Available:{" "}
                        {String(
                          model.available ??
                            "unknown",
                        )}
                      </div>
                    </div>
                  ),
                )}
              </div>
            </IntegrationCard>

            <IntegrationCard
              title="Workspace"
              cardStyle={cardStyle}
            >
              <div
                style={{
                  wordBreak:
                    "break-all",
                }}
              >
                <strong>
                  {workspace}
                </strong>
              </div>

              <div
                style={{
                  marginTop: 10,
                  opacity: 0.72,
                }}
              >
                Agents:{" "}
                {data.agents.length}
              </div>

              <div
                style={{
                  marginTop: 6,
                  opacity: 0.72,
                }}
              >
                Config hash:{" "}
                {data.configHash
                  ?.slice(0, 12) ??
                  "—"}
              </div>
            </IntegrationCard>

            <IntegrationCard
              title="Tools / MCP"
              cardStyle={cardStyle}
            >
              <div>
                Effective tools:{" "}
                <strong>
                  {effectiveToolCount}
                </strong>
              </div>

              <div>
                Catalog groups:{" "}
                <strong>
                  {
                    data
                      .catalogGroups
                      .length
                  }
                </strong>
              </div>

              <div>
                AI OS MCP servers:{" "}
                <strong>
                  {mcpServers.length}
                </strong>
              </div>

              <div
                style={{
                  marginTop: 10,
                  display:
                    "flex",
                  flexWrap:
                    "wrap",
                  gap: 6,
                }}
              >
                {effectiveGroups
                  .flatMap(
                    (group) =>
                      group.tools ??
                      [],
                  )
                  .slice(0, 20)
                  .map(
                    (tool) => (
                      <span
                        key={
                          tool.id
                        }
                        style={{
                          padding:
                            "4px 8px",
                          borderRadius:
                            999,
                          background:
                            "rgba(59,130,246,.18)",
                          fontSize:
                            12,
                        }}
                      >
                        {tool.label ??
                          tool.id}
                      </span>
                    ),
                  )}
              </div>
            </IntegrationCard>
          </div>

          <div
            style={{
              ...cardStyle,
              padding: 18,
              marginTop: 14,
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems:
                  "center",
                justifyContent:
                  "space-between",
                gap: 12,
                flexWrap:
                  "wrap",
              }}
            >
              <h3
                style={{
                  margin: 0,
                }}
              >
                Sessions
              </h3>

              <select
                value={
                  selectedSessionKey
                }
                onChange={(
                  event,
                ) =>
                  setSelectedSessionKey(
                    event.target
                      .value,
                  )
                }
              >
                {data.sessions.map(
                  (session) => (
                    <option
                      key={
                        session.key
                      }
                      value={
                        session.key
                      }
                    >
                      {session.displayName ??
                        session.key}
                    </option>
                  ),
                )}
              </select>
            </div>

            <div
              style={{
                marginTop: 14,
                maxHeight: 440,
                overflowY:
                  "auto",
                display: "grid",
                gap: 10,
              }}
            >
              {messages
                .filter((message) => {
                  const text =
                    messageText(
                      message,
                    ).trim();

                  if (
                    message.role ===
                    "system"
                  ) {
                    return false;
                  }

                  if (
                    text ===
                      "Continue the OpenClaw runtime event." ||
                    text ===
                      "Compaction" ||
                    text
                      .toLowerCase()
                      .includes(
                        "pre-compaction memory flush",
                      )
                  ) {
                    return false;
                  }

                  return true;
                })
                .map(
                (
                  message,
                  index,
                ) => (
                  <article
                    key={`${message.timestamp ?? index}-${index}`}
                    style={{
                      padding: 12,
                      borderRadius:
                        12,
                      background:
                        message.role ===
                        "user"
                          ? "rgba(37,99,235,.18)"
                          : message.role ===
                              "assistant"
                            ? "rgba(16,185,129,.15)"
                            : "rgba(148,163,184,.12)",
                    }}
                  >
                    <div
                      style={{
                        display:
                          "flex",
                        justifyContent:
                          "space-between",
                        gap: 10,
                        marginBottom:
                          6,
                        opacity:
                          0.72,
                        fontSize:
                          12,
                      }}
                    >
                      <strong>
                        {message.role ??
                          "message"}
                      </strong>

                      <span>
                        {formatTime(
                          message.timestamp,
                        )}
                      </span>
                    </div>

                    <div
                      style={{
                        whiteSpace:
                          "pre-wrap",
                        overflowWrap:
                          "anywhere",
                      }}
                    >
                      {messageText(
                        message,
                      )}
                    </div>
                  </article>
                ),
              )}

              {messages.length ===
                0 && (
                <div
                  style={{
                    opacity: 0.65,
                  }}
                >
                  No session messages.
                </div>
              )}
            </div>

            <div
              style={{
                display: "flex",
                gap: 10,
                marginTop: 14,
              }}
            >
              <textarea
                rows={3}
                value={draft}
                placeholder="Continue this OpenClaw session..."
                style={{
                  flex: 1,
                }}
                onChange={(
                  event,
                ) =>
                  setDraft(
                    event.target
                      .value,
                  )
                }
                onKeyDown={(
                  event,
                ) => {
                  if (
                    event.key ===
                      "Enter" &&
                    !event.shiftKey
                  ) {
                    event.preventDefault();
                    void sendMessage();
                  }
                }}
              />

              <button
                type="button"
                className="action-button backup-button"
                disabled={
                  sending ||
                  !draft.trim() ||
                  !selectedSessionKey
                }
                onClick={() => {
                  void sendMessage();
                }}
              >
                {sending
                  ? "Sending..."
                  : "Send"}
              </button>
            </div>
          </div>
        </>
      )}
    </section>
  );
}
