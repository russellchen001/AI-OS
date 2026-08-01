import type {
  SkillManifest,
  SkillCapability,
} from "../types/skill";

import {
  findSkillsByCapability,
} from "./skillRegistry";


export type SkillResolutionResult = {
  found: boolean;
  skill?: SkillManifest;
  message: string;
};


export function resolveSkill(
  capability: SkillCapability,
): SkillResolutionResult {

  const skills =
    findSkillsByCapability(capability);


  if (skills.length === 0) {
    return {
      found: false,
      message:
        `No skill supports capability: ${capability}`,
    };
  }


  return {
    found: true,
    skill: skills[0],
    message:
      `Resolved skill: ${skills[0].name}`,
  };
}
