import { invoke } from "@tauri-apps/api/core";
import { estimateContextTokens } from "./conversations";

export type KnowledgeProject = {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
};

export type KnowledgeSource = {
  id: string;
  projectId: string;
  name: string;
  path: string;
  kind: "pdf" | "word" | "presentation" | "spreadsheet" | "text";
  updatedAt: string;
};

export type KnowledgeSearchHit = {
  sourceId: string;
  sourceName: string;
  sourcePath: string;
  kind: string;
  locator: string;
  excerpt: string;
  score: number;
};

export async function listKnowledgeProjects(): Promise<KnowledgeProject[]> {
  return invoke<KnowledgeProject[]>("list_knowledge_projects");
}

export async function createKnowledgeProject(name: string): Promise<KnowledgeProject> {
  const now = new Date().toISOString();
  const project: KnowledgeProject = {
    id: crypto.randomUUID(),
    name: name.trim().slice(0, 120),
    createdAt: now,
    updatedAt: now,
  };
  if (!project.name) throw new Error("Project name is required.");
  await invoke("save_knowledge_project", { project });
  return project;
}

export async function listKnowledgeSources(projectId: string): Promise<KnowledgeSource[]> {
  return invoke<KnowledgeSource[]>("list_knowledge_sources", { projectId });
}

export async function indexKnowledgeSource(
  projectId: string,
  path: string,
): Promise<KnowledgeSource> {
  return invoke<KnowledgeSource>("index_knowledge_source", { projectId, path });
}

export async function removeKnowledgeSource(projectId: string, id: string): Promise<void> {
  await invoke("remove_knowledge_source", { projectId, id });
}

export async function searchKnowledge(
  projectId: string,
  query: string,
  limit = 6,
): Promise<KnowledgeSearchHit[]> {
  return invoke<KnowledgeSearchHit[]>("search_knowledge", { projectId, query, limit });
}

export function buildKnowledgeContext(
  project: KnowledgeProject,
  hits: KnowledgeSearchHit[],
  tokenBudget: number,
): { role: "system"; content: string } | undefined {
  if (hits.length === 0 || tokenBudget <= 0) return undefined;
  const lines = [
    `Project knowledge workspace: ${project.name} (${project.id}).`,
    "Use only the retrieved excerpts below as project-source evidence. Cite the bracketed source and locator when relying on it. If the excerpts are insufficient, say so.",
  ];
  for (const hit of hits) {
    const line = `- [source:${hit.sourceId}; ${hit.locator}; ${hit.sourceName}] ${hit.excerpt}`;
    if (estimateContextTokens([...lines, line].join("\n")) > tokenBudget) break;
    lines.push(line);
  }
  return lines.length > 2 ? { role: "system", content: lines.join("\n") } : undefined;
}
