import {
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
import MarkdownRenderer from "../components/MarkdownRenderer";
import {
  useDialog,
} from "../components/DialogProvider";
import {
  cancelMultiLlmStream,
  startMultiLlmStream,
  type MultiLlmMessage,
} from "../services/multillm";
import {
  deleteCouncilSession,
  loadCouncilMembers,
  loadCouncilSessions,
  resetCouncilMembers,
  saveCouncilMembers,
  upsertCouncilSession,
} from "../services/council";
import type {
  CouncilMember,
  CouncilProviderId,
  CouncilRole,
  CouncilSession,
  CouncilStepResult,
} from "../types/council";

type AiCouncilPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

type ProviderConfig = {
  id: CouncilProviderId;
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

type ChunkEvent = {
  operationId: string;
  providerId: CouncilProviderId;
  text: string;
};

type DoneEvent = {
  operationId: string;
  providerId: CouncilProviderId;
  cancelled: boolean;
};

type ErrorEvent = {
  operationId: string;
  providerId: CouncilProviderId;
  message: string;
};

const PROVIDER_STORAGE_KEY =
  "ai-os.multillm.providers.v1";

const ROLE_ORDER:
  CouncilRole[] = [
  "planner",
  "engineer",
  "researcher",
  "critic",
  "judge",
];

function loadProviders():
  ProviderConfig[] {
  try {
    const raw =
      localStorage.getItem(
        PROVIDER_STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    return Array.isArray(parsed)
      ? (
          parsed as
            ProviderConfig[]
        )
      : [];
  } catch {
    return [];
  }
}

function createSessionTitle(
  prompt: string,
): string {
  const value =
    prompt.trim() ||
    "Untitled Council Session";

  return value.length > 60
    ? `${value.slice(0, 60)}…`
    : value;
}

function safeFilename(
  title: string,
): string {
  return (
    title
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
    "ai-council"
  );
}

function AiCouncilPage({
  cardStyle,
  onMessage,
}: AiCouncilPageProps) {
  const dialog =
    useDialog();

  const [
    providers,
  ] = useState<
    ProviderConfig[]
  >(loadProviders);

  const [
    members,
    setMembers,
  ] = useState<
    CouncilMember[]
  >(loadCouncilMembers);

  const [
    sessions,
    setSessions,
  ] = useState<
    CouncilSession[]
  >(loadCouncilSessions);

  const [
    selectedSessionId,
    setSelectedSessionId,
  ] = useState<
    string | null
  >(null);

  const [
    prompt,
    setPrompt,
  ] = useState("");

  const [
    steps,
    setSteps,
  ] = useState<
    CouncilStepResult[]
  >([]);

  const [
    finalAnswer,
    setFinalAnswer,
  ] = useState("");

  const [
    isRunning,
    setIsRunning,
  ] = useState(false);

  const [
    editingMember,
    setEditingMember,
  ] = useState<
    CouncilRole | null
  >(null);

  const [
    sessionSearch,
    setSessionSearch,
  ] = useState("");

  const cancelledRef =
    useRef(false);

  const currentOperationRef =
    useRef<string | null>(
      null,
    );

  const configuredProviders =
    useMemo(
      () =>
        providers.filter(
          (provider) =>
            provider.enabled &&
            (
              provider.id ===
                "ollama" ||
              Boolean(
                provider.apiKey
                  .trim(),
              )
            ),
        ),
      [providers],
    );

  const filteredSessions =
    useMemo(() => {
      const query =
        sessionSearch
          .trim()
          .toLowerCase();

      return sessions.filter(
        (session) =>
          !query ||
          session.title
            .toLowerCase()
            .includes(query) ||
          session.prompt
            .toLowerCase()
            .includes(query) ||
          session.finalAnswer
            .toLowerCase()
            .includes(query),
      );
    }, [
      sessionSearch,
      sessions,
    ]);

  const updateMember =
    <K extends keyof CouncilMember>(
      id: CouncilRole,
      key: K,
      value:
        CouncilMember[K],
    ) => {
      setMembers(
        (current) =>
          current.map(
            (member) =>
              member.id === id
                ? {
                    ...member,
                    [key]: value,
                  }
                : member,
          ),
      );
    };

  const persistMembers =
    () => {
      saveCouncilMembers(
        members,
      );

      onMessage(
        "AI Council configuration saved.",
      );
    };

  const runProviderStream =
    async (
      provider:
        ProviderConfig,
      messages:
        MultiLlmMessage[],
      onChunk: (
        text: string,
      ) => void,
    ): Promise<string> => {
      const operationId =
        crypto.randomUUID();

      currentOperationRef.current =
        operationId;

      let output = "";
      let unlistenChunk:
        UnlistenFn | undefined;
      let unlistenDone:
        UnlistenFn | undefined;
      let unlistenError:
        UnlistenFn | undefined;

      return await new Promise<
        string
      >(
        (
          resolve,
          reject,
        ) => {
          let settled = false;

          const cleanup =
            () => {
              unlistenChunk?.();
              unlistenDone?.();
              unlistenError?.();

              if (
                currentOperationRef
                  .current ===
                operationId
              ) {
                currentOperationRef.current =
                  null;
              }
            };

          const fail =
            (
              error: unknown,
            ) => {
              if (settled) {
                return;
              }

              settled = true;
              cleanup();

              reject(
                error instanceof Error
                  ? error
                  : new Error(
                      String(error),
                    ),
              );
            };

          const succeed =
            () => {
              if (settled) {
                return;
              }

              settled = true;
              cleanup();
              resolve(output);
            };

          const install =
            async () => {
              unlistenChunk =
                await listen<ChunkEvent>(
                  "multillm://chunk",
                  (event) => {
                    if (
                      event.payload
                        .operationId !==
                      operationId
                    ) {
                      return;
                    }

                    output +=
                      event.payload.text;

                    onChunk(
                      event.payload.text,
                    );
                  },
                );

              unlistenDone =
                await listen<DoneEvent>(
                  "multillm://done",
                  (event) => {
                    if (
                      event.payload
                        .operationId !==
                      operationId
                    ) {
                      return;
                    }

                    if (
                      event.payload
                        .cancelled
                    ) {
                      fail(
                        new Error(
                          "Council execution cancelled.",
                        ),
                      );
                    } else {
                      succeed();
                    }
                  },
                );

              unlistenError =
                await listen<ErrorEvent>(
                  "multillm://error",
                  (event) => {
                    if (
                      event.payload
                        .operationId !==
                      operationId
                    ) {
                      return;
                    }

                    fail(
                      new Error(
                        event.payload
                          .message,
                      ),
                    );
                  },
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
                  maxTokens:
                    provider.maxTokens,
                  messages,
                });
              } catch (error) {
                fail(error);
              }
            };

          void install();
        },
      );
    };

  const runMemberWithFailover =
    async (
      member:
        CouncilMember,
      messages:
        MultiLlmMessage[],
      onProviderChange: (
        providerId:
          CouncilProviderId,
        attempt:
          number,
        total:
          number,
      ) => void,
      onChunk: (
        text: string,
      ) => void,
    ): Promise<{
      output: string;
      providerId:
        CouncilProviderId;
      errors: string[];
    }> => {
      const primary =
        configuredProviders.find(
          (provider) =>
            provider.id ===
            member.providerId,
        );

      const ollama =
        configuredProviders.find(
          (provider) =>
            provider.id ===
            "ollama" &&
            provider.id !==
            member.providerId,
        );

      const remaining =
        configuredProviders.filter(
          (provider) =>
            provider.id !==
              member.providerId &&
            provider.id !==
              "ollama",
        );

      const candidates = [
        ...(primary
          ? [primary]
          : []),
        ...(ollama
          ? [ollama]
          : []),
        ...remaining,
      ];

      if (
        candidates.length ===
        0
      ) {
        throw new Error(
          `${member.name}: no configured providers are available.`,
        );
      }

      const errors:
        string[] = [];

      for (
        let index = 0;
        index <
        candidates.length;
        index += 1
      ) {
        if (
          cancelledRef.current
        ) {
          throw new Error(
            "Council execution cancelled.",
          );
        }

        const provider =
          candidates[index];

        onProviderChange(
          provider.id,
          index + 1,
          candidates.length,
        );

        try {
          let attemptOutput =
            "";

          const output =
            await runProviderStream(
              provider,
              messages,
              (chunk) => {
                attemptOutput +=
                  chunk;

                onChunk(chunk);
              },
            );

          return {
            output,
            providerId:
              provider.id,
            errors,
          };
        } catch (error) {
          const message =
            error instanceof Error
              ? error.message
              : String(error);

          if (
            cancelledRef.current ||
            message
              .toLowerCase()
              .includes(
                "cancelled",
              )
          ) {
            throw error;
          }

          errors.push(
            `${provider.label}: ${message}`,
          );

          console.warn(
            `Council ${member.name} provider ${provider.id} failed:`,
            error,
          );
        }
      }

      throw new Error(
        `${member.name}: all providers failed. ${errors.join(
          " | ",
        )}`,
      );
    };

  const runCouncil =
    async () => {
      const userPrompt =
        prompt.trim();

      if (
        !userPrompt ||
        isRunning
      ) {
        return;
      }

      const activeMembers =
        ROLE_ORDER
          .map((role) =>
            members.find(
              (member) =>
                member.id ===
                role,
            ),
          )
          .filter(
            (
              member,
            ): member is CouncilMember =>
              Boolean(
                member?.enabled,
              ),
          );

      if (
        activeMembers.length ===
        0
      ) {
        onMessage(
          "Unable to run Council: enable at least one member.",
        );
        return;
      }

      const judge =
        activeMembers.find(
          (member) =>
            member.id ===
            "judge",
        );

      if (!judge) {
        onMessage(
          "Unable to run Council: the Judge must be enabled.",
        );
        return;
      }

      cancelledRef.current =
        false;
      setIsRunning(true);
      setFinalAnswer("");

      const initialSteps =
        activeMembers.map(
          (
            member,
          ): CouncilStepResult => ({
            role: member.id,
            memberName:
              member.name,
            providerId:
              member.providerId,
            status: "idle",
            output: "",
          }),
        );

      setSteps(initialSteps);

      const completed:
        CouncilStepResult[] = [];

      const sessionId =
        crypto.randomUUID();

      try {
        for (
          const member
          of activeMembers
        ) {
          if (
            cancelledRef.current
          ) {
            throw new Error(
              "Council execution cancelled.",
            );
          }

          const startedAt =
            Date.now();

          setSteps(
            (current) =>
              current.map(
                (step) =>
                  step.role ===
                  member.id
                    ? {
                        ...step,
                        status:
                          "running",
                        startedAt,
                      }
                    : step,
              ),
          );

          const previousWork =
            completed.length ===
            0
              ? "No previous council work is available."
              : completed
                  .map(
                    (step) =>
                      [
                        `## ${step.memberName}`,
                        `Provider: ${step.providerId}`,
                        "",
                        step.status ===
                        "done"
                          ? step.output
                          : `FAILED: ${
                              step.error ??
                              "No usable output."
                            }`,
                      ].join(
                        "\n",
                      ),
                  )
                  .join(
                    "\n\n---\n\n",
                  );

          const messages:
            MultiLlmMessage[] = [
            {
              role: "system",
              content:
                member.systemPrompt,
            },
            {
              role: "user",
              content: [
                "# Original User Request",
                "",
                userPrompt,
                "",
                "# Previous Council Work",
                "",
                previousWork,
                "",
                "# Your Task",
                "",
                member.id ===
                "judge"
                  ? [
                      "Produce the final polished answer.",
                      "Ignore failed council members and use only successful outputs.",
                      "Do not invent missing analysis.",
                      "Briefly mention important missing coverage only when necessary.",
                    ].join("\n")
                  : `Complete your responsibilities as the ${member.name}.`,
              ].join("\n"),
            },
          ];

          let activeProviderId =
            member.providerId;

          let attemptLabel = "";

          try {
            const result =
              await runMemberWithFailover(
                member,
                messages,
                (
                  providerId,
                  attempt,
                  total,
                ) => {
                  activeProviderId =
                    providerId;

                  attemptLabel =
                    total > 1
                      ? `Trying ${providerId} (${attempt}/${total})…`
                      : `Using ${providerId}…`;

                  setSteps(
                    (current) =>
                      current.map(
                        (step) =>
                          step.role ===
                          member.id
                            ? {
                                ...step,
                                providerId,
                                status:
                                  "running",
                                error:
                                  attemptLabel,
                                output:
                                  "",
                              }
                            : step,
                      ),
                  );
                },
                (chunk) => {
                  setSteps(
                    (current) =>
                      current.map(
                        (step) =>
                          step.role ===
                          member.id
                            ? {
                                ...step,
                                providerId:
                                  activeProviderId,
                                error:
                                  attemptLabel,
                                output:
                                  step.output +
                                  chunk,
                              }
                            : step,
                      ),
                  );
                },
              );

            const completedStep:
              CouncilStepResult = {
              role: member.id,
              memberName:
                member.name,
              providerId:
                result.providerId,
              status: "done",
              output:
                result.output,
              startedAt,
              completedAt:
                Date.now(),
            };

            completed.push(
              completedStep,
            );

            setSteps(
              (current) =>
                current.map(
                  (step) =>
                    step.role ===
                    member.id
                      ? completedStep
                      : step,
                ),
            );

            if (
              member.id ===
              "judge"
            ) {
              setFinalAnswer(
                result.output,
              );
            }
          } catch (error) {
            if (
              cancelledRef.current
            ) {
              throw error;
            }

            const failure =
              error instanceof Error
                ? error.message
                : String(error);

            const failedStep:
              CouncilStepResult = {
              role: member.id,
              memberName:
                member.name,
              providerId:
                activeProviderId,
              status: "error",
              output: "",
              error:
                failure,
              startedAt,
              completedAt:
                Date.now(),
            };

            completed.push(
              failedStep,
            );

            setSteps(
              (current) =>
                current.map(
                  (step) =>
                    step.role ===
                    member.id
                      ? failedStep
                      : step,
                ),
            );

            console.warn(
              `Council member ${member.name} failed; continuing.`,
              error,
            );
          }

        }

        const judgeOutput =
          completed.find(
            (step) =>
              step.role ===
                "judge" &&
              step.status ===
                "done",
          )?.output ??
          [...completed]
            .reverse()
            .find(
              (step) =>
                step.status ===
                  "done" &&
                step.output.trim(),
            )?.output ??
          "";

        if (
          !finalAnswer &&
          judgeOutput
        ) {
          setFinalAnswer(
            judgeOutput,
          );
        }

        const timestamp =
          Date.now();

        const session:
          CouncilSession = {
          id: sessionId,
          title:
            createSessionTitle(
              userPrompt,
            ),
          prompt:
            userPrompt,
          createdAt:
            timestamp,
          updatedAt:
            timestamp,
          favorite:
            false,
          steps:
            completed,
          finalAnswer:
            judgeOutput,
        };

        const next =
          upsertCouncilSession(
            session,
          );

        setSessions(next);
        setSelectedSessionId(
          session.id,
        );

        onMessage(
          "AI Council completed successfully.",
        );
      } catch (error) {
        const message =
          String(error);

        setSteps(
          (current) =>
            current.map(
              (step) =>
                step.status ===
                "running"
                  ? {
                      ...step,
                      status:
                        "error",
                      error:
                        message,
                      completedAt:
                        Date.now(),
                    }
                  : step,
            ),
        );

        onMessage(
          `AI Council failed: ${message}`,
        );
      } finally {
        setIsRunning(false);
        currentOperationRef.current =
          null;
      }
    };

  const stopCouncil =
    async () => {
      cancelledRef.current =
        true;

      const operationId =
        currentOperationRef.current;

      if (operationId) {
        try {
          await cancelMultiLlmStream(
            operationId,
          );
        } catch {
          // Ignore cancellation race.
        }
      }

      setIsRunning(false);
      onMessage(
        "AI Council stopped.",
      );
    };

  const loadSession =
    (
      session:
        CouncilSession,
    ) => {
      setSelectedSessionId(
        session.id,
      );
      setPrompt(
        session.prompt,
      );
      setSteps(
        session.steps,
      );
      setFinalAnswer(
        session.finalAnswer,
      );
    };

  const removeSession =
    (
      session:
        CouncilSession,
    ) => {
      const confirmed =
        window.confirm(
          `Delete "${session.title}"?`,
        );

      if (!confirmed) {
        return;
      }

      const next =
        deleteCouncilSession(
          session.id,
        );

      setSessions(next);

      if (
        selectedSessionId ===
        session.id
      ) {
        setSelectedSessionId(
          null,
        );
        setSteps([]);
        setFinalAnswer("");
      }

      onMessage(
        "Council session deleted.",
      );
    };

  const toggleSessionFavorite =
    (
      session:
        CouncilSession,
    ) => {
      const updated = {
        ...session,
        favorite:
          !session.favorite,
        updatedAt:
          Date.now(),
      };

      const next =
        upsertCouncilSession(
          updated,
        );

      setSessions(next);
    };

  const exportSession =
    async (
      session:
        CouncilSession,
      format:
        | "markdown"
        | "json",
    ) => {
      try {
        const extension =
          format ===
          "json"
            ? "json"
            : "md";

        const filePath =
          await save({
            defaultPath:
              `${safeFilename(
                session.title,
              )}.${extension}`,
            filters: [
              {
                name:
                  format ===
                  "json"
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

        const content =
          format === "json"
            ? JSON.stringify(
                session,
                null,
                2,
              )
            : [
                `# ${session.title}`,
                "",
                "## User Request",
                "",
                session.prompt,
                "",
                ...session.steps.flatMap(
                  (step) => [
                    "---",
                    "",
                    `## ${step.memberName}`,
                    "",
                    `- Provider: ${step.providerId}`,
                    `- Status: ${step.status}`,
                    "",
                    step.output ||
                      step.error ||
                      "_No output._",
                    "",
                  ],
                ),
                "---",
                "",
                "## Final Answer",
                "",
                session.finalAnswer ||
                  "_No final answer._",
                "",
              ].join("\n");

        await writeTextFile(
          filePath,
          content,
        );

        onMessage(
          `Council session exported to ${filePath}`,
        );
      } catch (error) {
        onMessage(
          `Council export failed: ${String(
            error,
          )}`,
        );
      }
    };

  const selectedSession =
    sessions.find(
      (session) =>
        session.id ===
        selectedSessionId,
    ) ?? null;

  return (
    <section className="page-section council-page">
      <div className="page-heading">
        <div>
          <h1>
            AI Council
          </h1>
          <p>
            Multiple specialised AI roles collaborate, critique and produce one final answer.
          </p>
        </div>

        <div className="council-heading-actions">
          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              const next =
                resetCouncilMembers();

              setMembers(next);

              onMessage(
                "AI Council members reset.",
              );
            }}
          >
            Reset Members
          </button>

          <button
            type="button"
            className="action-button"
            onClick={
              persistMembers
            }
          >
            Save Council
          </button>
        </div>
      </div>

      <div className="council-members-grid">
        {members.map(
          (member) => {
            const provider =
              providers.find(
                (item) =>
                  item.id ===
                  member.providerId,
              );

            return (
              <article
                key={
                  member.id
                }
                className={[
                  "settings-card",
                  "council-member-card",
                  member.enabled
                    ? ""
                    : "council-member-disabled",
                ].join(" ")}
                style={cardStyle}
              >
                <header>
                  <div>
                    <strong>
                      {member.icon}{" "}
                      {member.name}
                    </strong>

                    <small>
                      {provider?.label ??
                        member.providerId}
                    </small>
                  </div>

                  <label className="council-member-toggle">
                    <input
                      type="checkbox"
                      checked={
                        member.enabled
                      }
                      onChange={(
                        event,
                      ) =>
                        updateMember(
                          member.id,
                          "enabled",
                          event.target
                            .checked,
                        )
                      }
                    />
                    Enabled
                  </label>
                </header>

                <select
                  value={
                    member.providerId
                  }
                  disabled={
                    isRunning
                  }
                  onChange={(
                    event,
                  ) =>
                    updateMember(
                      member.id,
                      "providerId",
                      event.target
                        .value as CouncilProviderId,
                    )
                  }
                >
                  {providers.map(
                    (item) => (
                      <option
                        key={
                          item.id
                        }
                        value={
                          item.id
                        }
                      >
                        {item.label} ·{" "}
                        {item.model}
                      </option>
                    ),
                  )}
                </select>

                <button
                  type="button"
                  className="secondary-button"
                  onClick={() =>
                    setEditingMember(
                      editingMember ===
                        member.id
                        ? null
                        : member.id,
                    )
                  }
                >
                  Edit System Prompt
                </button>

                {editingMember ===
                  member.id && (
                  <textarea
                    className="council-system-prompt"
                    value={
                      member.systemPrompt
                    }
                    onChange={(
                      event,
                    ) =>
                      updateMember(
                        member.id,
                        "systemPrompt",
                        event.target
                          .value,
                      )
                    }
                  />
                )}
              </article>
            );
          },
        )}
      </div>

      <div className="council-workspace">
        <aside
          className="settings-card council-history"
          style={cardStyle}
        >
          <div className="council-history-heading">
            <strong>
              Council History
            </strong>
            <span>
              {sessions.length}
            </span>
          </div>

          <input
            type="search"
            value={
              sessionSearch
            }
            placeholder="Search sessions…"
            onChange={(
              event,
            ) =>
              setSessionSearch(
                event.target
                  .value,
              )
            }
          />

          <div className="council-history-list">
            {filteredSessions.length ===
            0 ? (
              <p>
                No Council sessions yet.
              </p>
            ) : (
              filteredSessions.map(
                (session) => (
                  <div
                    key={
                      session.id
                    }
                    className={[
                      "council-history-item",
                      selectedSessionId ===
                      session.id
                        ? "council-history-item-active"
                        : "",
                    ].join(" ")}
                  >
                    <button
                      type="button"
                      onClick={() =>
                        loadSession(
                          session,
                        )
                      }
                    >
                      <strong>
                        {session.favorite
                          ? "★ "
                          : ""}
                        {
                          session.title
                        }
                      </strong>

                      <small>
                        {new Date(
                          session.createdAt,
                        ).toLocaleString()}
                      </small>
                    </button>

                    <button
                      type="button"
                      title="Favorite"
                      onClick={() =>
                        toggleSessionFavorite(
                          session,
                        )
                      }
                    >
                      {session.favorite
                        ? "★"
                        : "☆"}
                    </button>

                    <button
                      type="button"
                      title="Delete"
                      onClick={() =>
                        removeSession(
                          session,
                        )
                      }
                    >
                      🗑
                    </button>
                  </div>
                ),
              )
            )}
          </div>
        </aside>

        <main className="council-main">
          <div
            className="settings-card council-compose"
            style={cardStyle}
          >
            <textarea
              value={prompt}
              disabled={
                isRunning
              }
              placeholder="Describe the problem or task for the AI Council…"
              onChange={(event) =>
                setPrompt(
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
                  (
                    event.metaKey ||
                    event.ctrlKey
                  )
                ) {
                  event.preventDefault();
                  void runCouncil();
                }
              }}
            />

            <div className="council-compose-actions">
              <span>
                {
                  members.filter(
                    (member) =>
                      member.enabled,
                  ).length
                }{" "}
                active member(s)
              </span>

              <button
                type="button"
                className="danger-button"
                disabled={
                  !isRunning
                }
                onClick={() => {
                  void stopCouncil();
                }}
              >
                Stop
              </button>

              <button
                type="button"
                className="action-button"
                disabled={
                  isRunning ||
                  !prompt.trim()
                }
                onClick={() => {
                  void runCouncil();
                }}
              >
                🏛 Run Council
              </button>
            </div>
          </div>

          {steps.length > 0 && (
            <div className="council-steps">
              {steps.map(
                (step) => {
                  const member =
                    members.find(
                      (item) =>
                        item.id ===
                        step.role,
                    );

                  return (
                    <article
                      key={
                        step.role
                      }
                      className={[
                        "settings-card",
                        "council-step-card",
                        `council-step-${step.status}`,
                      ].join(" ")}
                      style={cardStyle}
                    >
                      <header>
                        <div>
                          <strong>
                            {member?.icon}{" "}
                            {
                              step.memberName
                            }
                          </strong>

                          <small>
                            {
                              step.providerId
                            }
                          </small>
                        </div>

                        <span className="council-step-status">
                          {step.status ===
                          "running"
                            ? step.error ||
                              "Thinking…"
                            : step.status}
                        </span>
                      </header>

                      <div className="council-step-output">
                        <MarkdownRenderer
                          artifactSource="Council"
                          artifactProvider={
                            step.providerId
                          }
                          content={
                            step.output ||
                            step.error ||
                            ""
                          }
                          fallback={
                            step.status ===
                            "idle"
                              ? "Waiting for the previous council member."
                              : "Waiting for output…"
                          }
                        />
                      </div>
                    </article>
                  );
                },
              )}
            </div>
          )}

          {finalAnswer && (
            <article
              className="settings-card council-final-answer"
              style={cardStyle}
            >
              <header>
                <div>
                  <strong>
                    ⚖️ Final Answer
                  </strong>
                  <small>
                    Synthesised by the Judge
                  </small>
                </div>

                {selectedSession && (
                  <div className="council-export-actions">
                    <button
                      type="button"
                      className="secondary-button"
                      onClick={() => {
                        void exportSession(
                          selectedSession,
                          "markdown",
                        );
                      }}
                    >
                      Export Markdown
                    </button>

                    <button
                      type="button"
                      className="secondary-button"
                      onClick={() => {
                        void exportSession(
                          selectedSession,
                          "json",
                        );
                      }}
                    >
                      Export JSON
                    </button>
                  </div>
                )}
              </header>

              <div className="council-final-content">
                <MarkdownRenderer
                  artifactSource="Council"
                  artifactProvider={
                    steps.find(
                      (step) =>
                        step.role ===
                        "judge",
                    )?.providerId
                  }
                  content={
                    finalAnswer
                  }
                />
              </div>
            </article>
          )}
        </main>
      </div>
    </section>
  );
}

export default AiCouncilPage;
