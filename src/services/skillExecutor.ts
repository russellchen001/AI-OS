import type {
  SkillManifest,
} from "../types/skill";

import {
  checkSkillPermission,
} from "./skillPermission";


export type SkillExecutionResult = {
  success: boolean;
  message: string;
  skillId: string;
};


export async function executeSkill(
  skill: SkillManifest,
  input: unknown,
): Promise<SkillExecutionResult> {

  const permission =
    checkSkillPermission(skill);


  if (permission !== "allowed") {
    return {
      success: false,
      message:
        `Skill ${skill.name} requires permission approval.`,
      skillId: skill.id,
    };
  }


  switch (skill.executor.type) {

    case "openclaw":
      return executeOpenClawSkill(
        skill,
        input,
      );


    case "mcp":
      return executeMcpSkill(
        skill,
        input,
      );


    case "local":
      return executeLocalSkill(
        skill,
        input,
      );


    case "remote":
      return executeRemoteSkill(
        skill,
        input,
      );


    default:
      return {
        success: false,
        message:
          `Unsupported executor type.`,
        skillId: skill.id,
      };
  }
}


async function executeOpenClawSkill(
  skill: SkillManifest,
  _input: unknown,
): Promise<SkillExecutionResult> {

  return {
    success: false,
    message:
      `OpenClaw execution adapter pending: ${skill.executor.handler}`,
    skillId: skill.id,
  };
}


async function executeMcpSkill(
  skill: SkillManifest,
  _input: unknown,
): Promise<SkillExecutionResult> {

  return {
    success: false,
    message:
      `MCP execution adapter pending: ${skill.executor.handler}`,
    skillId: skill.id,
  };
}


async function executeLocalSkill(
  skill: SkillManifest,
  _input: unknown,
): Promise<SkillExecutionResult> {

  return {
    success: false,
    message:
      `Local execution adapter pending: ${skill.executor.handler}`,
    skillId: skill.id,
  };
}


async function executeRemoteSkill(
  skill: SkillManifest,
  _input: unknown,
): Promise<SkillExecutionResult> {

  return {
    success: false,
    message:
      `Remote execution adapter pending: ${skill.executor.handler}`,
    skillId: skill.id,
  };
}
