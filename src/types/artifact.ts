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
  | "code";

export type ArtifactRecord = {
  id: string;
  title: string;
  language: ArtifactLanguage;
  content: string;
  source: string;
  favorite: boolean;
  createdAt: number;
  updatedAt: number;
};
