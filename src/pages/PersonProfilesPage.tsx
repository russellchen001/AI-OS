import { useCallback, useEffect, useMemo, useState } from "react";
import {
  activatePersonProfile,
  buildPersonPersonaSkill,
  getPersonProfile,
  listPersonProfiles,
  reviewPersonProfile,
  rollbackPersonProfile,
} from "../services/personProfiles";
import {
  CATEGORISED_FIELDS,
  CLAIM_CATEGORIES,
  type ClaimCategory,
  type CognitiveClaim,
  type PendingCognitiveCandidate,
  type PersonProfileView,
  type ProfileSummary,
  type RunnablePersonaSkill,
} from "../types/personProfile";

type PersonProfilesPageProps = {
  onMessage: (message: string) => void;
};

/** What the reviewer has decided about one drafted claim, before submitting. */
type PendingDecision = {
  category: ClaimCategory | "reject" | "";
  correctedStatement: string;
};

function describeError(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

function PersonProfilesPage({ onMessage }: PersonProfilesPageProps) {
  const [summaries, setSummaries] = useState<ProfileSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<PersonProfileView | null>(null);
  const [reviewer, setReviewer] = useState("");
  const [decisions, setDecisions] = useState<Record<string, PendingDecision>>({});
  const [skill, setSkill] = useState<RunnablePersonaSkill | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshList = useCallback(async () => {
    try {
      const listed = await listPersonProfiles();
      setSummaries(listed);
      setSelectedId((current) => current ?? listed[0]?.profileId ?? null);
    } catch (error) {
      onMessage(describeError(error));
    }
  }, [onMessage]);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  useEffect(() => {
    if (!selectedId) return;
    let cancelled = false;
    getPersonProfile(selectedId)
      .then((next) => {
        if (cancelled) return;
        setView(next);
        setSkill(null);
        setDecisions(
          Object.fromEntries([
            ...next.profile.unclassifiedClaims.map((claim) => [
              claim.claimId,
              { category: "", correctedStatement: "" } as PendingDecision,
            ]),
            ...next.profile.pendingCognitiveCandidates.map((candidate) => [
              candidate.candidateId,
              { category: "", correctedStatement: "" } as PendingDecision,
            ]),
          ]),
        );
      })
      .catch((error) => {
        if (!cancelled) onMessage(describeError(error));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, onMessage]);

  const profile = view?.profile ?? null;
  const pending = profile?.unclassifiedClaims ?? [];
  const cognitivePending = profile?.pendingCognitiveCandidates ?? [];

  const undecidedCount = useMemo(
    () =>
      pending.filter((claim) => !decisions[claim.claimId]?.category).length +
      cognitivePending.filter(
        (candidate) => !decisions[candidate.candidateId]?.category,
      ).length,
    [pending, cognitivePending, decisions],
  );

  async function run(
    action: () => Promise<PersonProfileView>,
    success: string,
  ): Promise<void> {
    setBusy(true);
    try {
      const next = await action();
      setView(next);
      setSkill(null);
      await refreshList();
      onMessage(success);
    } catch (error) {
      onMessage(describeError(error));
    } finally {
      setBusy(false);
    }
  }

  function submitReview() {
    if (!profile) return;
    const named = reviewer.trim();
    if (!named) {
      onMessage("Add your name before submitting a review, so the decision is attributable.");
      return;
    }
    if (undecidedCount > 0) {
      onMessage(
        `${undecidedCount} claim${undecidedCount === 1 ? "" : "s"} still undecided. Every claim must be categorised or rejected.`,
      );
      return;
    }
    void run(
      () =>
        reviewPersonProfile(profile.profileId, {
          reviewer: named,
          decisions: pending.map((claim) => {
            const decision = decisions[claim.claimId];
            const corrected = decision.correctedStatement.trim();
            return {
              claimId: claim.claimId,
              category: decision.category === "reject" ? null : (decision.category as ClaimCategory),
              correctedStatement:
                corrected && corrected !== claim.statement ? corrected : null,
            };
          }),
          cognitiveCandidateDecisions: cognitivePending.map((candidate) => {
            const decision = decisions[candidate.candidateId];
            const corrected = decision.correctedStatement.trim();
            return {
              candidateId: candidate.candidateId,
              category: decision.category === "reject" ? null : (decision.category as ClaimCategory),
              correctedStatement:
                corrected && corrected !== candidate.statement ? corrected : null,
            };
          }),
        }),
      "Review recorded.",
    );
  }

  function setDecision(claimId: string, patch: Partial<PendingDecision>) {
    setDecisions((current) => ({
      ...current,
      [claimId]: { ...current[claimId], ...patch },
    }));
  }

  return (
    <section className="person-profiles-page">
      <header className="person-profiles-header">
        <div>
          <p className="settings-kicker">My AI · Person profiles</p>
          <h1>Person Profiles</h1>
          <p>
            Review what AI-OS distilled from your evidence. Nothing becomes active until you
            categorise every claim.
          </p>
        </div>
      </header>

      {summaries.length === 0 && (
        <article className="person-profile-empty">
          <h2>No profiles yet</h2>
          <p>
            Person profiles appear here after a distillation run. Each one arrives as a draft with
            every claim uncategorised, waiting for your review.
          </p>
        </article>
      )}

      {summaries.length > 0 && (
        <div className="person-profile-list">
          {summaries.map((summary) => (
            <button
              key={summary.profileId}
              type="button"
              className={
                summary.profileId === selectedId
                  ? "person-profile-chip person-profile-chip-selected"
                  : "person-profile-chip"
              }
              onClick={() => setSelectedId(summary.profileId)}
            >
              <span>{summary.profileId}</span>
              <span className="person-profile-chip-meta">
                r{summary.latestRevision} · {summary.status}
                {summary.activeRevision !== null &&
                  summary.activeRevision !== summary.latestRevision &&
                  ` · live r${summary.activeRevision}`}
              </span>
            </button>
          ))}
        </div>
      )}

      {profile && view && (
        <>
          <article className="person-profile-status">
            <div>
              <h2>{profile.profileId}</h2>
              <p>
                Revision {profile.revision} · {profile.status} · subject recorded as{" "}
                <strong>{profile.subjectKind}</strong>
              </p>
              {view.activeRevision !== null && view.activeRevision !== profile.revision && (
                <p className="person-profile-note">
                  Revision {view.activeRevision} is still the live one. Opening a new revision does
                  not take the active profile down.
                </p>
              )}
              {profile.contradictions.length > 0 && (
                <p className="person-profile-warning">
                  {profile.contradictions.length} contradiction
                  {profile.contradictions.length === 1 ? "" : "s"} in the evidence. They are carried
                  through rather than resolved automatically.
                </p>
              )}
            </div>
            <div className="person-profile-actions">
              <button
                type="button"
                disabled={busy || profile.status !== "reviewed"}
                onClick={() =>
                  void run(() => activatePersonProfile(profile.profileId), "Profile activated.")
                }
              >
                Activate
              </button>
              <button
                type="button"
                disabled={busy || view.activeRevision === null}
                onClick={() => {
                  setBusy(true);
                  buildPersonPersonaSkill(profile.profileId)
                    .then((built) => {
                      setSkill(built);
                      onMessage("Persona skill built from the live revision.");
                    })
                    .catch((error) => onMessage(describeError(error)))
                    .finally(() => setBusy(false));
                }}
              >
                Build persona skill
              </button>
            </div>
          </article>

          {profile.draftNarrative.length > 0 && (
            <article className="person-profile-narrative">
              <h3>What the creator wrote</h3>
              <p className="person-profile-note">
                This is prose, not evidence-linked claims. It is here for you to read; it is never
                promoted into the profile automatically, and it is cleared once you submit a review.
              </p>
              {profile.draftNarrative.map((section, index) => (
                <details key={`${section.origin}-${index}`}>
                  <summary>
                    {section.origin} · {section.adapter}
                  </summary>
                  <pre>{section.text}</pre>
                </details>
              ))}
            </article>
          )}

          {(pending.length > 0 || cognitivePending.length > 0) && (
            <article className="person-profile-review">
              <h3>Profile material awaiting your decision</h3>
              <p className="person-profile-note">
                Distilly claims come directly from evidence observations. Nuwa items are derived
                cognitive inferences. Neither becomes canonical until you categorise or reject it,
                and evidence links cannot be changed during review.
              </p>

              <label className="person-profile-reviewer">
                <span>Reviewer</span>
                <input
                  value={reviewer}
                  onChange={(event) => setReviewer(event.target.value)}
                  placeholder="Your name, recorded in the revision history"
                />
              </label>

              {pending.length > 0 && (
                <div>
                  <h4>Evidence-derived claims</h4>
                  {pending.map((claim) => (
                    <ClaimRow
                      key={claim.claimId}
                      claim={claim}
                      decision={
                        decisions[claim.claimId] ?? { category: "", correctedStatement: "" }
                      }
                      onChange={(patch) => setDecision(claim.claimId, patch)}
                    />
                  ))}
                </div>
              )}

              {cognitivePending.length > 0 && (
                <div>
                  <h4>Nuwa cognitive candidates</h4>
                  <p className="person-profile-note">
                    These passed AI-OS schema and evidence-reference validation, but they remain
                    model-derived inferences. Accepting one keeps it marked unconfirmed.
                  </p>
                  {cognitivePending.map((candidate) => (
                    <CognitiveCandidateRow
                      key={candidate.candidateId}
                      candidate={candidate}
                      decision={
                        decisions[candidate.candidateId] ?? {
                          category: "",
                          correctedStatement: "",
                        }
                      }
                      onChange={(patch) => setDecision(candidate.candidateId, patch)}
                    />
                  ))}
                </div>
              )}

              <div className="person-profile-review-footer">
                <span>
                  {undecidedCount === 0
                    ? "Every review item decided."
                    : `${undecidedCount} still undecided.`}
                </span>
                <button type="button" disabled={busy || undecidedCount > 0} onClick={submitReview}>
                  Submit review
                </button>
              </div>
            </article>
          )}

          <article className="person-profile-categorised">
            <h3>Categorised claims</h3>
            {CATEGORISED_FIELDS.every(
              (field) => (profile[field.key] as CognitiveClaim[]).length === 0,
            ) && <p className="person-profile-note">Nothing categorised yet.</p>}
            {CATEGORISED_FIELDS.map((field) => {
              const claims = profile[field.key] as CognitiveClaim[];
              if (claims.length === 0) return null;
              return (
                <div key={field.key}>
                  <h4>{field.label}</h4>
                  <ul>
                    {claims.map((claim) => (
                      <li key={claim.claimId}>
                        {claim.statement}
                        {!claim.confirmed && <em> (unconfirmed)</em>}
                      </li>
                    ))}
                  </ul>
                </div>
              );
            })}
          </article>

          {skill && (
            <article className="person-profile-skill">
              <h3>Persona skill · revision {skill.profileRevision}</h3>
              <pre>{skill.instructions}</pre>
            </article>
          )}

          <article className="person-profile-history">
            <h3>History</h3>
            <p className="person-profile-note">
              {view.revisionCount} stored record{view.revisionCount === 1 ? "" : "s"}. Rollback
              republishes an earlier revision as a new one; nothing here is ever overwritten.
            </p>
            <ol>
              {profile.revisionHistory.map((record, index) => (
                <li key={`${record.revision}-${index}`}>
                  <strong>r{record.revision}</strong> {record.reason}
                  {record.revision !== view.activeRevision && view.activeRevision !== null && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => rollbackPersonProfile(profile.profileId, record.revision),
                          `Republished revision ${record.revision}.`,
                        )
                      }
                    >
                      Roll back to r{record.revision}
                    </button>
                  )}
                </li>
              ))}
            </ol>
          </article>
        </>
      )}
    </section>
  );
}

