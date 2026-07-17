export type ArtifactLanguage =
  | "html"
  | "css"
  | "javascript"
  | "typescript"
  | "python"
  | "rust"
  | "sql"
  | "json"
  | "svg"
  | "mermaid"
  | "shell"
  | "markdown"
  | "toml"
  | "yaml"
  | "code";

export type ArtifactSource =
  | "Compare"
  | "Router"
  | "Council"
  | "MultiLLM"
  | "Manual"
  | "Imported";

export type ArtifactKind =
  | "code"
  | "document"
  | "data"
  | "diagram"
  | "web"
  | "config";

export type ArtifactRecord = {
  id: string;
  projectId: string;

  title: string;
  filename: string;
  path: string;

  language: ArtifactLanguage;
  kind: ArtifactKind;
  content: string;

  source: ArtifactSource;
  provider?: string;

  tags: string[];
  favorite: boolean;

  createdAt: number;
  updatedAt: number;
};

export type ArtifactProject = {
  id: string;
  title: string;
  description: string;

  favorite: boolean;
  tags: string[];

  createdAt: number;
  updatedAt: number;
};

export type ArtifactWorkspaceExport = {
  schemaVersion: 2;
  exportedAt: string;
  projects: ArtifactProject[];
  artifacts: ArtifactRecord[];
};
