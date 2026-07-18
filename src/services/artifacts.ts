import {
  recordAnalyticsEvent,
} from "./analytics";
import type {
  ArtifactKind,
  ArtifactLanguage,
  ArtifactProject,
  ArtifactRecord,
  ArtifactSource,
} from "../types/artifact";

const ARTIFACTS_STORAGE_KEY =
  "ai-os.artifacts.v2";

const PROJECTS_STORAGE_KEY =
  "ai-os.artifact-projects.v2";

const LEGACY_STORAGE_KEY =
  "ai-os.artifacts.v1";

const EXTENSIONS:
  Record<ArtifactLanguage, string> = {
  html: "html",
  css: "css",
  javascript: "js",
  typescript: "ts",
  python: "py",
  rust: "rs",
  sql: "sql",
  json: "json",
  svg: "svg",
  mermaid: "mmd",
  shell: "sh",
  markdown: "md",
  toml: "toml",
  yaml: "yml",
  code: "txt",
};

function uniqueStrings(
  values: unknown,
): string[] {
  if (!Array.isArray(values)) {
    return [];
  }

  return Array.from(
    new Set(
      values
        .filter(
          (item): item is string =>
            typeof item === "string",
        )
        .map((item) =>
          item.trim(),
        )
        .filter(Boolean),
    ),
  );
}

export function normalizeArtifactLanguage(
  value: string,
): ArtifactLanguage {
  const language =
    value
      .trim()
      .toLowerCase()
      .replace(/^language-/, "");

  const aliases:
    Record<string, ArtifactLanguage> = {
    js: "javascript",
    jsx: "javascript",
    ts: "typescript",
    tsx: "typescript",
    py: "python",
    rs: "rust",
    sh: "shell",
    bash: "shell",
    zsh: "shell",
    md: "markdown",
    htm: "html",
    xml: "svg",
    yml: "yaml",
  };

  const normalized =
    aliases[language] ??
    language;

  switch (normalized) {
    case "html":
    case "css":
    case "javascript":
    case "typescript":
    case "python":
    case "rust":
    case "sql":
    case "json":
    case "svg":
    case "mermaid":
    case "shell":
    case "markdown":
    case "toml":
    case "yaml":
      return normalized;
    default:
      return "code";
  }
}

export function artifactKindForLanguage(
  language: ArtifactLanguage,
): ArtifactKind {
  switch (language) {
    case "markdown":
      return "document";

    case "json":
      return "data";

    case "mermaid":
    case "svg":
      return "diagram";

    case "html":
    case "css":
    case "javascript":
    case "typescript":
      return "web";

    case "toml":
    case "yaml":
      return "config";

    default:
      return "code";
  }
}

export function extensionForLanguage(
  language: ArtifactLanguage,
): string {
  return EXTENSIONS[language];
}

function safeBaseName(
  value: string,
): string {
  return (
    value
      .trim()
      .replace(
        /[^a-zA-Z0-9\u4e00-\u9fff_-]+/g,
        "-",
      )
      .replace(
        /^-+|-+$/g,
        "",
      )
      .slice(0, 64) ||
    "artifact"
  );
}

function filenameFromContent(
  content: string,
): string | null {
  const lines =
    content
      .split("\n")
      .slice(0, 6);

  for (const line of lines) {
    const match =
      line.match(
        /^\s*(?:\/\/|#|<!--|\/\*)?\s*(?:filename|file|path|title)\s*[:=]\s*([^\s*<>]+).*$/i,
      );

    if (match?.[1]) {
      return match[1]
        .replace(
          /^["']|["']$/g,
          "",
        )
        .trim();
    }
  }

  return null;
}

export function createArtifactFilename(
  language: ArtifactLanguage,
  content: string,
  title?: string,
): string {
  const detected =
    filenameFromContent(
      content,
    );

  if (detected) {
    return detected;
  }

  if (
    language === "json" &&
    /"name"\s*:\s*"[^"]+"/.test(
      content,
    ) &&
    /"scripts"\s*:/.test(
      content,
    )
  ) {
    return "package.json";
  }

  if (
    language === "toml" &&
    /\[package\]/.test(content)
  ) {
    return "Cargo.toml";
  }

  if (
    language === "markdown" &&
    /^#\s+readme/im.test(
      content,
    )
  ) {
    return "README.md";
  }

  if (
    language === "html" &&
    /<!doctype html>/i.test(
      content,
    )
  ) {
    return "index.html";
  }

  return `${safeBaseName(
    title ||
      language,
  )}.${extensionForLanguage(
    language,
  )}`;
}

