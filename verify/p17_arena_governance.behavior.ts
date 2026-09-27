import {
  createArenaOutcome,
} from "../src/services/arenaGovernance";

function assert(value: unknown, message: string): void {
  if (!value) throw new Error(message);
}

const base = {
  strategy: "competition" as const,
  seed: 17,
  runtimeStatus: "completed" as const,
  round: 2,
  maxRounds: 2,
  degraded: false,
};

const scored = createArenaOutcome({
  ...base,
  scores: [
    { scope: "participant", evaluatorSeatId: "j1", subjectId: "a", criterionId: "quality", value: 8 },
    { scope: "participant", evaluatorSeatId: "j1", subjectId: "b", criterionId: "quality", value: 6 },
  ],
});
assert(scored.winnerIds[0] === "a", "score winner incorrect");

const tied = createArenaOutcome({
  ...base,
  scores: [
    { scope: "participant", evaluatorSeatId: "j1", subjectId: "a", criterionId: "quality", value: 8 },
    { scope: "participant", evaluatorSeatId: "j1", subjectId: "b", criterionId: "quality", value: 8 },
  ],
  tiePolicy: "preserve",
});
assert(tied.status === "no-consensus", "tie must remain visible");

const seedA = createArenaOutcome({
  ...base,
  votes: [
    { scope: "participant", evaluatorSeatId: "j1", targetId: "a", weight: 1 },
    { scope: "participant", evaluatorSeatId: "j2", targetId: "b", weight: 1 },
  ],
  tiePolicy: "seeded",
});

const seedB = createArenaOutcome({
  ...base,
  votes: [
    { scope: "participant", evaluatorSeatId: "j2", targetId: "b", weight: 1 },
    { scope: "participant", evaluatorSeatId: "j1", targetId: "a", weight: 1 },
  ],
  tiePolicy: "seeded",
});

assert(
  seedA.winnerIds[0] === seedB.winnerIds[0],
  "seeded tie-break depends on input order",
);

const degraded = createArenaOutcome({
  ...base,
  degraded: true,
  explicitWinnerIds: ["a"],
});
assert(degraded.status === "degraded", "degraded state lost");

const cancelled = createArenaOutcome({
  ...base,
  runtimeStatus: "cancelled",
});
assert(cancelled.status === "cancelled", "cancelled state lost");

console.log("PASS — score aggregation");
console.log("PASS — visible no-consensus tie");
console.log("PASS — fixed-seed deterministic tie-break");
console.log("PASS — degraded outcome preservation");
console.log("PASS — terminal runtime-state preservation");
console.log("P17 ARENA GOVERNANCE BEHAVIOR: PASS=5 FAIL=0");
