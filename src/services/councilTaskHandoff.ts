import { submitChatTask, type SubmitChatTaskResponse } from "./tasks";
import type { CouncilSession } from "../types/council";
import type { CouncilRecommendation } from "../types/councilAssembly";

export function recommendationFromCouncilSession(
  session: CouncilSession,
): CouncilRecommendation {
  if (session.recommendation) return session.recommendation;
  if (!session.finalAnswer.trim()) {
    throw new Error("The Council session has no recommendation to execute.");
  }
  return {
    id: `legacy-${session.id}`,
    councilSessionId: session.id,
    summary: session.finalAnswer,
    recommendedPlan: [session.finalAnswer],
    rationale: [],
    disagreements: [],
    risks: [],
    assumptions: [],
    uncertainty: [],
    provenance: session.metadata?.provenanceReferences ?? [],
    createdAt: session.updatedAt,
  };
}

export function councilRecommendationExecutionPrompt(
  session: CouncilSession,
): string {
  const recommendation = recommendationFromCouncilSession(session);
  return [
    "Execute the following approved AI Council recommendation through the normal AI-OS Task Engine.",
    "The Council is advisory and grants no execution permission.",
    "Planner, user confirmation, Runtime permissions, selected Agent, and Skill confirmation remain mandatory.",
    "Prefer harmless read-only actions when they can satisfy the recommendation.",
    "",
    JSON.stringify({
      councilSessionId: session.id,
      objective: session.prompt,
      recommendation,
    }, null, 2),
  ].join("\n");
}

export async function createCouncilRecommendationTask(
  session: CouncilSession,
): Promise<SubmitChatTaskResponse> {
  const recommendation = recommendationFromCouncilSession(session);
  return submitChatTask(councilRecommendationExecutionPrompt(session), "DO", {
    source: "ai-council",
    councilSessionId: session.id,
    recommendationId: recommendation.id,
  });
}