function normalizeSource(
  value: unknown,
): ArtifactSource {
  switch (value) {
    case "Compare":
    case "Router":
    case "Council":
    case "MultiLLM":
    case "Manual":
    case "Imported":
      return value;
    default:
      return "MultiLLM";
  }
}

function normalizeProject(
  value:
    Partial<ArtifactProject>,
): ArtifactProject {
  const createdAt =
    typeof value.createdAt ===
    "number"
      ? value.createdAt
      : Date.now();

  return {
    id:
      typeof value.id === "string" &&
      value.id
        ? value.id
        : crypto.randomUUID(),

    title:
      typeof value.title ===
        "string" &&
      value.title.trim()
        ? value.title.trim()
        : "Untitled Project",

    description:
      typeof value.description ===
      "string"
        ? value.description
        : "",

    favorite:
      value.favorite ??
      false,

    tags:
      uniqueStrings(
        value.tags,
      ),

    createdAt,

    updatedAt:
      typeof value.updatedAt ===
      "number"
        ? value.updatedAt
        : createdAt,
  };
}

function normalizeArtifact(
  value:
    Partial<ArtifactRecord>,
  fallbackProjectId:
    string,
): ArtifactRecord {
  const createdAt =
    typeof value.createdAt ===
    "number"
      ? value.createdAt
      : Date.now();

  const language =
    normalizeArtifactLanguage(
      typeof value.language ===
      "string"
        ? value.language
        : "code",
    );

  const content =
    typeof value.content ===
    "string"
      ? value.content
      : "";

  const title =
    typeof value.title ===
      "string" &&
    value.title.trim()
      ? value.title.trim()
      : `${language} artifact`;

  const filename =
    typeof value.filename ===
      "string" &&
    value.filename.trim()
      ? value.filename.trim()
      : createArtifactFilename(
          language,
          content,
          title,
        );

  const path =
    typeof value.path ===
      "string" &&
    value.path.trim()
      ? value.path.trim()
      : filename;

  return {
    id:
      typeof value.id ===
        "string" &&
      value.id
        ? value.id
        : crypto.randomUUID(),

    projectId:
      typeof value.projectId ===
        "string" &&
      value.projectId
        ? value.projectId
        : fallbackProjectId,

    title,
    filename,
    path,

    language,
    kind:
      value.kind ??
      artifactKindForLanguage(
        language,
      ),

    content,

    source:
      normalizeSource(
        value.source,
      ),

    provider:
      typeof value.provider ===
      "string"
        ? value.provider
        : undefined,

    tags:
      uniqueStrings(
        value.tags,
      ),

    favorite:
      value.favorite ??
      false,

    createdAt,

    updatedAt:
      typeof value.updatedAt ===
      "number"
        ? value.updatedAt
        : createdAt,
  };
}

export function saveArtifactProjects(
  projects:
    ArtifactProject[],
): void {
  localStorage.setItem(
    PROJECTS_STORAGE_KEY,
    JSON.stringify(
      projects,
    ),
  );
}

