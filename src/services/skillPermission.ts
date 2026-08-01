import type {
  SkillManifest,
  SkillPermission,
} from "../types/skill";


export type PermissionDecision =
  | "allowed"
  | "denied"
  | "requires-approval";


const approvedPermissions = new Set<SkillPermission>();


export function approveSkillPermission(
  permission: SkillPermission,
): void {
  approvedPermissions.add(permission);
}


export function revokeSkillPermission(
  permission: SkillPermission,
): void {
  approvedPermissions.delete(permission);
}


export function checkSkillPermission(
  skill: SkillManifest,
): PermissionDecision {

  const missing = skill.permissions.some(
    (permission) =>
      !approvedPermissions.has(permission),
  );


  if (missing) {
    return "requires-approval";
  }


  return "allowed";
}


export function getRequiredPermissions(
  skill: SkillManifest,
): SkillPermission[] {

  return [
    ...skill.permissions,
  ];
}
