import {
  ArenaProductSession,
} from "../src/services/arenaProductSession";
import type {
  ArenaAssemblyPlan,
  ArenaRuntimeResult,
} from "../src/types/arena";
import type {
  ArenaRuntime,
} from "../src/services/arenaRuntime";

function assert(value: unknown, message: string): void {
  if (!value) throw new Error(message);
}

const plan = {
  id: "product-plan",
  objective: "product flow",
} as ArenaAssemblyPlan;

const result: ArenaRuntimeResult = {
  sessionId: "product-session",
  status: "completed",
  strategy: "compare",
  startedAt: 1,
  completedAt: 2,
  roundsCompleted: 1,
  contexts: [],
  metrics: [],
  events: [],
  degraded: false,
  errors: [],
  outcome: {
    strategy: "compare",
    seed: 42,
    status: "completed",
    completionReason: "rule-complete",
    winnerIds: [],
    scores: [],
    votes: [],
    provenance: [],
    rationale: ["product result"],
  },
};

let status = "created";
let planned = "";
let constraintCount = 0;
let runCount = 0;
let pauseCount = 0;
let resumeCount = 0;
let stopCount = 0;
let cancelCount = 0;

const runtime = {
  run: async (received: ArenaAssemblyPlan) => {
    assert(received === plan, "Product Session changed the approved Arena plan.");
    runCount += 1;
    status = "completed";
    return result;
  },
  getStatus: () => status,
  pause: () => { pauseCount += 1; status = "paused"; },
  resume: () => { resumeCount += 1; status = "running"; },
  stop: async () => { stopCount += 1; status = "stopped"; },
  cancel: async () => { cancelCount += 1; status = "cancelled"; },
} as unknown as ArenaRuntime;

const session = new ArenaProductSession({
  plan: async (objective, constraints) => {
    planned = objective;
    constraintCount = constraints.length;
    return plan;
  },
  createRuntime: () => runtime,
});

const assembled = await session.plan(
  "product objective",
  [{ mode: "auto" }],
);

assert(assembled === plan, "Product Session did not preserve the AssemblyPlan.");
assert(planned === "product objective", "Objective did not reach planning.");
assert(constraintCount === 1, "Model constraints did not reach planning.");

assert(await session.run(plan) === result, "Runtime result was not returned.");
assert(runCount === 1, "Arena Runtime was not invoked exactly once.");

session.pause();
session.resume();
await session.stop();
await session.cancel();

assert(pauseCount === 1, "Pause was not delegated.");
assert(resumeCount === 1, "Resume was not delegated.");
assert(stopCount === 1, "Stop was not delegated.");
assert(cancelCount === 1, "Cancel was not delegated.");

console.log("PASS — UI product flow preserves ArenaAssemblyPlan");
console.log("PASS — objective and model constraints reach planning");
console.log("PASS — approved plan reaches Shared Arena Runtime");
console.log("PASS — structured ArenaRuntimeResult returns to product layer");
console.log("PASS — pause/resume lifecycle delegates to Shared Arena Runtime");
console.log("PASS — stop/cancel lifecycle delegates to Shared Arena Runtime");
console.log("P17 FINAL PRODUCT FLOW: PASS=6 FAIL=0");
