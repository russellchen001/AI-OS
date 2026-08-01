import type { SkillManifest } from "../types/skill";
import { registerSkill } from "./skillRegistry";


const filesystemSkill: SkillManifest = {
  id: "filesystem",
  name: "Filesystem",
  category: "system",
  description:
    "Read, write and manage files through approved execution paths.",
  version: "1.0.0",

  capabilities: [
    "filesystem.read",
    "filesystem.write",
  ],

  permissions: [
    "filesystem.read",
    "filesystem.write",
  ],

  executor: {
    type: "openclaw",
    handler: "filesystem",
  },

  enabled: true,
  builtIn: true,

  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
};


const browserSkill: SkillManifest = {
  id: "browser",
  name: "Browser",
  category: "browser",
  description:
    "Search and interact with web pages through approved browser tools.",
  version: "1.0.0",

  capabilities: [
    "browser.search",
    "browser.control",
  ],

  permissions: [
    "network.access",
    "browser.control",
  ],

  executor: {
    type: "mcp",
    handler: "browser",
  },

  enabled: true,
  builtIn: true,

  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
};


export function registerBuiltInSkills(): void {
  registerSkill(filesystemSkill);
  registerSkill(browserSkill);
}


export const BUILT_IN_SKILLS = [
  filesystemSkill,
  browserSkill,
];
