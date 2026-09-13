import { invoke } from "@tauri-apps/api/core";
import type {
  PersonProfileView,
  ProfileSummary,
  ReviewDecisions,
  RunnablePersonaSkill,
} from "../types/personProfile";

export async function listPersonProfiles(): Promise<ProfileSummary[]> {
  return invoke<ProfileSummary[]>("list_person_profiles");
}

export async function getPersonProfile(profileId: string): Promise<PersonProfileView> {
  return invoke<PersonProfileView>("get_person_profile", { profileId });
}

export async function reviewPersonProfile(
  profileId: string,
  decisions: ReviewDecisions,
): Promise<PersonProfileView> {
  return invoke<PersonProfileView>("review_person_profile", { profileId, decisions });
}

/**
 * Takes no "reviewed" flag on purpose: the backend requires the profile to
 * already be in `reviewed`, a state only a completed review can produce.
 */
export async function activatePersonProfile(profileId: string): Promise<PersonProfileView> {
  return invoke<PersonProfileView>("activate_person_profile", { profileId });
}

export async function rollbackPersonProfile(
  profileId: string,
  revision: number,
): Promise<PersonProfileView> {
  return invoke<PersonProfileView>("rollback_person_profile", { profileId, revision });
}

export async function buildPersonPersonaSkill(
  profileId: string,
): Promise<RunnablePersonaSkill> {
  return invoke<RunnablePersonaSkill>("build_person_persona_skill", { profileId });
}
