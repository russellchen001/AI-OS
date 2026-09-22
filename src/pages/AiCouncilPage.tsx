import {
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  save,
} from "@tauri-apps/plugin-dialog";
import {
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import MarkdownRenderer from "../components/MarkdownRenderer";
import {
  recordAnalyticsEvent,
} from "../services/analytics";
import {
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
import {
  CouncilCancelledError,
  createCouncilRuntime,
} from "../services/councilRuntime";
import {
  createCouncilChiefOfStaff,
} from "../services/councilChiefOfStaff";
import {
  createCouncilExecutionCoordinator,
} from "../services/councilExecution";
import type {
  CouncilMember,
  CouncilSession,
  CouncilStepResult,
} from "../types/council";
import type {
  ProviderId,
} from "../types/provider";
import type {
  CouncilAssemblyPlan,
  CouncilRecommendation,
} from "../types/councilAssembly";

type AiCouncilPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

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
    assemblyPlan,
    setAssemblyPlan,
  ] = useState<CouncilAssemblyPlan | null>(null);

  const [
    recommendation,
    setRecommendation,
  ] = useState<CouncilRecommendation | null>(null);

  const [
    isAssembling,
    setIsAssembling,
  ] = useState(false);

  const [
    isRunning,
    setIsRunning,
  ] = useState(false);

  const [
    editingMember,
    setEditingMember,
  ] = useState<
    string | null
  >(null);

  const [
    isCreatingTask,
    setIsCreatingTask,
  ] = useState(false);

  const [
    sessionSearch,
    setSessionSearch,
  ] = useState("");

  const councilRuntimeRef =
    useRef(
      createCouncilRuntime(),
    );

  const chiefOfStaffRef =
    useRef(
      createCouncilChiefOfStaff(),
    );

  const executionCoordinatorRef =
    useRef(
      createCouncilExecutionCoordinator(),
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
      id: string,
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

  const runCouncil = async () => {
    const userPrompt = prompt.trim();
    if (!userPrompt || isRunning) return;

    const startedAt = Date.now();
    setIsRunning(true);
    setFinalAnswer("");
    setRecommendation(null);
    setAssemblyPlan(null);
    setIsAssembling(true);

    try {
      let nextAssemblyPlan: CouncilAssemblyPlan | undefined;
      try {
        nextAssemblyPlan = await chiefOfStaffRef.current.assemble({
          objective: userPrompt,
        });
        setAssemblyPlan(nextAssemblyPlan);
      } catch (error) {
        onMessage(
          `Dynamic assembly unavailable; using the legacy Council fallback: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      } finally {
        setIsAssembling(false);
      }

      const result = await councilRuntimeRef.current.run({
        prompt: userPrompt,
        members,
        assemblyPlan: nextAssemblyPlan,
        callbacks: {
          onCouncilStarted: (sessionId, initialSteps) => {
            setSteps(initialSteps);
            recordAnalyticsEvent({
              module: "council",
              type: "started",
              title: "AI Council started",
              description: `${initialSteps.length} active member(s)`,
              inputTokens: Math.ceil(userPrompt.length / 4),
              metadata: {
                sessionId,
                memberCount: initialSteps.length,
                tokenEstimate: true,
              },
            });
          },
          onMemberStarted: (nextStep) => {
            setSteps((current) =>
              current.map((step) =>
                step.role === nextStep.role ? nextStep : step,
              ),
            );
          },
          onProviderChanged: (memberId, providerId, attempt, total) => {
            setSteps((current) =>
              current.map((step) =>
                step.role === memberId
                  ? {
                      ...step,
                      providerId,
                      status: "running",
                      output: "",
                      error:
                        total > 1
                          ? `Trying ${providerId} (${attempt}/${total})…`
                          : `Using ${providerId}…`,
                    }
                  : step,
              ),
            );
          },
          onChunk: (memberId, providerId, text) => {
            setSteps((current) =>
              current.map((step) =>
                step.role === memberId
                  ? {
                      ...step,
                      providerId,
                      output: step.output + text,
                    }
                  : step,
              ),
            );
          },
          onMemberCompleted: (nextStep) => {
            setSteps((current) =>
              current.map((step) =>
                step.role === nextStep.role ? nextStep : step,
              ),
            );
            recordAnalyticsEvent({
              module: "council",
              type: "success",
              title: `Council ${nextStep.memberName} completed`,
              description: `${nextStep.providerId} · ${nextStep.role}`,
              provider: nextStep.providerId,
              outputTokens: Math.ceil(nextStep.output.length / 4),
              latencyMs:
                nextStep.completedAt && nextStep.startedAt
                  ? nextStep.completedAt - nextStep.startedAt
                  : undefined,
              metadata: { role: nextStep.role, tokenEstimate: true },
            });
          },
          onMemberFailed: (nextStep) => {
            setSteps((current) =>
              current.map((step) =>
                step.role === nextStep.role ? nextStep : step,
              ),
            );
          },
        },
      });

      setSteps(result.steps);
      setFinalAnswer(result.finalAnswer);
      setRecommendation(result.recommendation ?? null);
      const next = upsertCouncilSession(result.session);
      setSessions(next);
      setSelectedSessionId(result.session.id);
      recordAnalyticsEvent({
        module: "council",
        type: "completed",
        title: "AI Council completed",
        description: result.session.title,
        outputTokens: Math.ceil(result.finalAnswer.length / 4),
        latencyMs: Date.now() - startedAt,
        metadata: {
          sessionId: result.session.id,
          memberCount: result.steps.length,
          successfulMembers: result.steps.filter((step) => step.status === "done").length,
          integrationIds: result.metadata.integrationIds.join(","),
          tokenEstimate: true,
        },
      });
      onMessage("AI Council completed successfully.");
    } catch (error) {
      if (error instanceof CouncilCancelledError) {
        onMessage("AI Council stopped.");
      } else {
        const message = error instanceof Error ? error.message : String(error);
        setSteps((current) =>
          current.map((step) =>
            step.status === "running"
              ? { ...step, status: "error", error: message, completedAt: Date.now() }
              : step,
          ),
        );
        onMessage(`AI Council failed: ${message}`);
      }
    } finally {
      setIsAssembling(false);
      setIsRunning(false);
    }
  };

  const stopCouncil = async () => {
    await councilRuntimeRef.current.cancel();
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


  const persistExecution = (
    session: CouncilSession,
    execution: NonNullable<CouncilSession["execution"]>,
  ): void => {
    const next = upsertCouncilSession({
      ...session,
      execution,
      updatedAt: Date.now(),
    });
    setSessions(next);
  };

  const approveRecommendation = async () => {
    if (!selectedSession || isCreatingTask) return;
    setIsCreatingTask(true);
    try {
      const execution = await executionCoordinatorRef.current.approve(selectedSession);
      persistExecution(selectedSession, execution);
      onMessage(
        `Council recommendation approved and handed to Task Engine as ${execution.linkage.taskId}.`,
      );
    } catch (error) {
      onMessage(
        `Council recommendation handoff failed: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    } finally {
      setIsCreatingTask(false);
    }
  };

  const executeRecommendation = async () => {
    if (!selectedSession?.execution || isCreatingTask) return;
    let approval;
    const current = selectedSession.execution;
    if (
      current.feedback.kind === "progress" &&
      current.feedback.status === "awaiting-confirmation" &&
      current.feedback.approval
    ) {
      const requested = current.feedback.approval;
      const confirmed = window.confirm([
        "Allow this exact Agent-selected Skill once?",
        "",
        requested.capability,
        "",
        JSON.stringify(requested.input, null, 2).slice(0, 2_000),
      ].join("\n"));
      if (!confirmed) {
        onMessage("Execution confirmation cancelled; no Skill was invoked.");
        return;
      }
      approval = {
        capability: requested.capability,
        input: requested.input,
        userConfirmed: true,
      };
    }
    setIsCreatingTask(true);
    try {
      let execution = await executionCoordinatorRef.current.execute(
        selectedSession,
        current,
        approval,
        (feedback) => {
          persistExecution(selectedSession, {
            linkage: {
              councilSessionId: feedback.councilSessionId,
              recommendationId: feedback.recommendationId,
              taskId: feedback.taskId,
              previousTaskId:
                feedback.taskId === current.linkage.taskId
                  ? current.linkage.previousTaskId
                  : current.linkage.taskId,
            },
            feedback,
          });
        },
      );
      if (execution.feedback.kind === "blocker") {
        execution = {
          ...execution,
          reconveneDecision: chiefOfStaffRef.current.evaluateExecutionBlocker(
            execution.feedback,
            selectedSession.assemblyPlan,
          ),
        };
      }
      persistExecution(selectedSession, execution);
      onMessage(execution.feedback.message);
    } finally {
      setIsCreatingTask(false);
    }
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
      setAssemblyPlan(session.assemblyPlan ?? null);
      setRecommendation(session.recommendation ?? null);
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
        setAssemblyPlan(null);
        setRecommendation(null);
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
        <p>Chief of Staff dynamically assembles specialist seats; saved five-role members remain the compatibility fallback.</p>
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
                {assemblyPlan
                  ? `${assemblyPlan.seats.length} assembled seat(s)`
                  : `${members.filter((member) => member.enabled).length} fallback member(s)`}
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
                {isAssembling ? "Assembling Council…" : "Run Council"}
              </button>
            </div>
          </div>

          {assemblyPlan && (
            <article className="settings-card council-current-note" style={cardStyle}>
              <div>
                <strong>Chief of Staff assembled</strong>
                <p>{assemblyPlan.rationale}</p>
              </div>
              <div className="council-members-grid">
                {assemblyPlan.seats.map((seat) => (
                  <div key={seat.id} className="council-member-card">
                    <strong>{seat.title}</strong>
                    <small>
                      {seat.source.fallback
                        ? "Built-in fallback"
                        : `Agency Agents: ${seat.source.sourceId}`}
                    </small>
                    <small>{seat.assignedModel.label}</small>
                  </div>
                ))}
              </div>
            </article>
          )}

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
                  const seat = assemblyPlan?.seats.find(
                    (item) => item.id === step.seatId,
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
                            {member?.icon ?? "●"}{" "}
                            {
                              step.memberName
                            }
                          </strong>

                          <small>
                            {seat?.source.sourceId ?? step.stage ?? "legacy"} · {step.providerId}
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
                    {assemblyPlan
                      ? "Synthesised by the assembled Council"
                      : "Synthesised by the Judge"}
                  </small>
                </div>

                {selectedSession && (
                  <div className="council-export-actions">
                    {!selectedSession.execution && (
                      <button
                        type="button"
                        className="action-button"
                        disabled={isCreatingTask}
                        onClick={() => {
                          void approveRecommendation();
                        }}
                      >
                        {isCreatingTask
                          ? "Creating Task…"
                          : "Approve recommendation"}
                      </button>
                    )}

                    {selectedSession.execution &&
                      selectedSession.execution.feedback.kind === "progress" && (
                      <button
                        type="button"
                        className="action-button"
                        disabled={isCreatingTask}
                        onClick={() => {
                          void executeRecommendation();
                        }}
                      >
                        {isCreatingTask
                          ? "Executing…"
                          : selectedSession.execution.feedback.status === "awaiting-confirmation"
                            ? "Confirm selected Skill"
                            : "Start Agent execution"}
                      </button>
                    )}

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
                {recommendation && (
                  <small>
                    Structured recommendation · {recommendation.risks.length} risk(s) · {recommendation.uncertainty.length} uncertainty item(s)
                  </small>
                )}
                {selectedSession?.execution && (
                  <div className="council-current-note">
                    <span>Execution · {selectedSession.execution.feedback.status}</span>
                    <p>{selectedSession.execution.feedback.message}</p>
                    <small>
                      Council {selectedSession.execution.linkage.councilSessionId} · Recommendation {selectedSession.execution.linkage.recommendationId} · Task {selectedSession.execution.linkage.taskId}
                    </small>
                    {selectedSession.execution.reconveneDecision && (
                      <small>
                        Chief of Staff: {selectedSession.execution.reconveneDecision.action}; user approval is required before any Council reconvenes or execution resumes.
                      </small>
                    )}
                  </div>
                )}
              </div>
            </article>
          )}
        </main>
      </div>
    </section>
  );
}

export default AiCouncilPage;
