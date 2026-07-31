import { useEffect, useMemo, useState } from "react";
import {
  listAiCenterModels,
  type AiCenterModelChoice,
} from "../services/aiCenter";
import { PROVIDERS_CHANGED_EVENT } from "../services/providers";

type AiArenaPageProps = {
  onOpenMyAi: () => void;
};

type ArenaTeam = "independent" | "team-a" | "team-b" | "team-c" | "team-d";

type ArenaParticipant = {
  id: string;
  modelKey: string;
  team: ArenaTeam;
};

const arenaModes = [
  ["Compare", "Side by side"],
  ["Debate", "Make a case"],
  ["Collaborate", "Build together"],
  ["Compete", "Score a result"],
  ["Role play", "Take a role"],
  ["Game", "Play by rules"],
  ["Simulate", "Model a world"],
] as const;

const teamLabels: Record<ArenaTeam, string> = {
  independent: "Independent",
  "team-a": "Team A",
  "team-b": "Team B",
  "team-c": "Team C",
  "team-d": "Team D",
};

function createParticipant(team: ArenaTeam = "independent"): ArenaParticipant {
  return { id: crypto.randomUUID(), modelKey: "", team };
}

function presetParticipants(preset: "1v1" | "1v1v1" | "2v2"): ArenaParticipant[] {
  if (preset === "1v1") return [createParticipant("team-a"), createParticipant("team-b")];
  if (preset === "2v2") {
    return [
      createParticipant("team-a"),
      createParticipant("team-a"),
      createParticipant("team-b"),
      createParticipant("team-b"),
    ];
  }
  return [createParticipant(), createParticipant(), createParticipant()];
}

function AiArenaPage({ onOpenMyAi }: AiArenaPageProps) {
  const [participants, setParticipants] = useState(() => presetParticipants("1v1"));
  const [selectedMode, setSelectedMode] = useState<(typeof arenaModes)[number][0]>("Compare");
  const [availableModels, setAvailableModels] = useState<AiCenterModelChoice[]>(
    () => listAiCenterModels(),
  );

  useEffect(() => {
    const refreshModels = () => {
      setAvailableModels(listAiCenterModels());
    };

    window.addEventListener(PROVIDERS_CHANGED_EVENT, refreshModels);
    window.addEventListener("storage", refreshModels);

    refreshModels();

    return () => {
      window.removeEventListener(PROVIDERS_CHANGED_EVENT, refreshModels);
      window.removeEventListener("storage", refreshModels);
    };
  }, []);
  const selectedCount = participants.filter((participant) => participant.modelKey).length;
  const formation = useMemo(() => {
    const teams = participants.reduce<Record<string, number>>((current, participant) => {
      const label = teamLabels[participant.team];
      current[label] = (current[label] ?? 0) + 1;
      return current;
    }, {});
    return Object.entries(teams).map(([label, count]) => `${label} ${count}`).join(" · ");
  }, [participants]);

  function updateParticipant(id: string, patch: Partial<ArenaParticipant>) {
    setParticipants((current) => current.map((participant) =>
      participant.id === id ? { ...participant, ...patch } : participant,
    ));
  }

  return (
    <section className="arena-page">
      <header className="arena-header">
        <div>
          <p className="settings-kicker">Multi‑AI interaction</p>
          <h1>AI Arena</h1>
          <p>Set the rules, build any formation, and watch different AIs think together.</p>
        </div>
        <span className="roadmap-badge">Roadmap · P17</span>
      </header>

      <div className="arena-mode-rail" aria-label="Arena modes">
        {arenaModes.map(([mode, description], index) => (
          <button
            key={mode}
            type="button"
            className={
              selectedMode === mode
                ? "arena-mode-card arena-mode-active"
                : "arena-mode-card"
            }
            aria-pressed={selectedMode === mode}
            onClick={() => setSelectedMode(mode)}
          >
            <span>{String(index + 1).padStart(2, "0")}</span>
            <strong>{mode}</strong>
            <small>{description}</small>
          </button>
        ))}
      </div>

      <section className="arena-roster" aria-labelledby="arena-roster-title">
        <header className="arena-roster-header">
          <div>
            <p className="settings-kicker">Formation</p>
            <h2 id="arena-roster-title">Participants</h2>
            <span>{participants.length} participants · {selectedCount} models selected</span>
          </div>
          <div className="arena-preset-actions" aria-label="Formation presets">
            <span>Presets</span>
            <button type="button" onClick={() => setParticipants(presetParticipants("1v1"))}>1v1</button>
            <button type="button" onClick={() => setParticipants(presetParticipants("1v1v1"))}>1v1v1</button>
            <button type="button" onClick={() => setParticipants(presetParticipants("2v2"))}>2v2</button>
          </div>
        </header>

        <div className="arena-participant-grid">
          {participants.map((participant, index) => (
            <article key={participant.id} className="arena-participant-card">
              <div className="arena-participant-index">{String(index + 1).padStart(2, "0")}</div>
              <label>
                <span>Model</span>
                <select value={participant.modelKey} onChange={(event) =>
                  updateParticipant(participant.id, { modelKey: event.target.value })}>
                  <option value="">Choose a connected model</option>
                  {availableModels.map((model) => (
                    <option key={`${model.providerInstanceId}:${model.modelId}`}
                      value={`${model.providerInstanceId}:${model.modelId}`}>{model.label}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>Side</span>
                <select value={participant.team} onChange={(event) =>
                  updateParticipant(participant.id, { team: event.target.value as ArenaTeam })}>
                  {Object.entries(teamLabels).map(([value, label]) =>
                    <option key={value} value={value}>{label}</option>)}
                </select>
              </label>
              <button type="button" className="arena-remove-participant"
                aria-label={`Remove participant ${index + 1}`}
                disabled={participants.length <= 2}
                onClick={() => setParticipants((current) => current.filter((item) => item.id !== participant.id))}>×</button>
            </article>
          ))}
        </div>

        <footer className="arena-roster-footer">
          <span>{formation}</span>
          <div>
            {!availableModels.length && <button type="button" onClick={onOpenMyAi}>Connect models</button>}
            <button type="button" disabled={participants.length >= 12}
              onClick={() => setParticipants((current) => [...current, createParticipant()])}>＋ Add participant</button>
          </div>
        </footer>
      </section>

      <div className="arena-prompt-shell">
        <label htmlFor="arena-prompt">{selectedMode} opening prompt</label>
        <textarea id="arena-prompt" placeholder="Give the participants a question, challenge, or scenario…" disabled />
        <div>
          <span>
            {selectedMode} configuration and execution arrive with P17. This screen currently previews the planned interaction modes.
          </span>
          <button type="button" disabled>Start arena</button>
        </div>
      </div>
    </section>
  );
}

export default AiArenaPage;
