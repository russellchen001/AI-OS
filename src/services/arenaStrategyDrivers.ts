import type {
  ArenaAssemblyPlan,
  ArenaStrategy,
  ArenaStrategyAction,
  ArenaStrategyDriverPlan,
} from "../types/arena";

function seatsByRole(
  assembly: ArenaAssemblyPlan,
  role: string,
): string[] {
  return assembly.seats
    .filter((seat) => seat.role === role)
    .map((seat) => seat.id);
}

function participants(assembly: ArenaAssemblyPlan): string[] {
  return seatsByRole(assembly, "participant");
}

function evaluationSeats(assembly: ArenaAssemblyPlan): string[] {
  return assembly.seats
    .filter((seat) =>
      ["judge", "evaluator", "observer", "synthesizer"].includes(
        seat.role,
      ),
    )
    .map((seat) => seat.id);
}

function action(
  id: string,
  round: number,
  strategy: ArenaStrategy,
  actorSeatIds: string[],
  instruction: string,
  outputVisibility: ArenaStrategyAction["outputVisibility"] = "public",
  allowFailure = true,
): ArenaStrategyAction {
  return {
    id,
    round,
    strategy,
    actorSeatIds,
    instruction,
    outputVisibility,
    allowFailure,
  };
}

function compareDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const contenders = participants(assembly);
  const evaluators = evaluationSeats(assembly);

  return {
    strategy: "compare",
    maxRounds: 3,
    requiresHiddenState: false,
    requiresWorldState: false,
    actions: [
      action(
        "compare-independent",
        1,
        "compare",
        contenders,
        "Produce an independent answer. Do not imitate other participants. State assumptions and concrete reasoning.",
      ),
      action(
        "compare-critique",
        2,
        "compare",
        contenders,
        "Review the public candidate outputs. Identify disagreements, unsupported claims and trade-offs. Preserve meaningful dissent.",
      ),
      action(
        "compare-evaluate",
        3,
        "compare",
        evaluators,
        "Evaluate the candidate work against the objective without relying on provider identity. Synthesize strengths, weaknesses and unresolved disagreement.",
        "public",
        false,
      ),
    ],
  };
}

function debateDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const debaters = participants(assembly);
  const moderators = seatsByRole(assembly, "moderator");
  const judges = evaluationSeats(assembly);

  return {
    strategy: "debate",
    maxRounds: 4,
    requiresHiddenState: false,
    requiresWorldState: false,
    actions: [
      action(
        "debate-opening",
        1,
        "debate",
        debaters,
        "Present an independent opening position with explicit claims, evidence needs and assumptions.",
      ),
      action(
        "debate-rebuttal",
        2,
        "debate",
        debaters,
        "Directly challenge the strongest opposing claims visible in the Arena context. Do not merely restate your opening.",
      ),
      action(
        "debate-moderation",
        3,
        "debate",
        moderators,
        "Identify unanswered questions, rule violations and the central disputed issues. Keep the debate bounded.",
      ),
      action(
        "debate-judgment",
        4,
        "debate",
        judges,
        "Assess argument quality, evidence, rebuttals and remaining uncertainty. Preserve dissent instead of forcing false consensus.",
        "public",
        false,
      ),
    ],
  };
}

function collaborationDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const collaborators = participants(assembly);
  const synthesizers = evaluationSeats(assembly);

  return {
    strategy: "collaboration",
    maxRounds: 3,
    requiresHiddenState: false,
    requiresWorldState: false,
    actions: [
      action(
        "collaboration-contribute",
        1,
        "collaboration",
        collaborators,
        "Contribute your distinct expertise independently. Focus on complementary value rather than duplicating other roles.",
      ),
      action(
        "collaboration-review",
        2,
        "collaboration",
        collaborators,
        "Review the shared work. Correct weaknesses, integrate compatible ideas and explicitly flag unresolved conflicts.",
      ),
      action(
        "collaboration-synthesis",
        3,
        "collaboration",
        synthesizers,
        "Produce an integrated result from the shared work while preserving material disagreement, assumptions and uncertainty.",
        "public",
        false,
      ),
    ],
  };
}

function competitionDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const competitors = participants(assembly);
  const evaluators = evaluationSeats(assembly);

  return {
    strategy: "competition",
    maxRounds: 2,
    requiresHiddenState: false,
    requiresWorldState: false,
    actions: [
      action(
        "competition-submit",
        1,
        "competition",
        competitors,
        "Solve the same task independently. Do not refer to other competitors. Produce a self-contained candidate result.",
      ),
      action(
        "competition-score",
        2,
        "competition",
        evaluators,
        "Blindly evaluate the public candidate results using objective-specific criteria. Record ties and evaluator disagreement when appropriate.",
        "public",
        false,
      ),
    ],
  };
}

function rolePlayDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const actors = participants(assembly);
  const observers = evaluationSeats(assembly);

  return {
    strategy: "role-play",
    maxRounds: 3,
    requiresHiddenState: true,
    requiresWorldState: false,
    actions: [
      action(
        "roleplay-position",
        1,
        "role-play",
        actors,
        "Act only from your assigned role, private goals and visible context. Do not reveal private context unless the scenario requires it.",
      ),
      action(
        "roleplay-interaction",
        2,
        "role-play",
        actors,
        "Respond to the public interaction while remaining consistent with your role, incentives and information boundary.",
      ),
      action(
        "roleplay-observe",
        3,
        "role-play",
        observers,
        "Summarize the interaction, stakeholder tensions and insights. Do not invent access to participant-private context.",
        "public",
        false,
      ),
    ],
  };
}

function gameDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const players = participants(assembly);
  const controllers = [
    ...seatsByRole(assembly, "rules-controller"),
    ...seatsByRole(assembly, "moderator"),
  ];
  const observers = evaluationSeats(assembly);

  return {
    strategy: "game",
    maxRounds: 4,
    requiresHiddenState: true,
    requiresWorldState: true,
    actions: [
      action(
        "game-rules",
        1,
        "game",
        controllers,
        "Validate the public rules, legal actions and current public state. Never disclose participant-private hidden roles.",
        "public",
        false,
      ),
      action(
        "game-private-decision",
        2,
        "game",
        players,
        "Choose your action using only public information plus your own participant-private and permitted team-private information.",
      ),
      action(
        "game-public-turn",
        3,
        "game",
        players,
        "Take the next public turn based only on your projected visible context. Never claim knowledge of another player's hidden role unless it was publicly revealed.",
      ),
      action(
        "game-observe",
        4,
        "game",
        observers,
        "Evaluate legal play and summarize the public game state without revealing runtime-private or participant-private information.",
        "public",
        false,
      ),
    ],
  };
}

function simulationDriver(
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  const actors = participants(assembly);
  const environment = seatsByRole(assembly, "environment");
  const evaluators = evaluationSeats(assembly);

  return {
    strategy: "simulation",
    maxRounds: 4,
    requiresHiddenState: true,
    requiresWorldState: true,
    actions: [
      action(
        "simulation-initial-state",
        1,
        "simulation",
        environment,
        "Describe the explicit initial environment state, assumptions and uncertainties. Distinguish observed evidence from simulated state.",
        "public",
        false,
      ),
      action(
        "simulation-actors",
        2,
        "simulation",
        actors,
        "Act from your assigned actor context and currently visible world state. Simulated behavior is not evidence of a real person's private thoughts.",
      ),
      action(
        "simulation-transition",
        3,
        "simulation",
        environment,
        "Apply bounded state transitions from the actors' public actions. Record changed assumptions and scenario events.",
        "public",
        false,
      ),
      action(
        "simulation-evaluate",
        4,
        "simulation",
        evaluators,
        "Evaluate outcomes, uncertainty, counterfactuals and limitations. Separate observed evidence from simulated inference.",
        "public",
        false,
      ),
    ],
  };
}

export function buildArenaStrategyDriver(
  strategy: ArenaStrategy,
  assembly: ArenaAssemblyPlan,
): ArenaStrategyDriverPlan {
  switch (strategy) {
    case "compare":
      return compareDriver(assembly);

    case "debate":
      return debateDriver(assembly);

    case "collaboration":
      return collaborationDriver(assembly);

    case "competition":
      return competitionDriver(assembly);

    case "role-play":
      return rolePlayDriver(assembly);

    case "game":
      return gameDriver(assembly);

    case "simulation":
      return simulationDriver(assembly);
  }
}
