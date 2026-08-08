import type {
  ProviderId,
} from "./provider";

export type CouncilRole =
  | "planner"
  | "engineer"
  | "researcher"
  | "critic"
  | "judge";

export type CouncilMember = {
  id: CouncilRole;
  name: string;
  icon: string;
  providerId: ProviderId;
  enabled: boolean;
  systemPrompt: string;
};

export type CouncilStepStatus =
  | "idle"
  | "running"
  | "done"
  | "error"
  | "skipped";

export type CouncilStepResult = {
  role: CouncilRole;
  memberName: string;
  providerId: ProviderId;
  status: CouncilStepStatus;
  output: string;
  error?: string;
  startedAt?: number;
  completedAt?: number;
};

export type CouncilSession = {
  id: string;
  title: string;
  prompt: string;
  createdAt: number;
  updatedAt: number;
  favorite: boolean;
  steps: CouncilStepResult[];
  finalAnswer: string;
};
