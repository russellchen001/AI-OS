import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  listen,
  type UnlistenFn,
} from "@tauri-apps/api/event";
import {
  cancelMultiLlmStream,
  startMultiLlmStream,
} from "../services/multillm";

type ProviderId =
  | "chatgpt"
  | "grok"
  | "gemini"
  | "claude"
  | "deepseek"
  | "ollama";

type ProviderConfig = {
  id: ProviderId;
  label: string;
  icon: string;
  color: string;
  enabled: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
  persona: string;
  maxTokens: number;
};

type ProviderStatus =
  | "idle"
  | "running"
  | "stopping"
  | "done"
  | "cancelled"
  | "error";

type ChunkEvent = {
  operationId: string;
  providerId: ProviderId;
  text: string;
};

type DoneEvent = {
  operationId: string;
  providerId: ProviderId;
  cancelled: boolean;
};

type ErrorEvent = {
  operationId: string;
  providerId: ProviderId;
  message: string;
};

type MultiLlmPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

const STORAGE_KEY =
  "ai-os.multillm.providers.v1";

const DEFAULT_PROVIDERS:
  ProviderConfig[] = [
  {
    id: "chatgpt",
    label: "ChatGPT",
    icon: "💬",
    color: "#10b981",
    enabled: true,
    baseUrl:
      "https://api.openai.com/v1",
    model: "gpt-5.6",
    apiKey: "",
    persona:
      "Patient, clear, and skilled at explaining complex topics.",
    maxTokens: 4096,
  },
  {
    id: "grok",
    label: "Grok",
    icon: "🚀",
    color: "#94a3b8",
    enabled: true,
    baseUrl:
      "https://api.x.ai/v1",
    model: "grok-4.5",
    apiKey: "",
    persona:
      "Direct, humorous, and willing to challenge assumptions.",
    maxTokens: 4096,
  },
  {
    id: "gemini",
    label: "Gemini",
    icon: "✨",
    color: "#3b82f6",
    enabled: true,
    baseUrl:
      "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-3.5-flash",
    apiKey: "",
    persona:
      "Analytical, objective, evidence-driven, and balanced.",
    maxTokens: 4096,
  },
  {
    id: "claude",
    label: "Claude",
    icon: "🟠",
    color: "#f97316",
    enabled: true,
    baseUrl:
      "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-6",
    apiKey: "",
    persona:
      "Thoughtful, careful, and attentive to risks and long-term impact.",
    maxTokens: 4096,
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    icon: "🐋",
    color: "#6366f1",
    enabled: true,
    baseUrl:
      "https://api.deepseek.com/v1",
    model: "deepseek-v4-flash",
    apiKey: "",
    persona:
      "Technical, direct, efficient, and focused on implementation details.",
    maxTokens: 4096,
  },
  {
    id: "ollama",
    label: "Ollama",
    icon: "🦙",
    color: "#22c55e",
    enabled: true,
    baseUrl:
      "http://localhost:11434",
    model: "qwen3:8b",
    apiKey: "",
    persona:
      "Runs locally, prioritizes privacy, and responds directly and efficiently.",
    maxTokens: 4096,
  },
];

function loadProviders():
  ProviderConfig[] {
  try {
    const raw =
      localStorage.getItem(
        STORAGE_KEY,
      );

    if (!raw) {
      return DEFAULT_PROVIDERS;
    }

    const saved =
      JSON.parse(raw) as
        Partial<ProviderConfig>[];

    const personaMigrations:
      Record<string, string> = {
      "耐心、清晰、善于解释复杂问题。":
        "Patient, clear, and skilled at explaining complex topics.",
      "直接、幽默、善于提出不同观点。":
        "Direct, humorous, and willing to challenge assumptions.",
      "理性、客观、重视证据与多角度分析。":
        "Analytical, objective, evidence-driven, and balanced.",
      "深思熟虑、关注风险和长期影响。":
        "Thoughtful, careful, and attentive to risks and long-term impact.",
      "技术导向、直接、高效、重视实现细节。":
        "Technical, direct, efficient, and focused on implementation details.",
      "本地运行、注重隐私、直接高效。":
        "Runs locally, prioritizes privacy, and responds efficiently.",
    };

    for (const provider of saved) {
      if (
        typeof provider.persona === "string" &&
        personaMigrations[provider.persona]
      ) {
        provider.persona =
          personaMigrations[provider.persona];
      }
    }

    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(saved),
    );

    return DEFAULT_PROVIDERS.map(
      (fallback) => ({
        ...fallback,
        ...(saved.find(
          (item) =>
            item.id === fallback.id,
        ) ?? {}),
      }),
    );
  } catch {
    return DEFAULT_PROVIDERS;
  }
}