export function loadArtifactProjects():
  ArtifactProject[] {
  try {
    const raw =
      localStorage.getItem(
        PROJECTS_STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed.map(
      (item) =>
        normalizeProject(
          item as
            Partial<ArtifactProject>,
        ),
    );
  } catch {
    return [];
  }
}

export function saveArtifacts(
  artifacts:
    ArtifactRecord[],
): void {
  localStorage.setItem(
    ARTIFACTS_STORAGE_KEY,
    JSON.stringify(
      artifacts,
    ),
  );
}

function migrateLegacyArtifacts():
  ArtifactRecord[] {
  try {
    const raw =
      localStorage.getItem(
        LEGACY_STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (
      !Array.isArray(parsed) ||
      parsed.length === 0
    ) {
      return [];
    }

    const project:
      ArtifactProject =
      normalizeProject({
        title:
          "Migrated Artifacts",
        description:
          "Artifacts migrated automatically from Artifacts v1.",
      });

    const migrated =
      parsed.map(
        (
          item,
        ) =>
          normalizeArtifact(
            item as
              Partial<ArtifactRecord>,
            project.id,
          ),
      );

    saveArtifactProjects([
      project,
    ]);
    saveArtifacts(
      migrated,
    );

    return migrated;
  } catch {
    return [];
  }
}

export function loadArtifacts():
  ArtifactRecord[] {
  try {
    const raw =
      localStorage.getItem(
        ARTIFACTS_STORAGE_KEY,
      );

    if (!raw) {
      return migrateLegacyArtifacts();
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return [];
    }

    const defaultProjectId =
      loadArtifactProjects()[0]
        ?.id ??
      crypto.randomUUID();

    const normalized =
      parsed.map(
        (item) =>
          normalizeArtifact(
            item as
              Partial<ArtifactRecord>,
            defaultProjectId,
          ),
      );

    saveArtifacts(
      normalized,
    );

    return normalized;
  } catch {
    return [];
  }
}

export function createArtifactProject(
  title: string,
  description = "",
): ArtifactProject {
  const timestamp =
    Date.now();

  const project:
    ArtifactProject = {
    id: crypto.randomUUID(),
    title:
      title.trim() ||
      "Untitled Project",
    description:
      description.trim(),
    favorite: false,
    tags: [],
    createdAt: timestamp,
    updatedAt: timestamp,
  };

  saveArtifactProjects([
    project,
    ...loadArtifactProjects(),
  ]);

  recordAnalyticsEvent({
    module: "artifact",
    type: "created",
    title: "Artifact project created",
    description:
      project.title,
    metadata: {
      projectId:
        project.id,
    },
  });

  return project;
}

function ensureDefaultProject():
  ArtifactProject {
  const projects =
    loadArtifactProjects();

  if (projects[0]) {
    return projects[0];
  }

  return createArtifactProject(
    "Saved Artifacts",
    "Code and documents saved from AI responses.",
  );
}

export function createArtifact(
  input: {
    title?: string;
    filename?: string;
    path?: string;
    projectId?: string;

    language: string;
    content: string;

    source?: ArtifactSource;
    provider?: string;
    tags?: string[];
  },
): ArtifactRecord {
  const current =
    loadArtifacts();

  const timestamp =
    Date.now();

  const language =
    normalizeArtifactLanguage(
      input.language,
    );

  const projectId =
    input.projectId ||
    ensureDefaultProject().id;

  const filename =
    input.filename?.trim() ||
    createArtifactFilename(
      language,
      input.content,
      input.title,
    );

  const artifact:
    ArtifactRecord = {
    id: crypto.randomUUID(),
    projectId,

    title:
      input.title?.trim() ||
      filename,

    filename,

    path:
      input.path?.trim() ||
      filename,

    language,

    kind:
      artifactKindForLanguage(
        language,
      ),

    content:
      input.content,

    source:
      input.source ??
      "MultiLLM",

    provider:
      input.provider,

    tags:
      uniqueStrings(
        input.tags,
      ),

    favorite: false,

    createdAt:
      timestamp,

    updatedAt:
      timestamp,
  };

  saveArtifacts([
    artifact,
    ...current,
  ]);

  window.dispatchEvent(
    new CustomEvent(
      "ai-os:artifact-created",
      {
        detail:
          artifact,
      },
    ),
  );

  recordAnalyticsEvent({
    module: "artifact",
    type: "created",
    title: "Artifact saved",
    description:
      artifact.path,
    provider:
      artifact.provider,
    metadata: {
      artifactId:
        artifact.id,
      projectId:
        artifact.projectId,
      language:
        artifact.language,
      source:
        artifact.source,
    },
  });

  return artifact;
}

export function updateArtifact(
  id: string,
  updater: (
    artifact:
      ArtifactRecord,
  ) => ArtifactRecord,
): ArtifactRecord[] {
  const next =
    loadArtifacts().map(
      (artifact) =>
        artifact.id === id
          ? {
              ...updater(
                artifact,
              ),
              updatedAt:
                Date.now(),
            }
          : artifact,
    );

  saveArtifacts(next);
  return next;
}

export function updateArtifactProject(
  id: string,
  updater: (
    project:
      ArtifactProject,
  ) => ArtifactProject,
): ArtifactProject[] {
  const next =
    loadArtifactProjects().map(
      (project) =>
        project.id === id
          ? {
              ...updater(
                project,
              ),
              updatedAt:
                Date.now(),
            }
          : project,
    );

  saveArtifactProjects(next);
  return next;
}

export function deleteArtifact(
  id: string,
): ArtifactRecord[] {
  const next =
    loadArtifacts().filter(
      (artifact) =>
        artifact.id !== id,
    );

  saveArtifacts(next);
  return next;
}

export function deleteArtifactProject(
  id: string,
): {
  projects:
    ArtifactProject[];
  artifacts:
    ArtifactRecord[];
} {
  const projects =
    loadArtifactProjects().filter(
      (project) =>
        project.id !== id,
    );

  const artifacts =
    loadArtifacts().filter(
      (artifact) =>
        artifact.projectId !==
        id,
    );

  saveArtifactProjects(
    projects,
  );
  saveArtifacts(
    artifacts,
  );

  return {
    projects,
    artifacts,
  };
}

export function replaceArtifactWorkspace(
  projects: ArtifactProject[],
  artifacts: ArtifactRecord[],
): void {
  saveArtifactProjects(projects);
  saveArtifacts(artifacts);

  window.dispatchEvent(
    new CustomEvent(
      "ai-os:artifact-created",
    ),
  );
}

export function moveArtifacts(
  artifactIds: string[],
  projectId: string,
): ArtifactRecord[] {
  const selected =
    new Set(artifactIds);

  const next =
    loadArtifacts().map(
      (artifact) =>
        selected.has(artifact.id)
          ? {
              ...artifact,
              projectId,
              updatedAt: Date.now(),
            }
          : artifact,
    );

  saveArtifacts(next);
  return next;
}

export function deleteArtifacts(
  artifactIds: string[],
): ArtifactRecord[] {
  const selected =
    new Set(artifactIds);

  const next =
    loadArtifacts().filter(
      (artifact) =>
        !selected.has(
          artifact.id,
        ),
    );

  saveArtifacts(next);
  return next;
}

export function addTagsToArtifacts(
  artifactIds: string[],
  tags: string[],
): ArtifactRecord[] {
  const selected =
    new Set(artifactIds);

  const normalizedTags =
    tags
      .map((tag) =>
        tag.trim(),
      )
      .filter(Boolean);

  const next =
    loadArtifacts().map(
      (artifact) =>
        selected.has(artifact.id)
          ? {
              ...artifact,
              tags: Array.from(
                new Set([
                  ...artifact.tags,
                  ...normalizedTags,
                ]),
              ),
              updatedAt:
                Date.now(),
            }
          : artifact,
    );

  saveArtifacts(next);
  return next;
}
