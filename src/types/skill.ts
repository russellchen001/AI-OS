export type SkillId =
  | "filesystem"
  | "email"
  | "browser"
  | "calendar"
  | "nas"
  | (string & {});


export type SkillCapability =
  | "filesystem.read"
  | "filesystem.write"
  | "email.read"
  | "email.send"
  | "browser.search"
  | "browser.control"
  | "calendar.read"
  | "calendar.write"
  | "nas.manage"
  | (string & {});


export type SkillPermission =
  | "filesystem.read"
  | "filesystem.write"
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
  | "remote";


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

  createdAt: string;
  updatedAt: string;
};


export type SkillRegistryEntry = SkillManifest;
