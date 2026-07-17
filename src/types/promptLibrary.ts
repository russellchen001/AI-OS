export type PromptCategory =
  | "writing"
  | "coding"
  | "analysis"
  | "translation"
  | "general";

export type PromptTemplate = {
  id: string;
  title: string;
  description: string;
  content: string;
  category: PromptCategory;
  tags: string[];
  favorite: boolean;
  builtIn: boolean;
  createdAt: number;
  updatedAt: number;
};

export type PromptLibraryExport = {
  schemaVersion: 1;
  exportedAt: string;
  prompts: PromptTemplate[];
};
