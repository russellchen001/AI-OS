import type {
  ArtifactLanguage,
  ArtifactRecord,
} from "../types/artifact";

const STORAGE_KEY =
  "ai-os.artifacts.v1";

export function normalizeArtifactLanguage(
  value: string,
): ArtifactLanguage {
  const language =
    value
      .trim()
      .toLowerCase();

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
    md: "markdown",
    htm: "html",
    xml: "svg",
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
      return normalized;
    default:
      return "code";
  }
}

function normalizeArtifact(
  value: Partial<ArtifactRecord>,
): ArtifactRecord {
  const createdAt =
    typeof value.createdAt === "number"
      ? value.createdAt
      : Date.now();

  return {
    id:
      typeof value.id === "string" &&
      value.id
        ? value.id
        : crypto.randomUUID(),
    title:
      typeof value.title === "string" &&
      value.title.trim()
        ? value.title.trim()
        : "Untitled Artifact",
    language:
      normalizeArtifactLanguage(
        typeof value.language === "string"
          ? value.language
          : "code",
      ),
    content:
      typeof value.content === "string"
        ? value.content
        : "",
    source:
      typeof value.source === "string"
        ? value.source
        : "MultiLLM",
    favorite:
      value.favorite ?? false,
    createdAt,
    updatedAt:
      typeof value.updatedAt === "number"
        ? value.updatedAt
        : createdAt,
  };
}

export function loadArtifacts():
  ArtifactRecord[] {
  try {
    const raw =
      localStorage.getItem(
        STORAGE_KEY,
      );

    if (!raw) {
      return [];
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return [];
    }

    const artifacts =
      parsed.map((item) =>
        normalizeArtifact(
          item as Partial<ArtifactRecord>,
        ),
      );

    saveArtifacts(artifacts);
    return artifacts;
  } catch {
    return [];
  }
}

export function saveArtifacts(
  artifacts: ArtifactRecord[],
): void {
  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify(artifacts),
  );
}

export function createArtifact(
  input: {
    title?: string;
    language: string;
    content: string;
    source?: string;
  },
): ArtifactRecord {
  const current =
    loadArtifacts();

  const timestamp =
    Date.now();

  const artifact:
    ArtifactRecord = {
    id: crypto.randomUUID(),
    title:
      input.title?.trim() ||
      `${
        normalizeArtifactLanguage(
          input.language,
        )
      } artifact`,
    language:
      normalizeArtifactLanguage(
        input.language,
      ),
    content:
      input.content,
    source:
      input.source ??
      "MultiLLM",
    favorite: false,
    createdAt: timestamp,
    updatedAt: timestamp,
  };

  saveArtifacts([
    artifact,
    ...current,
  ]);

  window.dispatchEvent(
    new CustomEvent(
      "ai-os:artifact-created",
      {
        detail: artifact,
      },
    ),
  );

  return artifact;
}

export function updateArtifact(
  id: string,
  updater: (
    artifact: ArtifactRecord,
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
