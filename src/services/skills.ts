import { invoke } from "@tauri-apps/api/core";

import type {
  SkillManifest,
} from "../types/skill";

export async function listSkills(): Promise<
  SkillManifest[]
> {
  return invoke<SkillManifest[]>(
    "list_skills",
  );
}

export async function getSkill(
  skillId: string,
): Promise<SkillManifest> {
  return invoke<SkillManifest>(
    "get_skill",
    {
      skillId,
    },
  );
}
