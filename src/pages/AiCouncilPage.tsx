import { invoke } from "@tauri-apps/api/core";
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
  recordAnalyticsEvent,
} from "../services/analytics";
import {
  streamThroughAiCenter,
  type AiCenterConversationMessage,
  listAiCenterModels,
} from "../services/aiCenter";
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
  CouncilRole,
  CouncilSession,
  CouncilStepResult,
} from "../types/council";
import type {
  ProviderId,
} from "../types/provider";

type AiCouncilPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

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



const ROLE_ORDER:
  CouncilRole[] = [
  "planner",
  "engineer",
  "researcher",
  "critic",
  "judge",
];

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

  const runMemberWithFailover =
    async (
      member:
        CouncilMember,
      messages:
        AiCenterConversationMessage[],
      onProviderChange: (
        providerId:
          ProviderId,
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
        ProviderId;
      errors: string[];
    }> => {
      const availableModels =
        listAiCenterModels();

      if (
        availableModels.length === 0
      ) {
        throw new Error(
          `${member.name}: no AI Center models are connected.`,
        );
      }

      const preferred =
        availableModels.find(
          (model) =>
            model.providerId ===
            member.providerId,
        );

      const candidates = [
        ...(preferred
          ? [preferred]
          : []),
        ...availableModels.filter(
          (model) =>
            model !== preferred,
        ),
      ];

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

        const choice =
          candidates[index];

        onProviderChange(
          choice.providerId as ProviderId,
          index + 1,
          candidates.length,
        );

        try {
          const stream =
            streamThroughAiCenter(
              messages,
              choice,
              onChunk,
            );

          currentOperationRef.current =
            stream.operationId;

          const result =
            await stream.result;

          if (
            result.cancelled
          ) {
            throw new Error(
              "Council execution cancelled.",
            );
          }

          return {
            output:
              result.response.text,
            providerId:
              choice.providerId as ProviderId,
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
            `${choice.label}: ${message}`,
          );

          console.warn(
            `Council ${member.name} AI Center model ${choice.label} failed:`,
            error,
          );
        }
      }

      throw new Error(
        `${member.name}: all AI Center models failed. ${errors.join(
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

      if (listAiCenterModels().length === 0) {
        onMessage(
          "Unable to run Council: no AI Center model is connected. Connect a model from My AI first.",
        );
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

      const councilStartedAt =
        Date.now();

      cancelledRef.current =
        false;
      setIsRunning(true);

      recordAnalyticsEvent({
        module:
          "council",
        type:
          "started",
        title:
          "AI Council started",
        description:
          `${activeMembers.length} active member(s)`,
        inputTokens:
          Math.ceil(
            userPrompt.length / 4,
          ),
        metadata: {
          sessionId:
            crypto.randomUUID(),
          memberCount:
            activeMembers.length,
          tokenEstimate:
            true,
        },
      });
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
            AiCenterConversationMessage[] = [
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

            recordAnalyticsEvent({
              module:
                "council",
              type:
                "success",
              title:
                `Council ${member.name} completed`,
              description:
                `${result.providerId} · ${member.id}`,
              provider:
                result.providerId,
              outputTokens:
                Math.ceil(
                  result.output.length /
                  4,
                ),
              latencyMs:
                Date.now() -
                startedAt,
              metadata: {
                role:
                  member.id,
                sessionId,
                tokenEstimate:
                  true,
              },
            });

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

            recordAnalyticsEvent({
              module:
                "council",
              type:
                "failure",
              title:
                `Council ${member.name} failed`,
              description:
                failure,
              provider:
                activeProviderId,
              latencyMs:
                Date.now() -
                startedAt,
              metadata: {
                role:
                  member.id,
                sessionId,
              },
            });

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

        recordAnalyticsEvent({
          module:
            "council",
          type:
            "completed",
          title:
            "AI Council completed",
          description:
            session.title,
          outputTokens:
            Math.ceil(
              judgeOutput.length / 4,
            ),
          latencyMs:
            Date.now() -
            councilStartedAt,
          metadata: {
            sessionId:
              session.id,
            memberCount:
              completed.length,
            successfulMembers:
              completed.filter(
                (step) =>
                  step.status ===
                  "done",
              ).length,
            tokenEstimate:
              true,
          },
        });

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

        recordAnalyticsEvent({
          module:
            "council",
          type:
            "failure",
          title:
            "AI Council failed",
          description:
            message,
          latencyMs:
            Date.now() -
            councilStartedAt,
          metadata: {
            sessionId,
          },
        });

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
          if (currentOperationRef.current) {
            await invoke(
              "cancel_provider_response_stream",
              {
                operationId:
                  currentOperationRef.current,
              },
            );
          }
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
          <p className="settings-kicker">AI 智囊团 · Decision support</p>
          <h1>
            AI Council
          </h1>
          <p>
            Bring the right perspectives together, surface disagreement, and turn them into one clear recommendation.
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
            Reset team
          </button>

          <button
            type="button"
            className="action-button"
            onClick={
              persistMembers
            }
          >
            Save team
          </button>
        </div>
      </div>

      <div className="council-process" aria-label="Council process">
        {[
          ["01", "Brief", "Define the decision"],
          ["02", "Assemble", "Chief of Staff proposes expertise"],
          ["03", "Discuss", "Experts challenge assumptions"],
          ["04", "Synthesize", "One decision report"],
        ].map(([index, title, description], stepIndex) => (
          <div key={title} className={stepIndex === 0 ? "council-process-step council-process-active" : "council-process-step"}>
            <span>{index}</span>
            <strong>{title}</strong>
            <small>{description}</small>
          </div>
        ))}
      </div>

      <div className="council-current-note">
        <span>Current workflow</span>
        <p>This build uses your saved specialist roles. Dynamic Chief of Staff team assembly is planned for P16.</p>
      </div>



      <div className="council-members-grid">
        {members.map(
          (member) => {
            const provider =
              listAiCenterModels().find(
                (item) =>
                  item.providerId ===
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
                        .value as ProviderId,
                    )
                  }
                >
                  {listAiCenterModels().length === 0 && (
                    <option value={member.providerId}>
                      No AI Center model connected
                    </option>
                  )}
                  {listAiCenterModels().map(
                    (item) => (
                      <option
                        key={
                          item.providerId
                        }
                        value={
                          item.providerId
                        }
                      >
                        {item.label} ·{" "}
                        {item.label}
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

              {isRunning && (
                <button
                  type="button"
                  className="danger-button"
                  onClick={() => {
                    void stopCouncil();
                  }}
                >
                  Stop
                </button>
              )}

              <button
                type="button"
                className="action-button"
                disabled={
                  isRunning ||
                  !prompt.trim() ||
                  listAiCenterModels().length === 0
                }
                onClick={() => {
                  void runCouncil();
                }}
              >
                Run Council
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
