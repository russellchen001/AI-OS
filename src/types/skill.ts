export type SkillId =
  | "filesystem"
  | "openclaw-session"
  | "browser"
  | "email"
  | "calendar"
  | "nas"
  | (string & {});

export type SkillCapability =
  | "filesystem.read"
  | "filesystem.write"
  | "filesystem.scan"
  | "filesystem.move"
  | "sessions.create"
  | "ai.openclaw.gateway"
  | "browser.search"
  | "browser.control"
  | "email.read"
  | "email.send"
  | "calendar.read"
  | "calendar.write"
  | "nas.manage"
  | (string & {});

export type SkillPermission =
  | "filesystem.read"
  | "filesystem.write"
  | "sessions.create"
  | "network.access"
  | "browser.control"
  | "email.access"
  | "calendar.access"
  | "device.control"
  | (string & {});

export type SkillExecutorType =
  | "openclaw"
  | "mcp"
  | "local"
  | "remote"
  | (string & {});

export type SkillExecutor = {
  type: SkillExecutorType;
  handler: string;
};

export type SkillCategory =
  | "system"
  | "communication"
  | "productivity"
  | "browser"
  | "storage"
  | "device"
  | (string & {});

export type SkillManifest = {
  id: SkillId;
  name: string;
  category: SkillCategory;
  description: string;
  version: string;
  capabilities: SkillCapability[];
  permissions: SkillPermission[];
  executor: SkillExecutor;
  enabled: boolean;
  builtIn: boolean;
};

export type SkillRegistryEntry = SkillManifest;
