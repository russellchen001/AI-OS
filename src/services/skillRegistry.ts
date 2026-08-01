import type {
  SkillCapability,
  SkillManifest,
  SkillPermission,
} from "../types/skill";


const registry = new Map<string, SkillManifest>();


export function registerSkill(
  skill: SkillManifest,
): SkillManifest {
  registry.set(skill.id, skill);
  return skill;
}


export function unregisterSkill(
  skillId: string,
): boolean {
  return registry.delete(skillId);
}


export function getSkill(
  skillId: string,
): SkillManifest | undefined {
  return registry.get(skillId);
}


export function listSkills(): SkillManifest[] {
  return Array.from(registry.values());
}


export function findSkillsByCapability(
  capability: SkillCapability,
): SkillManifest[] {
  return listSkills().filter((skill) =>
    skill.capabilities.includes(capability),
  );
}


export function findSkillsByPermission(
  permission: SkillPermission,
): SkillManifest[] {
  return listSkills().filter((skill) =>
    skill.permissions.includes(permission),
  );
}


export function clearSkillRegistry(): void {
  registry.clear();
}