type CognitiveCandidateRowProps = {
  candidate: PendingCognitiveCandidate;
  decision: PendingDecision;
  onChange: (patch: Partial<PendingDecision>) => void;
};

function CognitiveCandidateRow({
  candidate,
  decision,
  onChange,
}: CognitiveCandidateRowProps) {
  const evidenceCount =
    candidate.evidenceIds.length + candidate.contradictoryEvidenceIds.length;

  return (
    <div className="person-profile-claim">
      <textarea
        value={decision.correctedStatement || candidate.statement}
        onChange={(event) => onChange({ correctedStatement: event.target.value })}
        rows={2}
      />
      <div className="person-profile-claim-meta">
        <span>
          {candidate.adapter} · {candidate.kind.split("-").join(" ")}
        </span>
        <span>
          {evidenceCount} evidence reference{evidenceCount === 1 ? "" : "s"}
        </span>
        <span>confidence {Math.round(candidate.confidence * 100)}%</span>
        <span>derived inference · unconfirmed if accepted</span>
        {candidate.contradictoryEvidenceIds.length > 0 && <span>contradicted</span>}
      </div>
      <select
        value={decision.category}
        onChange={(event) =>
          onChange({ category: event.target.value as PendingDecision["category"] })
        }
      >
        <option value="">Undecided…</option>
        {CLAIM_CATEGORIES.map((category) => (
          <option key={category.value} value={category.value}>
            {category.label}
          </option>
        ))}
        <option value="reject">Reject this candidate</option>
      </select>
    </div>
  );
}

