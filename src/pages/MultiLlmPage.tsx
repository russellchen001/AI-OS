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
  save,
} from "@tauri-apps/plugin-dialog";
import {
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import {
  cancelMultiLlmStream,
  startMultiLlmStream,
} from "../services/multillm";
import MarkdownRenderer from "../components/MarkdownRenderer";

import {
  classifyHistoryCategory,
  deleteHistory,
  loadHistory,
  saveHistory,
  updateHistory,
} from "../services/history";
import type {
  ConversationRecord,
} from "../types/history";
import {
  canRouteToProvider,
  checkingRuntime,
  classifyProviderError,
  createProviderRuntime,
  healthLabel,
  readyRuntime,
  type ProviderRuntime,
} from "../services/providerRuntime";

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
    "compare" | "router" | "providers" | "history"
  >("compare");

  const [
    providers,
    setProviders,
  ] = useState<ProviderConfig[]>(
    loadProviders,
  );

  const [
    history,
    setHistory,
  ] = useState<
    ConversationRecord[]
  >(loadHistory);

  const [
    selectedHistoryId,
    setSelectedHistoryId,
  ] = useState<string | null>(
    null,
  );
  const [
    historySearch,
    setHistorySearch,
  ] = useState("");


  const [
    editingHistoryId,
    setEditingHistoryId,
  ] = useState<string | null>(
    null,
  );

  const [
    historyTitleDraft,
    setHistoryTitleDraft,
  ] = useState("");

  const [
    prompt,
    setPrompt,
  ] = useState("");

  const [
    usePersona,
    setUsePersona,
  ] = useState(false);

  const [
    routerPrompt,
    setRouterPrompt,
  ] = useState("");

  const [
    routedProviderId,
    setRoutedProviderId,
  ] = useState<ProviderId | null>(
    null,
  );

  const [
    routedCategory,
    setRoutedCategory,
  ] = useState("");

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

  const [
    providerRuntime,
    setProviderRuntime,
  ] = useState<
    Partial<
      Record<
        ProviderId,
        ProviderRuntime
      >
    >
  >(() =>
    Object.fromEntries(
      providers.map((provider) => [
        provider.id,
        createProviderRuntime(
          provider,
        ),
      ]),
    ) as Partial<
      Record<
        ProviderId,
        ProviderRuntime
      >
    >,
  );

  const operationIds =
    useRef<
      Partial<
        Record<
          ProviderId,
          string
        >
      >
    >({});

  
  const healthCheckIds =
    useRef<Set<string>>(
      new Set(),
    );

  const healthCheckOutputs =
    useRef<Map<string, string>>(
      new Map(),
    );

  type ActiveCompareHistory = {
    id: string;
    createdAt: number;
    prompt: string;
    responses: Partial<
      Record<ProviderId, string>
    >;
    pending: Set<ProviderId>;
  };

  const activeCompareHistory =
    useRef<
      ActiveCompareHistory | null
    >(null);


  type ActiveRouterHistory = {
    id: string;
    createdAt: number;
    prompt: string;
    providerId?: ProviderId;
    responses: Partial<
      Record<ProviderId, string>
    >;
    saved: boolean;
  };

  const activeRouterHistory =
    useRef<
      ActiveRouterHistory | null
    >(null);

  const saveRouterHistory =
    () => {
      const active =
        activeRouterHistory.current;

      if (!active || active.saved) {
        return;
      }

      active.saved = true;

      const record:
        ConversationRecord = {
        id: active.id,
        createdAt:
          active.createdAt,
        updatedAt: Date.now(),
        lastOpenedAt: Date.now(),
        mode: "router",
        title:
          active.prompt.length > 60
            ? `${active.prompt.slice(
                0,
                60,
              )}…`
            : active.prompt,
        prompt: active.prompt,
        routedProviderId:
          active.providerId,
        category:
          classifyHistoryCategory(
            active.prompt,
          ),
        favorite: false,
        pinned: false,
        tags: [],
        responses:
          active.responses,
      };

      activeRouterHistory.current =
        null;

      setHistory((current) => {
        const next = [
          record,
          ...current,
        ].slice(0, 100);

        saveHistory(next);
        return next;
      });

      setSelectedHistoryId(
        record.id,
      );
    };

  const saveCompletedCompareHistory =
    () => {
      const active =
        activeCompareHistory.current;

      if (
        !active ||
        active.pending.size > 0
      ) {
        return;
      }

      const record:
        ConversationRecord = {
        id: active.id,
        createdAt:
          active.createdAt,
        updatedAt: Date.now(),
        lastOpenedAt: Date.now(),
        mode: "compare",
        title:
          active.prompt.length > 60
            ? `${active.prompt.slice(
                0,
                60,
              )}…`
            : active.prompt,
        prompt: active.prompt,
        category:
          classifyHistoryCategory(
            active.prompt,
          ),
        favorite: false,
        pinned: false,
        tags: [],
        responses:
          active.responses,
      };

      activeCompareHistory.current =
        null;

      setHistory((current) => {
        const next = [
          record,
          ...current,
        ].slice(0, 100);

        saveHistory(next);
        return next;
      });

      setSelectedHistoryId(
        record.id,
      );
    };
useEffect(() => {
    setProviderRuntime((current) => {
      const next = {
        ...current,
      };

      for (const provider of providers) {
        const existing =
          current[provider.id];

        if (
          !provider.enabled ||
          (
            provider.id !== "ollama" &&
            !provider.apiKey.trim()
          )
        ) {
          next[provider.id] =
            createProviderRuntime(
              provider,
            );
          continue;
        }

        if (
          !existing ||
          existing.health ===
            "missing-key"
        ) {
          next[provider.id] =
            createProviderRuntime(
              provider,
            );
        }
      }

      return next;
    });
  }, [providers]);

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

  const filteredHistory =
    useMemo(() => {
      const query =
        historySearch
          .trim()
          .toLowerCase();

      const sortedHistory = [
        ...history,
      ].sort(
        (left, right) =>
          Number(right.favorite) -
          Number(left.favorite),
      );

      if (!query) {
        return sortedHistory;
      }

      if (query.startsWith("provider:")) {
        const provider =
          query
            .replace(
              "provider:",
              "",
            )
            .trim();

        return sortedHistory.filter(
          (record) =>
            Object.keys(
              record.responses,
            ).some(
              (id) =>
                id.toLowerCase() ===
                provider,
            ),
        );
      }

      return sortedHistory.filter(
        (record) =>
          record.title
            .toLowerCase()
            .includes(query) ||
          record.prompt
            .toLowerCase()
            .includes(query) ||
          record.tags.some(
            (tag) =>
              tag
                .toLowerCase()
                .includes(query),
          ),
      );
    }, [
      history,
      historySearch,
    ]);

  const historyStatistics =
    useMemo(() => {
      const providerUsage:
        Record<string, number> = {};

      let compareCount = 0;
      let routerCount = 0;
      let pinnedCount = 0;
      let favoriteCount = 0;

      for (const record of history) {
        if (record.mode === "compare") {
          compareCount += 1;
        } else {
          routerCount += 1;
        }

        if (record.pinned) {
          pinnedCount += 1;
        }

        if (record.favorite) {
          favoriteCount += 1;
        }

        for (
          const providerId
          of Object.keys(
            record.responses,
          )
        ) {
          providerUsage[providerId] =
            (providerUsage[providerId] ?? 0) + 1;
        }
      }

      return {
        compareCount,
        routerCount,
        pinnedCount,
        favoriteCount,
        providerUsage:
          Object.entries(
            providerUsage,
          ).sort(
            ([, left], [, right]) =>
              right - left,
          ),
      };
    }, [history]);

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

          if (
            healthCheckIds.current.has(
              payload.operationId,
            )
          ) {
            const current =
              healthCheckOutputs.current.get(
                payload.operationId,
              ) ?? "";

            healthCheckOutputs.current.set(
              payload.operationId,
              current + payload.text,
            );
            return;
          }

          const activeHistory =
            activeCompareHistory.current;

          if (
            activeHistory?.pending.has(
              payload.providerId,
            )
          ) {
            activeHistory.responses[
              payload.providerId
            ] =
              (
                activeHistory.responses[
                  payload.providerId
                ] ?? ""
              ) + payload.text;
          }

          const activeRouter =
            activeRouterHistory.current;

          if (
            activeRouter?.providerId ===
            payload.providerId
          ) {
            activeRouter.responses[
              payload.providerId
            ] =
              (
                activeRouter.responses[
                  payload.providerId
                ] ?? ""
              ) + payload.text;
          }

          setOutputs((current) => ({
            ...current,
            [payload.providerId]:
              (current[
                payload.providerId
              ] ?? "") +
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

          const wasHealthCheck =
            healthCheckIds.current.delete(
              payload.operationId,
            );

          if (wasHealthCheck) {
            const response =
              healthCheckOutputs.current
                .get(
                  payload.operationId,
                )
                ?.trim() ?? "";

            healthCheckOutputs.current.delete(
              payload.operationId,
            );

            if (payload.cancelled) {
              setProviderRuntime(
                (current) => ({
                  ...current,
                  [payload.providerId]:
                    classifyProviderError(
                      "Health check cancelled",
                    ),
                }),
              );
              return;
            }

            const normalized =
              response
                .replace(
                  /[`*_#]/g,
                  "",
                )
                .trim();

            if (
              /^ok[.!]?$/i.test(
                normalized,
              )
            ) {
              setProviderRuntime(
                (current) => ({
                  ...current,
                  [payload.providerId]:
                    readyRuntime(),
                }),
              );
            } else {
              setProviderRuntime(
                (current) => ({
                  ...current,
                  [payload.providerId]:
                    classifyProviderError(
                      response ||
                        "Empty health-check response",
                    ),
                }),
              );
            }

            return;
          }

          setStatuses((current) => ({
            ...current,
            [payload.providerId]:
              payload.cancelled
                ? "cancelled"
                : "done",
          }));

          const activeHistory =
            activeCompareHistory.current;

          if (
            activeHistory?.pending.has(
              payload.providerId,
            )
          ) {
            activeHistory.pending.delete(
              payload.providerId,
            );

            if (
              payload.cancelled &&
              !activeHistory.responses[
                payload.providerId
              ]
            ) {
              activeHistory.responses[
                payload.providerId
              ] =
                "Request cancelled.";
            }

            saveCompletedCompareHistory();
          }

          const activeRouter =
            activeRouterHistory.current;

          if (
            activeRouter?.providerId ===
              payload.providerId &&
            !payload.cancelled
          ) {
            saveRouterHistory();
          }

          if (!payload.cancelled) {
            setProviderRuntime(
              (current) => ({
                ...current,
                [payload.providerId]:
                  readyRuntime(),
              }),
            );
          }
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

          const wasHealthCheck =
            healthCheckIds.current.delete(
              payload.operationId,
            );

          healthCheckOutputs.current.delete(
            payload.operationId,
          );

          setProviderRuntime(
            (current) => ({
              ...current,
              [payload.providerId]:
                classifyProviderError(
                  payload.message,
                ),
            }),
          );

          if (wasHealthCheck) {
            return;
          }

          setStatuses((current) => ({
            ...current,
            [payload.providerId]:
              "error",
          }));

          const errorOutput =
            `Request failed: ${payload.message}`;

          const activeRouter =
            activeRouterHistory.current;

          if (
            activeRouter?.providerId ===
            payload.providerId
          ) {
            activeRouter.responses[
              payload.providerId
            ] = errorOutput;
          }

          const activeHistory =
            activeCompareHistory.current;

          if (
            activeHistory?.pending.has(
              payload.providerId,
            )
          ) {
            activeHistory.responses[
              payload.providerId
            ] = errorOutput;

            activeHistory.pending.delete(
              payload.providerId,
            );

            saveCompletedCompareHistory();
          }

          setOutputs((current) => ({
            ...current,
            [payload.providerId]:
              errorOutput,
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

  const startRenamingHistory =
    (
      id: string,
      title: string,
    ) => {
      setEditingHistoryId(id);
      setHistoryTitleDraft(title);
    };

  const saveHistoryTitle =
    (id: string) => {
      const title =
        historyTitleDraft.trim();

      if (!title) {
        return;
      }

      const next =
        updateHistory(
          id,
          (record) => ({
            ...record,
            title,
          }),
        );

      setHistory(next);
      setEditingHistoryId(null);
      setHistoryTitleDraft("");
    };

  const downloadHistoryFile =
    async (
      filename: string,
      content: string,
      mimeType: string,
    ) => {
      try {
        const extension =
          filename
            .split(".")
            .pop() ?? "txt";

        const filePath =
          await save({
            defaultPath: filename,
            filters: [
              {
                name:
                  mimeType.includes(
                    "json",
                  )
                    ? "JSON"
                    : "Markdown",
                extensions: [
                  extension,
                ],
              },
            ],
          });

        if (!filePath) {
          return;
        }

        await writeTextFile(
          filePath,
          content,
        );

        onMessage(
          `Exported to ${filePath}`,
        );
      } catch (error) {
        console.error(
          "History export failed:",
          error,
        );

        onMessage(
          `Export failed: ${String(
            error,
          )}`,
        );
      }
    };

  const createHistoryFilename =
    (
      record: ConversationRecord,
      extension: string,
    ) => {
      const safeTitle =
        record.title
          .trim()
          .replace(
            /[^a-zA-Z0-9\u4e00-\u9fff_-]+/g,
            "-",
          )
          .replace(
            /^-+|-+$/g,
            "",
          )
          .slice(0, 80) ||
        "conversation";

      return `${safeTitle}.${extension}`;
    };

  const exportHistoryMarkdown =
    async (
      record: ConversationRecord,
    ) => {
      const responses =
        Object.entries(
          record.responses,
        )
          .map(
            ([
              providerId,
              response,
            ]) =>
              [
                `## ${providerId}`,
                "",
                response ||
                  "_No response._",
              ].join("\n"),
          )
          .join(
            "\n\n---\n\n",
          );

      const markdown =
        [
          `# ${record.title}`,
          "",
          `- Mode: ${
            record.mode ===
            "compare"
              ? "Compare"
              : "Smart Router"
          }`,
          `- Created: ${new Date(
            record.createdAt,
          ).toLocaleString()}`,
          record.routedProviderId
            ? `- Routed Provider: ${record.routedProviderId}`
            : "",
          "",
          "## Question",
          "",
          record.prompt,
          "",
          "---",
          "",
          "## Responses",
          "",
          responses ||
            "_No responses._",
          "",
        ]
          .filter(
            (line) =>
              line !== "",
          )
          .join("\n");

      await downloadHistoryFile(
        createHistoryFilename(
          record,
          "md",
        ),
        markdown,
        "text/markdown;charset=utf-8",
      );
    };

  const exportHistoryJson =
    async (
      record: ConversationRecord,
    ) => {
      await downloadHistoryFile(
        createHistoryFilename(
          record,
          "json",
        ),
        JSON.stringify(
          record,
          null,
          2,
        ),
        "application/json;charset=utf-8",
      );
    };

  const toggleHistoryPinned =
    (id: string) => {
      const next =
        updateHistory(
          id,
          (record) => ({
            ...record,
            pinned:
              !record.pinned,
          }),
        );

      setHistory(next);
    };

  const toggleHistoryFavorite =
    (id: string) => {
      const next =
        updateHistory(
          id,
          (record) => ({
            ...record,
            favorite:
              !record.favorite,
          }),
        );

      setHistory(next);
    };

  const removeHistoryRecord =
    (id: string) => {
      const next =
        deleteHistory(id);

      setHistory(next);

      setSelectedHistoryId(
        (current) => {
          if (current !== id) {
            return current;
          }

          return next[0]?.id ?? null;
        },
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

      activeCompareHistory.current = {
        id: crypto.randomUUID(),
        createdAt: Date.now(),
        prompt: text,
        responses: {},
        pending: new Set(
          configuredProviders.map(
            (provider) =>
              provider.id,
          ),
        ),
      };

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

        setProviderRuntime(
          (current) => ({
            ...current,
            [provider.id]:
              checkingRuntime(),
          }),
        );

        const messages = [
          ...(usePersona &&
          provider.persona.trim()
            ? [
                {
                  role:
                    "system" as const,
                  content:
                    `Follow this response style: ${provider.persona}. When writing fenced code blocks, always include the language identifier, such as \`\`\`python or \`\`\`typescript.`,
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

            const errorOutput =
              `Request failed: ${String(
                error,
              )}`;

            const activeHistory =
              activeCompareHistory.current;

            if (
              activeHistory?.pending.has(
                provider.id,
              )
            ) {
              activeHistory.responses[
                provider.id
              ] = errorOutput;

              activeHistory.pending.delete(
                provider.id,
              );

              saveCompletedCompareHistory();
            }

            setOutputs(
              (current) => ({
                ...current,
                [provider.id]:
                  errorOutput,
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

  const classifyRoute = (
    value: string,
  ): {
    category: string;
    preferred: ProviderId[];
  } => {
    const normalized =
      value.toLowerCase();

    if (
      /code|coding|program|debug|bug|typescript|javascript|python|rust|sql|api|docker|git/.test(
        normalized,
      )
    ) {
      return {
        category:
          "Software Development",
        preferred: [
          "claude",
          "chatgpt",
          "deepseek",
          "ollama",
        ],
      };
    }

    if (
      /math|calculate|solve|equation|proof|logic|reasoning|probability|algebra|geometry|calculus|integral|derivative|matrix|polynomial|quadratic|factor|simplify/.test(
        normalized,
      ) ||
      /[0-9x-y]\s*[\+\-\*\/=]\s*[0-9x-y]/i.test(
        value,
      ) ||
      /[²³√∫∑π∞]/.test(
        value,
      )
    ) {
      return {
        category:
          "Mathematics",
        preferred: [
          "deepseek",
          "chatgpt",
          "ollama",
        ],
      };
    }

    if (
      /write|story|poem|creative|rewrite|marketing|slogan|copywriting/.test(
        normalized,
      )
    ) {
      return {
        category:
          "Creative Writing",
        preferred: [
          "chatgpt",
          "claude",
          "gemini",
          "ollama",
        ],
      };
    }

    if (
      /latest|today|news|current|recent|weather|stock|score/.test(
        normalized,
      )
    ) {
      return {
        category:
          "Current Information",
        preferred: [
          "grok",
          "gemini",
          "chatgpt",
          "ollama",
        ],
      };
    }

    if (
      value.length > 1500 ||
      /summarize|summary|analyze|document|long text|report/.test(
        normalized,
      )
    ) {
      return {
        category:
          "Long-form Analysis",
        preferred: [
          "gemini",
          "claude",
          "chatgpt",
          "ollama",
        ],
      };
    }

    return {
      category:
        "General Question",
      preferred: [
        "chatgpt",
        "ollama",
        "gemini",
        "claude",
        "deepseek",
        "grok",
      ],
    };
  };

  const sendRouted =
    async () => {
      const text =
        routerPrompt.trim();

      if (!text || isBusy) {
        return;
      }

      if (
        configuredProviders.length ===
        0
      ) {
        onMessage(
          "Unable to route request: configure at least one provider.",
        );
        setActiveTab(
          "providers",
        );
        return;
      }

      const route =
        classifyRoute(text);

      activeRouterHistory.current = {
        id: crypto.randomUUID(),
        createdAt: Date.now(),
        prompt: text,
        responses: {},
        saved: false,
      };

      const healthyProviders =
        configuredProviders.filter(
          (provider) => {
            const health =
              providerRuntime[
                provider.id
              ]?.health;

            return (
              health !== "quota" &&
              health !== "offline" &&
              health !==
                "missing-key" &&
              health !== "error"
            );
          },
        );

      const availableProviders =
        healthyProviders.length > 0
          ? healthyProviders
          : configuredProviders;

      const orderedProviders =
        route.preferred
          .map((id) =>
            availableProviders.find(
              (provider) =>
                provider.id === id,
            ),
          )
          .filter(
            (
              provider,
            ): provider is ProviderConfig =>
              Boolean(provider),
          );

      for (
        const provider
        of availableProviders
      ) {
        if (
          !orderedProviders.some(
            (item) =>
              item.id ===
              provider.id,
          )
        ) {
          orderedProviders.push(
            provider,
          );
        }
      }

      let lastError = "";

      for (
        let index = 0;
        index <
        orderedProviders.length;
        index += 1
      ) {
        const provider =
          orderedProviders[index];

        const operationId =
          crypto.randomUUID();

        operationIds.current[
          provider.id
        ] = operationId;

        if (
          activeRouterHistory.current
        ) {
          activeRouterHistory.current
            .providerId = provider.id;

          activeRouterHistory.current
            .responses[
              provider.id
            ] = "";
        }

        setRoutedProviderId(
          provider.id,
        );

        setRoutedCategory(
          route.category,
        );

        setOutputs((current) => ({
          ...current,
          [provider.id]:
            index === 0
              ? ""
              : `Previous provider was unavailable. Trying ${provider.label}…\n\n`,
        }));

        setStatuses((current) => ({
          ...current,
          [provider.id]:
            "running",
        }));

        const messages = [
          {
            role:
              "system" as const,
            content:
              "Provide the best possible answer for the detected task category. Be accurate, practical, and clear. When writing fenced code blocks, always include the language identifier, such as ```python or ```typescript.",
          },
          {
            role:
              "user" as const,
            content: text,
          },
        ];

        try {
          await startMultiLlmStream({
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
          });

          return;
        } catch (error) {
          lastError =
            String(error);

          const errorOutput =
            `Request failed: ${lastError}`;

          if (
            activeRouterHistory.current
          ) {
            activeRouterHistory.current
              .responses[
                provider.id
              ] = errorOutput;
          }

          delete operationIds.current[
            provider.id
          ];

          setStatuses(
            (current) => ({
              ...current,
              [provider.id]:
                "error",
            }),
          );

          const nextProvider =
            orderedProviders[
              index + 1
            ];

          if (nextProvider) {
            onMessage(
              `${provider.label} failed. Falling back to ${nextProvider.label}.`,
            );
          }
        }
      }

      saveRouterHistory();

      onMessage(
        `All routed providers failed. ${lastError}`,
      );
    };

  const checkProviders =
    async () => {
      if (isBusy) {
        return;
      }

      const candidates =
        providers.filter(
          (provider) =>
            provider.enabled &&
            (
              provider.id ===
                "ollama" ||
              provider.apiKey.trim()
            ),
        );

      setProviderRuntime(
        Object.fromEntries(
          providers.map(
            (provider) => [
              provider.id,
              createProviderRuntime(
                provider,
              ),
            ],
          ),
        ) as Record<
          ProviderId,
          ProviderRuntime
        >,
      );

      if (candidates.length === 0) {
        onMessage(
          "No configured providers to check.",
        );
        return;
      }

      for (const provider of candidates) {
        const operationId =
          crypto.randomUUID();

        operationIds.current[
          provider.id
        ] = operationId;

        healthCheckIds.current.add(
          operationId,
        );

        healthCheckOutputs.current.set(
          operationId,
          "",
        );

        setProviderRuntime(
          (current) => ({
            ...current,
            [provider.id]:
              checkingRuntime(),
          }),
        );

        try {
          await startMultiLlmStream({
            operationId,
            providerId:
              provider.id,
            baseUrl:
              provider.baseUrl,
            apiKey:
              provider.apiKey,
            model:
              provider.model,
            messages: [
              {
                role:
                  "user" as const,
                content:
                  "Reply with OK only.",
              },
            ],
            maxTokens: 8,
          });
        } catch (error) {
          healthCheckIds.current.delete(
            operationId,
          );

          delete operationIds.current[
            provider.id
          ];

          setProviderRuntime(
            (current) => ({
              ...current,
              [provider.id]:
                classifyProviderError(
                  error,
                ),
            }),
          );
        }
      }

      onMessage(
        "Provider health check completed.",
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
              "router"
                ? "action-button"
                : "secondary-button"
            }
            onClick={() =>
              setActiveTab(
                "router",
              )
            }
          >
            🧭 Smart Router
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
          <button
            type="button"
            className={
              activeTab ===
              "history"
                ? "action-button"
                : "secondary-button"
            }
            onClick={() =>
              setActiveTab(
                "history",
              )
            }
          >
            🕘 History
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
                      <MarkdownRenderer
                        content={
                          outputs[
                            provider.id
                          ] ?? ""
                        }
                        fallback={
                          provider.id ===
                            "ollama" ||
                          provider.apiKey.trim()
                            ? "Waiting for a prompt."
                            : "Configure this provider in the Providers tab."
                        }
                      />
                    </div>
                  </article>
                );
              },
            )}
          </div>
        </>
      )}

      {activeTab ===
        "router" && (
        <>
          <div
            className="settings-card multillm-compose"
            style={cardStyle}
          >
            <textarea
              value={
                routerPrompt
              }
              placeholder="Describe your task. Smart Router will select the most suitable available provider."
              disabled={isBusy}
              onChange={(event) =>
                setRouterPrompt(
                  event.target.value,
                )
              }
              onKeyDown={(event) => {
                if (
                  event.key ===
                    "Enter" &&
                  (event.metaKey ||
                    event.ctrlKey)
                ) {
                  event.preventDefault();
                  void sendRouted();
                }
              }}
            />

            <div className="multillm-compose-actions">
              <span className="multillm-provider-count">
                {
                  configuredProviders.length
                }{" "}
                provider(s) available
              </span>

              <button
                type="button"
                className="action-button"
                disabled={
                  isBusy ||
                  !routerPrompt.trim()
                }
                onClick={() => {
                  void sendRouted();
                }}
              >
                🧭 Route and Send
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

          {routedProviderId ? (
            <article
              className="settings-card multillm-result-card multillm-router-result"
              style={{
                ...cardStyle,
                borderTop:
                  `3px solid ${
                    providers.find(
                      (provider) =>
                        provider.id ===
                        routedProviderId,
                    )?.color ??
                    "#64748b"
                  }`,
              }}
            >
              <header className="multillm-result-header">
                <div>
                  <strong>
                    {
                      providers.find(
                        (provider) =>
                          provider.id ===
                          routedProviderId,
                      )?.icon
                    }{" "}
                    {
                      providers.find(
                        (provider) =>
                          provider.id ===
                          routedProviderId,
                      )?.label
                    }
                  </strong>

                  <small>
                    Routed as:{" "}
                    {routedCategory}
                  </small>
                </div>

                <span className="multillm-status">
                  {
                    statuses[
                      routedProviderId
                    ] ?? "idle"
                  }
                </span>
              </header>

              <div className="multillm-output">
                <MarkdownRenderer
                  content={
                    outputs[
                      routedProviderId
                    ] ?? ""
                  }
                  fallback="Waiting for the routed response."
                />
              </div>
            </article>
          ) : (
            <div
              className="settings-card multillm-router-empty"
              style={cardStyle}
            >
              <strong>
                Smart Router is ready
              </strong>
              <p>
                It currently uses local,
                deterministic routing
                rules and automatically
                falls back to any
                configured provider.
              </p>
            </div>
          )}
        </>
      )}

      {activeTab ===
        "history" && (
        <div className="multillm-history-layout">
          <aside
            className="settings-card multillm-history-list"
            style={cardStyle}
          >
            <div className="multillm-history-heading">
              <div>
                <strong>
                  Conversation History
                </strong>
                <span>
                  {historySearch.trim()
                    ? `${filteredHistory.length} result(s)`
                    : `${history.length} saved conversation(s)`}
                </span>
              </div>

              <button
                type="button"
                className="danger-button"
                disabled={
                  history.length === 0
                }
                onClick={() => {
                  setHistory([]);
                  setSelectedHistoryId(
                    null,
                  );
                  saveHistory([]);
                }}
              >
                Clear
              </button>
            </div>

            <div className="multillm-history-search">
              <input
                type="search"
                value={historySearch}
                placeholder="Search...  tag:rust  category:coding  provider:ollama"
                onChange={(event) =>
                  setHistorySearch(
                    event.target.value,
                  )
                }
              />

              {historySearch && (
                <button
                  type="button"
                  title="Clear search"
                  onClick={() =>
                    setHistorySearch("")
                  }
                >
                  ×
                </button>
              )}
            </div>

            <div className="multillm-history-statistics">
              <div className="multillm-history-stat-card">
                <span>Compare</span>
                <strong>
                  {historyStatistics.compareCount}
                </strong>
              </div>

              <div className="multillm-history-stat-card">
                <span>Router</span>
                <strong>
                  {historyStatistics.routerCount}
                </strong>
              </div>

              <div className="multillm-history-stat-card">
                <span>Pinned</span>
                <strong>
                  {historyStatistics.pinnedCount}
                </strong>
              </div>

              <div className="multillm-history-stat-card">
                <span>Favorites</span>
                <strong>
                  {historyStatistics.favoriteCount}
                </strong>
              </div>
            </div>

            {historyStatistics.providerUsage.length > 0 && (
              <div className="multillm-history-provider-stats">
                <strong className="multillm-history-provider-stats-title">
                  Provider Usage
                </strong>

                {historyStatistics.providerUsage.map(
                  ([providerId, count]) => (
                    <div
                      key={providerId}
                      className="multillm-history-provider-stat"
                    >
                      <div className="multillm-history-provider-stat-label">
                        <span>
                          {providerId}
                        </span>

                        <strong>
                          {count}
                        </strong>
                      </div>

                      <div className="multillm-history-provider-bar">
                        <span
                          style={{
                            width: `${
                              Math.max(
                                8,
                                Math.round(
                                  (
                                    count /
                                    (
                                      historyStatistics
                                        .providerUsage[0]?.[1] ??
                                      1
                                    )
                                  ) * 100,
                                ),
                              )
                            }%`,
                          }}
                        />
                      </div>
                    </div>
                  ),
                )}
              </div>
            )}

            {history.length === 0 ? (
              <p className="multillm-history-empty">
                No saved conversations yet.
              </p>
            ) : filteredHistory.length ===
              0 ? (
              <p className="multillm-history-empty">
                No matching conversations.
              </p>
            ) : (
              filteredHistory.map((record) => (
                <div
                  key={record.id}
                  className={[
                    "multillm-history-item",
                    record.pinned
                      ? "multillm-history-item-pinned"
                      : "",
                    selectedHistoryId ===
                    record.id
                      ? "multillm-history-item-active"
                      : "",
                  ].join(" ")}
                >
                  <button
                    type="button"
                    className="multillm-history-select"
                    onClick={() =>
                      setSelectedHistoryId(
                        record.id,
                      )
                    }
                  >
                    {editingHistoryId ===
                    record.id ? (
                      <input
                        className="multillm-history-title-input"
                        value={
                          historyTitleDraft
                        }
                        autoFocus
                        onClick={(event) =>
                          event.stopPropagation()
                        }
                        onChange={(event) =>
                          setHistoryTitleDraft(
                            event.target.value,
                          )
                        }
                        onKeyDown={(event) => {
                          if (
                            event.key ===
                            "Enter"
                          ) {
                            event.preventDefault();
                            saveHistoryTitle(
                              record.id,
                            );
                          }

                          if (
                            event.key ===
                            "Escape"
                          ) {
                            setEditingHistoryId(
                              null,
                            );
                          }
                        }}
                      />
                    ) : (
                      <strong>
                        {record.favorite
                          ? "⭐ "
                          : ""}
                        {record.title}
                      </strong>
                    )}

                    <span className="multillm-history-item-meta">
                      <span
                        className={[
                          "history-category-badge",
                          `history-category-${record.category}`,
                        ].join(" ")}
                      >
                        {record.category}
                      </span>

                      <span>
                        {record.mode ===
                        "compare"
                          ? "Compare"
                          : "Smart Router"}
                        {" · "}
                        {new Date(
                          record.createdAt,
                        ).toLocaleString()}
                      </span>
                    </span>
                  </button>

                  <button
                    type="button"
                    className={[
                      "multillm-history-pin",
                      record.pinned
                        ? "multillm-history-pin-active"
                        : "",
                    ].join(" ")}
                    title={
                      record.pinned
                        ? "Unpin conversation"
                        : "Pin conversation"
                    }
                    onClick={(event) => {
                      event.stopPropagation();

                      toggleHistoryPinned(
                        record.id,
                      );
                    }}
                  >
                    {record.pinned
                      ? "📌"
                      : "📍"}
                  </button>

                  <button
                    type="button"
                    className={[
                      "multillm-history-favorite",
                      record.favorite
                        ? "multillm-history-favorite-active"
                        : "",
                    ].join(" ")}
                    title={
                      record.favorite
                        ? "Remove from favorites"
                        : "Add to favorites"
                    }
                    aria-label={
                      record.favorite
                        ? `Unfavorite ${record.title}`
                        : `Favorite ${record.title}`
                    }
                    onClick={(event) => {
                      event.stopPropagation();
                      toggleHistoryFavorite(
                        record.id,
                      );
                    }}
                  >
                    {record.favorite
                      ? "★"
                      : "☆"}
                  </button>

                  <button
                    type="button"
                    className="multillm-history-rename"
                    title="Rename conversation"
                    aria-label={`Rename ${record.title}`}
                    onClick={(event) => {
                      event.stopPropagation();

                      if (
                        editingHistoryId ===
                        record.id
                      ) {
                        saveHistoryTitle(
                          record.id,
                        );
                      } else {
                        startRenamingHistory(
                          record.id,
                          record.title,
                        );
                      }
                    }}
                  >
                    {editingHistoryId ===
                    record.id
                      ? "✓"
                      : "✏️"}
                  </button>

                  <button
                    type="button"
                    className="multillm-history-delete"
                    title="Delete conversation"
                    aria-label={`Delete ${record.title}`}
                    onClick={() =>
                      removeHistoryRecord(
                        record.id,
                      )
                    }
                  >
                    🗑
                  </button>
                </div>
              ))
            )}
          </aside>

          <section
            className="settings-card multillm-history-detail"
            style={cardStyle}
          >
            {(() => {
              const record =
                history.find(
                  (item) =>
                    item.id ===
                    selectedHistoryId,
                ) ??
                history[0];

              if (!record) {
                return (
                  <div className="multillm-history-empty">
                    Select a saved conversation.
                  </div>
                );
              }

              return (
                <>
                  <header>
                    <div className="multillm-history-detail-meta">
                      <span>
                        {record.mode ===
                        "compare"
                          ? "Compare"
                          : "Smart Router"}
                      </span>

                      <time>
                        {new Date(
                          record.createdAt,
                        ).toLocaleString()}
                      </time>
                    </div>

                    <div className="multillm-history-export-actions">
                      <button
                        type="button"
                        className="secondary-button"
                        onClick={() =>
                          void exportHistoryMarkdown(
                            record,
                          )
                        }
                      >
                        Export Markdown
                      </button>

                      <button
                        type="button"
                        className="secondary-button"
                        onClick={() =>
                          void exportHistoryJson(
                            record,
                          )
                        }
                      >
                        Export JSON
                      </button>
                    </div>
                  </header>

                  <div className="multillm-history-question">
                    <strong>
                      Question
                    </strong>
                    <p>
                      {record.prompt}
                    </p>
                  </div>

                  <div className="multillm-history-responses">
                    {Object.entries(
                      record.responses,
                    ).map(
                      ([
                        providerId,
                        response,
                      ]) => (
                        <article
                          key={providerId}
                        >
                          <strong>
                            {providerId}
                          </strong>

                          <MarkdownRenderer
                            content={
                              response ?? ""
                            }
                            fallback="No response."
                          />
                        </article>
                      ),
                    )}
                  </div>
                </>
              );
            })()}
          </section>
        </div>
      )}

      {activeTab ===
        "providers" && (
        <div className="multillm-provider-list">
          <div className="multillm-provider-toolbar">
            <div>
              <strong>
                Provider Health
              </strong>
              <span>
                Check availability,
                quota, and connectivity.
              </span>
            </div>

            <button
              type="button"
              className="secondary-button"
              disabled={isBusy}
              onClick={() => {
                void checkProviders();
              }}
            >
              🩺 Check Providers
            </button>
          </div>

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

                    <span
                      className={[
                        "provider-health",
                        `provider-health-${
                          providerRuntime[
                            provider.id
                          ]?.health ??
                          "unknown"
                        }`,
                      ].join(" ")}
                      title={
                        providerRuntime[
                          provider.id
                        ]?.message
                      }
                    >
                      {
                        healthLabel(
                          providerRuntime[
                            provider.id
                          ],
                        )
                      }
                    </span>
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
