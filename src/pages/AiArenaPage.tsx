import { useEffect, useMemo, useRef, useState } from "react";
import {
  listAiCenterModels,
  type AiCenterModelChoice,
} from "../services/aiCenter";
import {
  createArenaProductSession,
} from "../services/arenaProductSession";
import {
  ArenaRuntimeControls,
} from "../components/arena/ArenaRuntimeControls";
import {
  ArenaOutcomePanel,
} from "../components/arena/ArenaOutcomePanel";
import { PROVIDERS_CHANGED_EVENT } from "../services/providers";
import type {
  ArenaAssemblyPlan,
  ArenaModelConstraint,
  ArenaRuntimeResult,
  ArenaSessionStatus,
} from "../types/arena";

type AiArenaPageProps = {
  onOpenMyAi: () => void;
};

function modelKey(model: AiCenterModelChoice): string {
  return `${model.providerId}:${model.providerInstanceId}:${model.modelId}`;
}

function formatStrategy(value: string): string {
  return value
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function AiArenaPage({ onOpenMyAi }: AiArenaPageProps) {
  const [objective, setObjective] = useState("");
  const [availableModels, setAvailableModels] =
    useState<AiCenterModelChoice[]>(
      () => listAiCenterModels(),
    );
  const [preferredModelKey, setPreferredModelKey] =
    useState("");
  const [pinPreferredModel, setPinPreferredModel] =
    useState(false);
  const [plan, setPlan] =
    useState<ArenaAssemblyPlan | null>(null);
  const [error, setError] = useState("");
const [isPlanning, setIsPlanning] = useState(false);
  const arenaSessionRef = useRef(createArenaProductSession());
  const [runtimeStatus, setRuntimeStatus] =
    useState<ArenaSessionStatus>("created");
  const [result, setResult] =
    useState<ArenaRuntimeResult | null>(null);
  const [isRunning, setIsRunning] = useState(false);

  useEffect(() => {
    const refreshModels = () => {
      setAvailableModels(listAiCenterModels());
    };

    window.addEventListener(
      PROVIDERS_CHANGED_EVENT,
      refreshModels,
    );
    window.addEventListener("storage", refreshModels);
    refreshModels();

    return () => {
      window.removeEventListener(
        PROVIDERS_CHANGED_EVENT,
        refreshModels,
      );
      window.removeEventListener(
        "storage",
        refreshModels,
      );
    };
  }, []);

  const preferredModel = useMemo(
    () =>
      availableModels.find(
        (model) => modelKey(model) === preferredModelKey,
      ),
    [availableModels, preferredModelKey],
  );

  async function buildPlan() {
    const normalized = objective.trim();

    if (!normalized) {
      setError("Describe a question, challenge, scenario, or game first.");
      return;
    }

    if (!availableModels.length) {
      setError("Connect at least one AI Center model first.");
      return;
    }

    setIsPlanning(true);
    setError("");
    setPlan(null);
    setResult(null);
    setRuntimeStatus("created");

    try {
      const explicitConstraints: ArenaModelConstraint[] =
        preferredModel
          ? [
              pinPreferredModel
                ? {
                    mode: "pinned",
                    providerId: preferredModel.providerId,
                    providerInstanceId:
                      preferredModel.providerInstanceId,
                    modelId: preferredModel.modelId,
                    label: preferredModel.label,
                  }
                : {
                    mode: "prefer",
                    providerId: preferredModel.providerId,
                    providerInstanceId:
                      preferredModel.providerInstanceId,
                    modelId: preferredModel.modelId,
                    label: preferredModel.label,
                  },
            ]
          : [];

      const nextPlan = await arenaSessionRef.current.plan(
        normalized,
        explicitConstraints,
      );

      setPlan(nextPlan);
    } catch (planningError) {
      setError(
        planningError instanceof Error
          ? planningError.message
          : String(planningError),
      );
    } finally {
      setIsPlanning(false);
    }
  }

  async function runArena() {
    if (!plan || isRunning) return;
    setError("");
    setResult(null);
    setIsRunning(true);
    setRuntimeStatus("running");

    try {
      const nextResult = await arenaSessionRef.current.run(
        plan,
        {},
        () => setRuntimeStatus(arenaSessionRef.current.getStatus()),
      );
      setResult(nextResult);
      setRuntimeStatus(nextResult.status);
    } catch (runtimeError) {
      const status = arenaSessionRef.current.getStatus();
      setRuntimeStatus(status === "running" ? "failed" : status);
      setError(runtimeError instanceof Error ? runtimeError.message : String(runtimeError));
    } finally {
      setIsRunning(false);
    }
  }

  function pauseArena() {
    arenaSessionRef.current.pause();
    setRuntimeStatus(arenaSessionRef.current.getStatus());
  }

  function resumeArena() {
    arenaSessionRef.current.resume();
    setRuntimeStatus(arenaSessionRef.current.getStatus());
  }

  async function stopArena() {
    await arenaSessionRef.current.stop();
    setRuntimeStatus(arenaSessionRef.current.getStatus());
  }

  async function cancelArena() {
    await arenaSessionRef.current.cancel();
    setRuntimeStatus(arenaSessionRef.current.getStatus());
  }

  return (
    <section className="arena-page">
      <header className="arena-header">
        <div>
          <p className="settings-kicker">
            Multi-AI interaction
          </p>
          <h1>AI Arena</h1>
          <p>
            Describe the problem. AI-OS chooses the Arena
            strategy, formation, roles, and models.
          </p>
        </div>
        <span className="roadmap-badge">
          P17 · Final integration
        </span>
      </header>

      <section className="arena-intent-card">
        <div className="arena-intent-heading">
          <div>
            <p className="settings-kicker">
              Arena objective
            </p>
            <h2>What should the AIs do?</h2>
          </div>
          <span>
            Strategy is selected automatically
          </span>
        </div>

        <textarea
          id="arena-objective"
          value={objective}
          onChange={(event) =>
            setObjective(event.target.value)
          }
          placeholder="Example: Let several AIs compare two database architectures, challenge each other, then have an independent judge evaluate the trade-offs."
          rows={6}
        />

        <div className="arena-model-preference">
          <label>
            <span>
              Optional model preference
            </span>
            <select
              value={preferredModelKey}
              onChange={(event) =>
                setPreferredModelKey(
                  event.target.value,
                )
              }
            >
              <option value="">
                Auto — let AI-OS assign models
              </option>
              {availableModels.map((model) => (
                <option
                  key={modelKey(model)}
                  value={modelKey(model)}
                >
                  {model.label}
                </option>
              ))}
            </select>
          </label>

          <label className="arena-pin-control">
            <input
              type="checkbox"
              checked={pinPreferredModel}
              disabled={!preferredModel}
              onChange={(event) =>
                setPinPreferredModel(
                  event.target.checked,
                )
              }
            />
            <span>
              Pin this model — never silently replace it
            </span>
          </label>
        </div>

        <footer className="arena-intent-footer">
          <div>
            {!availableModels.length ? (
              <button
                type="button"
                onClick={onOpenMyAi}
              >
                Connect models
              </button>
            ) : (
              <span>
                {availableModels.length} connected model
                {availableModels.length === 1
                  ? ""
                  : "s"}
              </span>
            )}
          </div>

          <button
            type="button"
            onClick={buildPlan}
            disabled={
              isPlanning ||
              isRunning ||
              !objective.trim() ||
              !availableModels.length
            }
          >
            {isPlanning
              ? "Planning Arena…"
              : "Plan Arena"}
          </button>
        </footer>

        {error && (
          <p
            className="arena-planning-error"
            role="alert"
          >
            {error}
          </p>
        )}
      </section>

      {plan && (
        <section className="arena-plan-card">
          <header>
            <div>
              <p className="settings-kicker">
                Arena plan
              </p>
              <h2>AI-OS assembled the Arena</h2>
            </div>
            <span>
              {plan.chiefOfStaff.mode ===
              "llm-chief-of-staff"
                ? "LLM Chief of Staff"
                : "Deterministic fallback"}
            </span>
          </header>

          <div className="arena-strategy-summary">
            <strong>Strategy</strong>
            <div>
              {plan.strategyPlan.strategies.map(
                (strategy, index) => (
                  <span key={strategy}>
                    {index > 0 && "→ "}
                    {formatStrategy(strategy)}
                  </span>
                ),
              )}
            </div>
            <p>{plan.strategyPlan.rationale}</p>
          </div>

          <div className="arena-formation-summary">
            <div>
              <strong>
                {plan.seats.length} seats
              </strong>
              <span>
                Chief of Staff chose the minimum
                sufficient formation.
              </span>
            </div>

            <div className="arena-planned-seat-grid">
              {plan.seats.map((seat) => {
                const assignment =
                  plan.modelAssignments.find(
                    (item) =>
                      item.seatId === seat.id,
                  );

                return (
                  <article key={seat.id}>
                    <div className="arena-seat-topline">
                      <span>{seat.role}</span>
                      <span>
                        {seat.modelConstraint.mode}
                      </span>
                    </div>
                    <h3>{seat.title}</h3>
                    <p>{seat.purpose}</p>
                    <strong>
                      {assignment?.choice.label ??
                        "Unassigned"}
                    </strong>
                    <small>
                      {assignment?.rationale}
                    </small>
                  </article>
                );
              })}
            </div>
          </div>

          <ArenaRuntimeControls
            status={runtimeStatus}
            isRunning={isRunning}
            onStart={() => void runArena()}
            onPause={pauseArena}
            onResume={resumeArena}
            onStop={() => void stopArena()}
            onCancel={() => void cancelArena()}
          />

          <ArenaOutcomePanel result={result} />
        </section>
      )}
    </section>
  );
}

export default AiArenaPage;