type ClaimRowProps = {
  claim: CognitiveClaim;
  decision: PendingDecision;
  onChange: (patch: Partial<PendingDecision>) => void;
};

function ClaimRow({ claim, decision, onChange }: ClaimRowProps) {
  const evidenceCount = claim.evidenceIds.length + claim.contradictoryEvidenceIds.length;
  return (
    <div className="person-profile-claim">
      <textarea
        value={decision.correctedStatement || claim.statement}
        onChange={(event) => onChange({ correctedStatement: event.target.value })}
        rows={2}
      />
      <div className="person-profile-claim-meta">
        <span>
          {evidenceCount} evidence reference{evidenceCount === 1 ? "" : "s"}
        </span>
        <span>confidence {Math.round(claim.confidence * 100)}%</span>
        {claim.contradictoryEvidenceIds.length > 0 && <span>contradicted</span>}
      </div>
      <select
        value={decision.category}
        onChange={(event) =>
          onChange({ category: event.target.value as PendingDecision["category"] })
        }
      >
        <option value="">Undecided…</option>
        {CLAIM_CATEGORIES.map((category) => (
          <option key={category.value} value={category.value}>
            {category.label}
          </option>
        ))}
        <option value="reject">Reject this claim</option>
      </select>
    </div>
  );
}

export default PersonProfilesPage;
