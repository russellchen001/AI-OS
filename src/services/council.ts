import type {
  CouncilMember,
  CouncilSession,
} from "../types/council";

const MEMBERS_STORAGE_KEY =
  "ai-os.council.members.v1";

const SESSIONS_STORAGE_KEY =
  "ai-os.council.sessions.v1";

export const DEFAULT_COUNCIL_MEMBERS:
  CouncilMember[] = [
  {
    id: "planner",
    name: "Planner",
    icon: "🧭",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt:
      `You are the Planner in an AI Council.

Analyse the user's objective and produce:
1. A precise interpretation of the task
2. Important assumptions
3. A step-by-step execution plan
4. Risks and constraints
5. Clear instructions for the next council members

Do not attempt to produce the final answer.`,
  },
  {
    id: "engineer",
    name: "Engineer",
    icon: "💻",
    providerId: "claude",
    enabled: true,
    systemPrompt:
      `You are the Engineer in an AI Council.

Use the user's request and the Planner's work to produce a practical solution.

Focus on:
- Technical correctness
- Implementation details
- Reliability
- Maintainability
- Concrete examples or code where appropriate

Do not simply repeat the Planner.`,
  },
  {
    id: "researcher",
    name: "Researcher",
    icon: "📚",
    providerId: "gemini",
    enabled: true,
    systemPrompt:
      `You are the Researcher in an AI Council.

Review the task and previous council work.

Your responsibilities:
- Identify missing information
- Add useful background and context
- Challenge unsupported assumptions
- Compare possible approaches
- Highlight uncertainty
- Improve factual completeness

Clearly distinguish facts, assumptions and recommendations.`,
  },
  {
    id: "critic",
    name: "Critic",
    icon: "🔍",
    providerId: "deepseek",
    enabled: true,
    systemPrompt:
      `You are the Critic in an AI Council.

Critically evaluate all previous work.

Look for:
- Logical errors
- Missing edge cases
- Security and privacy risks
- Weak assumptions
- Contradictions
- Unnecessary complexity
- Better alternatives

Be direct but constructive. Provide specific corrections for the Judge.`,
  },
  {
    id: "judge",
    name: "Judge",
    icon: "⚖️",
    providerId: "chatgpt",
    enabled: true,
    systemPrompt:
      `You are the Judge and final synthesiser in an AI Council.

Use the user's original request and all previous council outputs.

Produce the final answer that:
- Resolves disagreements
- Corrects identified errors
- Preserves the strongest ideas
- Is clear, complete and actionable
- Does not mention internal council mechanics unless useful
- Avoids unsupported claims

Return only the polished final response.`,
  },
];

function normalizeMembers(
  value: unknown,
): CouncilMember[] {
  if (!Array.isArray(value)) {
    return DEFAULT_COUNCIL_MEMBERS;
  }

  return DEFAULT_COUNCIL_MEMBERS.map(
    (defaultMember) => {
      const stored =
        value.find(
          (
            item,
          ): item is Partial<CouncilMember> =>
            typeof item ===
              "object" &&
            item !== null &&
            (
              item as
                Partial<CouncilMember>
            ).id ===
              defaultMember.id,
        );

      return {
        ...defaultMember,
        ...stored,
        id: defaultMember.id,
        name:
          typeof stored?.name ===
          "string"
            ? stored.name
            : defaultMember.name,
        systemPrompt:
          typeof stored?.systemPrompt ===
          "string"
            ? stored.systemPrompt
            : defaultMember.systemPrompt,
        enabled:
          typeof stored?.enabled ===
          "boolean"
            ? stored.enabled
            : defaultMember.enabled,
      };
    },
  );
}

export function loadCouncilMembers():
  CouncilMember[] {
  try {
    const raw =
      localStorage.getItem(
        MEMBERS_STORAGE_KEY,
      );

    if (!raw) {
      saveCouncilMembers(
        DEFAULT_COUNCIL_MEMBERS,
      );
      return DEFAULT_COUNCIL_MEMBERS;
    }

    return normalizeMembers(
      JSON.parse(raw),
    );
  } catch {
    return DEFAULT_COUNCIL_MEMBERS;
  }
}

export function saveCouncilMembers(
  members: CouncilMember[],
): void {
  localStorage.setItem(
    MEMBERS_STORAGE_KEY,
    JSON.stringify(members),
  );
}

export function resetCouncilMembers():
  CouncilMember[] {
  saveCouncilMembers(
    DEFAULT_COUNCIL_MEMBERS,
  );

  return DEFAULT_COUNCIL_MEMBERS;
}

export function loadCouncilSessions():
  CouncilSession[] {
  try {
    const raw =
      localStorage.getItem(
        SESSIONS_STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    return Array.isArray(parsed)
      ? (
          parsed as
            CouncilSession[]
        )
      : [];
  } catch {
    return [];
  }
}

export function saveCouncilSessions(
  sessions: CouncilSession[],
): void {
  localStorage.setItem(
    SESSIONS_STORAGE_KEY,
    JSON.stringify(
      sessions.slice(0, 100),
    ),
  );
}

export function upsertCouncilSession(
  session: CouncilSession,
): CouncilSession[] {
  const current =
    loadCouncilSessions();

  const next = [
    session,
    ...current.filter(
      (item) =>
        item.id !== session.id,
    ),
  ].slice(0, 100);

  saveCouncilSessions(next);
  return next;
}

export function deleteCouncilSession(
  id: string,
): CouncilSession[] {
  const next =
    loadCouncilSessions().filter(
      (session) =>
        session.id !== id,
    );

  saveCouncilSessions(next);
  return next;
}