function MultiLlmPage({
  cardStyle,
  onMessage,
}: MultiLlmPageProps) {
  const [
    activeTab,
    setActiveTab,
  ] = useState<
    "compare" | "providers"
  >("compare");

  const [
    providers,
    setProviders,
  ] = useState<ProviderConfig[]>(
    loadProviders,
  );

  const [
    prompt,
    setPrompt,
  ] = useState("");

  const [
    usePersona,
    setUsePersona,
  ] = useState(false);

  const [
    outputs,
    setOutputs,
  ] = useState<
    Record<string, string>
  >({});

  const [
    statuses,
    setStatuses,
  ] = useState<
    Record<string, ProviderStatus>
  >({});

  const operationIds =
    useRef<
      Partial<
        Record<
          ProviderId,
          string
        >
      >
    >({});

  const configuredProviders =
    useMemo(
      () =>
        providers.filter(
          (provider) =>
            provider.enabled &&
            (
              provider.id === "ollama" ||
              provider.apiKey.trim()
            ),
        ),
      [providers],
    );

  const isBusy =
    Object.values(
      statuses,
    ).some(
      (status) =>
        status === "running" ||
        status === "stopping",
    );

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const register = async <T,>(
      eventName: string,
      handler: (payload: T) => void,
    ) => {
      const unlisten = await listen<T>(
        eventName,
        (event) => {
          if (!disposed) {
            handler(event.payload);
          }
        },
      );

      if (disposed) {
        unlisten();
        return;
      }

      unlisteners.push(unlisten);
    };

    const install = async () => {
      await register<ChunkEvent>(
        "multillm://chunk",
        (payload) => {
          if (
            operationIds.current[
              payload.providerId
            ] !== payload.operationId
          ) {
            return;
          }

          setOutputs((current) => ({
            ...current,
            [payload.providerId]:
              (current[payload.providerId] ?? "") +
              payload.text,
          }));
        },
      );

      await register<DoneEvent>(
        "multillm://done",
        (payload) => {
          if (
            operationIds.current[
              payload.providerId
            ] !== payload.operationId
          ) {
            return;
          }

          delete operationIds.current[
            payload.providerId
          ];

          setStatuses((current) => ({
            ...current,
            [payload.providerId]:
              payload.cancelled
                ? "cancelled"
                : "done",
          }));
        },
      );

      await register<ErrorEvent>(
        "multillm://error",
        (payload) => {
          if (
            operationIds.current[
              payload.providerId
            ] !== payload.operationId
          ) {
            return;
          }

          delete operationIds.current[
            payload.providerId
          ];

          setStatuses((current) => ({
            ...current,
            [payload.providerId]: "error",
          }));

          setOutputs((current) => ({
            ...current,
            [payload.providerId]:
              `Request failed: ${payload.message}`,
          }));
        },
      );
    };

    void install();

    return () => {
      disposed = true;

      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const updateProvider = <
    K extends keyof ProviderConfig,
  >(
    id: ProviderId,
    key: K,
    value: ProviderConfig[K],
  ) => {
    setProviders(
      (current) =>
        current.map(
          (provider) =>
            provider.id === id
              ? {
                  ...provider,
                  [key]: value,
                }
              : provider,
        ),
    );
  };

  const saveProviders =
    () => {
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify(
          providers,
        ),
      );

      onMessage(
        "MultiLLM provider settings saved.",
      );
  };

  const sendToAll =
    async () => {
      const text =
        prompt.trim();

      if (!text || isBusy) {
        return;
      }

      if (
        configuredProviders
          .length === 0
      ) {
        onMessage(
          "Unable to start MultiLLM: configure at least one API key.",
        );
        setActiveTab(
          "providers",
        );
        return;
      }

      const nextOutputs:
        Record<string, string> = {};

      const nextStatuses:
        Record<
          string,
          ProviderStatus
        > = {};

      for (
        const provider
        of configuredProviders
      ) {
        nextOutputs[
          provider.id
        ] = "";

        nextStatuses[
          provider.id
        ] = "running";

        const operationId =
          crypto.randomUUID();

        operationIds.current[
          provider.id
        ] = operationId;

        const messages = [
          ...(usePersona &&
          provider.persona.trim()
            ? [
                {
                  role:
                    "system" as const,
                  content:
                    `Follow this response style: ${provider.persona}`,
                },
              ]
            : []),
          {
            role:
              "user" as const,
            content: text,
          },
        ];

        void startMultiLlmStream({
          operationId,
          providerId:
            provider.id,
          baseUrl:
            provider.baseUrl,
          apiKey:
            provider.apiKey,
          model:
            provider.model,
          messages,
          maxTokens:
            provider.maxTokens,
        }).catch(
          (error) => {
            if (
              operationIds.current[
                provider.id
              ] !== operationId
            ) {
              return;
            }

            delete operationIds
              .current[
                provider.id
              ];

            setStatuses(
              (current) => ({
                ...current,
                [provider.id]:
                  "error",
              }),
            );

            setOutputs(
              (current) => ({
                ...current,
                [provider.id]:
                  `Request failed: ${String(
                    error,
                  )}`,
              }),
            );
          },
        );
      }

      setOutputs(
        nextOutputs,
      );

      setStatuses(
        nextStatuses,
      );
    };

  const stopAll =
    async () => {
      const activeEntries =
        Object.entries(
          operationIds.current,
        ) as Array<
          [
            ProviderId,
            string,
          ]
        >;

      setStatuses(
        (current) => {
          const next = {
            ...current,
          };

          for (
            const [providerId]
            of activeEntries
          ) {
            next[
              providerId
            ] = "stopping";
          }

          return next;
        },
      );

      await Promise.allSettled(
        activeEntries.map(
          ([
            ,
            operationId,
          ]) =>
            cancelMultiLlmStream(
              operationId,
            ),
        ),
      );
  };

  return (
    <section className="page-section multillm-page">
      <div className="section-header">
        <div>
          <h2>
            MultiLLM Hub
          </h2>
          <p>
            Compare multiple AI
            providers with concurrent
            streaming responses.
          </p>
        </div>

        <div className="multillm-tabs">
          <button
            type="button"
            className={
              activeTab ===
              "compare"
                ? "action-button"
                : "secondary-button"
            }
            onClick={() =>
              setActiveTab(
                "compare",
              )
            }
          >
            ⚡ Compare
          </button>

          <button
            type="button"
            className={
              activeTab ===
              "providers"
                ? "action-button"
                : "secondary-button"
            }
            onClick={() =>
              setActiveTab(
                "providers",
              )
            }
          >
            🔑 Providers
          </button>
        </div>
      </div>

      {activeTab ===
        "compare" && (
        <>
          <div
            className="settings-card multillm-compose"
            style={cardStyle}
          >
            <textarea
              value={prompt}
              placeholder="Ask all configured models the same question…"
              disabled={isBusy}
              onChange={(event) =>
                setPrompt(
                  event.target.value,
                )
              }
              onKeyDown={(
                event,
              ) => {
                if (
                  event.key ===
                    "Enter" &&
                  (event.metaKey ||
                    event.ctrlKey)
                ) {
                  event.preventDefault();
                  void sendToAll();
                }
              }}
            />

            <div className="multillm-compose-actions">
              <label className="multillm-persona-toggle">
                <input
                  type="checkbox"
                  checked={
                    usePersona
                  }
                  disabled={isBusy}
                  onChange={(
                    event,
                  ) =>
                    setUsePersona(
                      event.target
                        .checked,
                    )
                  }
                />
                🎭 Use personas
              </label>

              <span className="multillm-provider-count">
                {
                  configuredProviders
                    .length
                }{" "}
                provider(s) ready
              </span>

              <button
                type="button"
                className="action-button"
                disabled={
                  isBusy ||
                  !prompt.trim()
                }
                onClick={() => {
                  void sendToAll();
                }}
              >
                ⚡ Send to All
              </button>

              <button
                type="button"
                className="danger-button"
                disabled={!isBusy}
                onClick={() => {
                  void stopAll();
                }}
              >
                ⏹ Stop
              </button>
            </div>
          </div>

          <div className="multillm-grid">
            {providers.map(
              (provider) => {
                const status =
                  statuses[
                    provider.id
                  ] ?? "idle";

                return (
                  <article
                    key={
                      provider.id
                    }
                    className="settings-card multillm-result-card"
                    style={{
                      ...cardStyle,
                      borderTop:
                        `3px solid ${provider.color}`,
                    }}
                  >
                    <header className="multillm-result-header">
                      <div>
                        <strong
                          style={{
                            color:
                              provider.color,
                          }}
                        >
                          {
                            provider.icon
                          }{" "}
                          {
                            provider.label
                          }
                        </strong>

                        <small>
                          {
                            provider.model
                          }
                        </small>
                      </div>

                      <span
                        className={[
                          "multillm-status",
                          `multillm-status-${status}`,
                        ].join(
                          " ",
                        )}
                      >
                        {status ===
                        "running"
                          ? "Thinking…"
                          : status ===
                              "stopping"
                            ? "Stopping…"
                            : status ===
                                "done"
                              ? "Done"
                              : status ===
                                  "cancelled"
                                ? "Stopped"
                                : status ===
                                    "error"
                                  ? "Error"
                                  : provider
                                        .apiKey
                                        .trim()
                                    ? "Ready"
                                    : "No API key"}
                      </span>
                    </header>

                    <div className="multillm-output">
                      {outputs[
                        provider.id
                      ] ||
                        (provider.id ===
                          "ollama" ||
                        provider
                          .apiKey
                          .trim()
                          ? "Waiting for a prompt."
                          : "Configure this provider in the Providers tab.")}
                    </div>
                  </article>
                );
              },
            )}
          </div>
        </>
      )}

      {activeTab ===
        "providers" && (
        <div className="multillm-provider-list">
          {providers.map(
            (provider) => (
              <article
                key={provider.id}
                className="settings-card multillm-provider-card"
                style={{
                  ...cardStyle,
                  borderTop:
                    `3px solid ${provider.color}`,
                }}
              >
                <div className="multillm-provider-title">
                  <h3>
                    {
                      provider.icon
                    }{" "}
                    {
                      provider.label
                    }
                  </h3>

                  <label>
                    <input
                      type="checkbox"
                      checked={
                        provider.enabled
                      }
                      onChange={(
                        event,
                      ) =>
                        updateProvider(
                          provider.id,
                          "enabled",
                          event.target
                            .checked,
                        )
                      }
                    />
                    Enabled
                  </label>
                </div>

                <label className="setting-field">
                  <span>
                    API Key
                  </span>
                  <input
                    type="password"
                    value={
                      provider.apiKey
                    }
                    placeholder="Enter API key"
                    onChange={(
                      event,
                    ) =>
                      updateProvider(
                        provider.id,
                        "apiKey",
                        event.target
                          .value,
                      )
                    }
                  />
                </label>

                <label className="setting-field">
                  <span>
                    Model
                  </span>
                  <input
                    type="text"
                    value={
                      provider.model
                    }
                    onChange={(
                      event,
                    ) =>
                      updateProvider(
                        provider.id,
                        "model",
                        event.target
                          .value,
                      )
                    }
                  />
                </label>

                <label className="setting-field">
                  <span>
                    Base URL
                  </span>
                  <input
                    type="text"
                    value={
                      provider.baseUrl
                    }
                    onChange={(
                      event,
                    ) =>
                      updateProvider(
                        provider.id,
                        "baseUrl",
                        event.target
                          .value,
                      )
                    }
                  />
                </label>

                <label className="setting-field">
                  <span>
                    Persona
                  </span>
                  <textarea
                    value={
                      provider.persona
                    }
                    onChange={(
                      event,
                    ) =>
                      updateProvider(
                        provider.id,
                        "persona",
                        event.target
                          .value,
                      )
                    }
                  />
                </label>
              </article>
            ),
          )}

          <div className="multillm-save-row">
            <button
              type="button"
              className="action-button"
              onClick={
                saveProviders
              }
            >
              💾 Save Providers
            </button>

            <small>
              Keys are currently saved
              in local application
              storage. System Keychain
              migration will follow.
            </small>
          </div>
        </div>
      )}
    </section>
  );
}

export default MultiLlmPage;
