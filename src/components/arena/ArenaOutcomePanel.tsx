import type {
  ArenaRuntimeResult,
} from "../../types/arena";

type Props = {
  result: ArenaRuntimeResult | null;
};

export function ArenaOutcomePanel({ result }: Props) {
  if (!result) return null;

  const publicContext = result.contexts.filter(
    (entry) => entry.visibility.kind === "public",
  );

  return (
    <>
      <section className="arena-outcome-panel">
        <div><span>Outcome</span><strong>{result.outcome.status}</strong></div>
        <div><span>Completion</span><strong>{result.outcome.completionReason}</strong></div>
        <div><span>Rounds</span><strong>{result.roundsCompleted}</strong></div>
        <div><span>Scores</span><strong>{result.outcome.scores.length}</strong></div>
        <div className="arena-outcome-wide">
          <span>Winner</span>
          <strong>{result.outcome.winnerIds.join(", ") || "No forced winner"}</strong>
        </div>
        {result.outcome.rationale.slice(0, 3).map((item, index) => (
          <p className="arena-outcome-wide" key={`${index}-${item}`}>{item}</p>
        ))}
        {!!result.errors.length && (
          <p className="arena-outcome-wide arena-outcome-error">
            {result.errors.join(" · ")}
          </p>
        )}
      </section>
      {result.outcome.finalAnswer && (
        <section className="arena-transcript">
          <h3>Final task answer</h3>
          <p style={{ whiteSpace: "pre-wrap" }}>{result.outcome.finalAnswer}</p>
        </section>
      )}
      {!!publicContext.length && (
        <section className="arena-transcript">
          <h3>Public Arena transcript</h3>
          {publicContext.map((entry) => (
            <article key={entry.id}>
              <span>Round {entry.round} · {entry.authorId ?? "Arena"}</span>
              <p>{entry.content}</p>
            </article>
          ))}
        </section>
      )}
    </>
  );
}
