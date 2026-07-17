import type {
  PromptCategory,
  PromptLibraryExport,
  PromptTemplate,
} from "../types/promptLibrary";

const STORAGE_KEY =
  "ai-os.prompt-library.v1";

const now = Date.now();

const DEFAULT_PROMPTS:
  PromptTemplate[] = [
  {
    id: "builtin-code-review",
    title: "Code Review",
    description:
      "Review code for bugs, security issues, readability and performance.",
    content: `Review the following code carefully.

Focus on:
1. Correctness and possible bugs
2. Security risks
3. Performance
4. Readability and maintainability
5. Concrete improvements

Return:
- Summary
- Issues by severity
- Improved code where useful

Code:
`,
    category: "coding",
    tags: [
      "review",
      "debug",
      "quality",
    ],
    favorite: true,
    builtIn: true,
    createdAt: now,
    updatedAt: now,
  },
  {
    id: "builtin-debug",
    title: "Debug an Error",
    description:
      "Diagnose an error and provide a safe step-by-step fix.",
    content: `Act as a senior software engineer.

Diagnose the following error:
- Explain the root cause
- Identify the most likely failing code
- Provide a minimal fix
- Provide verification steps
- Mention possible regressions

Error and context:
`,
    category: "coding",
    tags: [
      "debug",
      "error",
    ],
    favorite: false,
    builtIn: true,
    createdAt: now + 1,
    updatedAt: now + 1,
  },
  {
    id: "builtin-writing",
    title: "Professional Rewrite",
    description:
      "Rewrite text so it is clear, concise and professional.",
    content: `Rewrite the following text.

Requirements:
- Preserve the original meaning
- Improve clarity and structure
- Use a natural professional tone
- Remove repetition
- Do not invent facts

Text:
`,
    category: "writing",
    tags: [
      "rewrite",
      "professional",
    ],
    favorite: false,
    builtIn: true,
    createdAt: now + 2,
    updatedAt: now + 2,
  },
  {
    id: "builtin-analysis",
    title: "Deep Analysis",
    description:
      "Analyse a topic from multiple perspectives and reach a balanced conclusion.",
    content: `Analyse the following topic in depth.

Include:
1. Key facts and assumptions
2. Arguments for and against
3. Risks and trade-offs
4. Important unknowns
5. Practical recommendations
6. Final balanced conclusion

Topic:
`,
    category: "analysis",
    tags: [
      "analysis",
      "decision",
    ],
    favorite: false,
    builtIn: true,
    createdAt: now + 3,
    updatedAt: now + 3,
  },
  {
    id: "builtin-translation",
    title: "Accurate Translation",
    description:
      "Translate text naturally while preserving tone and formatting.",
    content: `Translate the following text.

Requirements:
- Preserve meaning and tone
- Preserve Markdown and formatting
- Keep names and technical terms accurate
- Do not add explanations unless needed
- Target language: [enter language]

Text:
`,
    category: "translation",
    tags: [
      "translation",
      "language",
    ],
    favorite: false,
    builtIn: true,
    createdAt: now + 4,
    updatedAt: now + 4,
  },
  {
    id: "builtin-summary",
    title: "Structured Summary",
    description:
      "Turn long content into a structured and actionable summary.",
    content: `Summarize the following content.

Return:
- One-paragraph overview
- Key points
- Important facts or numbers
- Decisions or conclusions
- Action items
- Open questions

Content:
`,
    category: "general",
    tags: [
      "summary",
      "notes",
    ],
    favorite: false,
    builtIn: true,
    createdAt: now + 5,
    updatedAt: now + 5,
  },
];

function normalizeCategory(
  value: unknown,
): PromptCategory {
  return value === "writing" ||
    value === "coding" ||
    value === "analysis" ||
    value === "translation" ||
    value === "general"
    ? value
    : "general";
}

function normalizePrompt(
  value: Partial<PromptTemplate>,
): PromptTemplate {
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
      typeof value.title === "string" &&
      value.title.trim()
        ? value.title.trim()
        : "Untitled Prompt",
    description:
      typeof value.description ===
      "string"
        ? value.description
        : "",
    content:
      typeof value.content ===
      "string"
        ? value.content
        : "",
    category:
      normalizeCategory(
        value.category,
      ),
    tags: Array.isArray(value.tags)
      ? Array.from(
          new Set(
            value.tags
              .filter(
                (
                  tag,
                ): tag is string =>
                  typeof tag ===
                  "string",
              )
              .map((tag) =>
                tag.trim(),
              )
              .filter(Boolean),
          ),
        )
      : [],
    favorite:
      value.favorite ?? false,
    builtIn:
      value.builtIn ?? false,
    createdAt,
    updatedAt:
      typeof value.updatedAt ===
      "number"
        ? value.updatedAt
        : createdAt,
  };
}

export function loadPrompts():
  PromptTemplate[] {
  try {
    const raw =
      localStorage.getItem(
        STORAGE_KEY,
      );

    if (!raw) {
      savePrompts(
        DEFAULT_PROMPTS,
      );
      return DEFAULT_PROMPTS;
    }

    const parsed: unknown =
      JSON.parse(raw);

    if (!Array.isArray(parsed)) {
      return DEFAULT_PROMPTS;
    }

    const normalized =
      parsed.map((item) =>
        normalizePrompt(
          item as Partial<PromptTemplate>,
        ),
      );

    savePrompts(normalized);
    return normalized;
  } catch {
    return DEFAULT_PROMPTS;
  }
}

export function savePrompts(
  prompts: PromptTemplate[],
): void {
  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify(prompts),
  );
}

export function createPrompt(
  input: Omit<
    PromptTemplate,
    | "id"
    | "createdAt"
    | "updatedAt"
    | "builtIn"
  >,
): PromptTemplate[] {
  const current =
    loadPrompts();

  const timestamp =
    Date.now();

  const prompt:
    PromptTemplate = {
    ...input,
    id: crypto.randomUUID(),
    builtIn: false,
    createdAt: timestamp,
    updatedAt: timestamp,
  };

  const next = [
    prompt,
    ...current,
  ];

  savePrompts(next);
  return next;
}

export function updatePrompt(
  id: string,
  updater: (
    prompt: PromptTemplate,
  ) => PromptTemplate,
): PromptTemplate[] {
  const next =
    loadPrompts().map(
      (prompt) =>
        prompt.id === id
          ? {
              ...updater(prompt),
              updatedAt:
                Date.now(),
            }
          : prompt,
    );

  savePrompts(next);
  return next;
}

export function deletePrompt(
  id: string,
): PromptTemplate[] {
  const next =
    loadPrompts().filter(
      (prompt) =>
        prompt.id !== id,
    );

  savePrompts(next);
  return next;
}

export function resetDefaultPrompts():
  PromptTemplate[] {
  savePrompts(
    DEFAULT_PROMPTS,
  );

  return DEFAULT_PROMPTS;
}

export function parsePromptImport(
  content: string,
): PromptTemplate[] {
  const parsed: unknown =
    JSON.parse(content);

  const rawPrompts =
    Array.isArray(parsed)
      ? parsed
      : (
          parsed as
            Partial<PromptLibraryExport>
        ).prompts;

  if (!Array.isArray(rawPrompts)) {
    throw new Error(
      "Invalid Prompt Library JSON.",
    );
  }

  return rawPrompts.map(
    (item) =>
      normalizePrompt(
        item as Partial<PromptTemplate>,
      ),
  );
}
