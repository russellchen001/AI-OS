import type {
  ArenaSessionStatus,
} from "../../types/arena";

type Props = {
  status: ArenaSessionStatus;
  isRunning: boolean;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  onCancel: () => void;
};

export function ArenaRuntimeControls({
  status,
  isRunning,
  onStart,
  onPause,
  onResume,
  onStop,
  onCancel,
}: Props) {
  const active =
    status === "running" ||
    status === "paused";

  return (
    <section className="arena-runtime-panel">
      <header>
        <div>
          <p className="settings-kicker">Arena runtime</p>
          <h3>Run the assembled Arena</h3>
        </div>
        <span data-status={status}>{status}</span>
      </header>
      <div className="arena-runtime-controls">
        <button onClick={onStart} disabled={active || isRunning}>
          {isRunning ? "Arena running…" : "Start Arena"}
        </button>
        <button onClick={onPause} disabled={status !== "running"}>
          Pause
        </button>
        <button onClick={onResume} disabled={status !== "paused"}>
          Resume
        </button>
        <button onClick={onStop} disabled={!active}>Stop</button>
        <button onClick={onCancel} disabled={!active}>Cancel</button>
      </div>
    </section>
  );
}
