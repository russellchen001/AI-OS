import {
  SkillManifest,
  SkillCapability,
} from "../types/skill";

import {
  resolveSkill,
} from "./skillResolver";

import {
  checkSkillPermission,
} from "./skillPermission";


export type SkillExecutionStatus =
  | "ready"
  | "requires-approval"
  | "skill-not-found"
  | "unsupported";


export type SkillExecutionContext = {
  capability: SkillCapability;
  payload?: unknown;
};


export type SkillExecutionPlan = {
  status: SkillExecutionStatus;

  skill?: SkillManifest;

  executor?: {
    type: string;
    handler: string;
  };

  reason?: string;
};


export function prepareSkillExecution(
  context: SkillExecutionContext,
): SkillExecutionPlan {


  const resolution = resolveSkill(
    context.capability,
  );


  if (!resolution) {
    return {
      status: "skill-not-found",
      reason:
        `No skill supports capability: ${context.capability}`,
    };
  }


  const skill = resolution.skill;


  if (!skill) {
    return {
      status: "skill-not-found",
      reason:
        `Skill resolution did not return a usable skill for capability: ${context.capability}`,
    };
  }


  const permission =
    checkSkillPermission(skill);


  if (permission === "requires-approval") {
    return {
      status: "requires-approval",
      skill,
      reason:
        "Skill permissions require user approval.",
    };
  }


  return {
    status: "ready",

    skill,

    executor: {
      type: skill.executor.type,
      handler: skill.executor.handler,
    },
  };
}
