import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import {
  open,
  save,
} from "@tauri-apps/plugin-dialog";
import {
  readTextFile,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import {
  createPrompt,
  deletePrompt,
  loadPrompts,
  parsePromptImport,
  resetDefaultPrompts,
  savePrompts,
  updatePrompt,
} from "../services/promptLibrary";
import type {
  PromptCategory,
  PromptLibraryExport,
  PromptTemplate,
} from "../types/promptLibrary";

import {
  useDialog,
} from "../components/DialogProvider";
type PromptLibraryPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
  onUsePrompt: (
    content: string,
    target:
      | "compare"
      | "router",
  ) => void;
};

type PromptDraft = {
  title: string;
  description: string;
  content: string;
  category: PromptCategory;
  tags: string;
};

const EMPTY_DRAFT:
  PromptDraft = {
  title: "",
  description: "",
  content: "",
  category: "general",
  tags: "",
};

const CATEGORY_LABELS:
  Record<
    PromptCategory,
    string
  > = {
  writing: "Writing",
  coding: "Coding",
  analysis: "Analysis",
  translation:
    "Translation",
  general: "General",
};

function PromptLibraryPage({
  cardStyle,
  onMessage,
  onUsePrompt,
}: PromptLibraryPageProps) {
  const dialog =
    useDialog();

  const [
    prompts,
    setPrompts,
  ] = useState<
    PromptTemplate[]
  >(loadPrompts);

  const [
    searchText,
    setSearchText,
  ] = useState("");

  const [
    categoryFilter,
    setCategoryFilter,
  ] = useState<
    PromptCategory | "all"
  >("all");

  const [
    favoritesOnly,
    setFavoritesOnly,
  ] = useState(false);

  const [
    selectedPromptId,
    setSelectedPromptId,
  ] = useState<
    string | null
  >(null);

  const [
    editingPromptId,
    setEditingPromptId,
  ] = useState<
    string | null
  >(null);

  const [
    draft,
    setDraft,
  ] = useState<
    PromptDraft
  >(EMPTY_DRAFT);

  const filteredPrompts =
    useMemo(() => {
      const query =
        searchText
          .trim()
          .toLowerCase();

      return [...prompts]
        .filter((prompt) => {
          if (
            categoryFilter !==
              "all" &&
            prompt.category !==
              categoryFilter
          ) {
            return false;
          }

          if (
            favoritesOnly &&
            !prompt.favorite
          ) {
            return false;
          }

          if (!query) {
            return true;
          }

          return (
            prompt.title
              .toLowerCase()
              .includes(query) ||
            prompt.description
              .toLowerCase()
              .includes(query) ||
            prompt.content
              .toLowerCase()
              .includes(query) ||
            prompt.tags.some(
              (tag) =>
                tag
                  .toLowerCase()
                  .includes(query),
            )
          );
        })
        .sort(
          (left, right) =>
            Number(
              right.favorite,
            ) -
              Number(
                left.favorite,
              ) ||
            right.updatedAt -
              left.updatedAt,
        );
    }, [
      categoryFilter,
      favoritesOnly,
      prompts,
      searchText,
    ]);

  const selectedPrompt =
    prompts.find(
      (prompt) =>
        prompt.id ===
        selectedPromptId,
    ) ??
    filteredPrompts[0] ??
    null;

  const startCreate =
    () => {
      setEditingPromptId(
        "new",
      );
      setSelectedPromptId(
        null,
      );
      setDraft(
        EMPTY_DRAFT,
      );
    };

  const startEdit =
    (
      prompt:
        PromptTemplate,
    ) => {
      setSelectedPromptId(
        prompt.id,
      );
      setEditingPromptId(
        prompt.id,
      );
      setDraft({
        title:
          prompt.title,
        description:
          prompt.description,
        content:
          prompt.content,
        category:
          prompt.category,
        tags:
          prompt.tags.join(
            ", ",
          ),
      });
    };

  const cancelEdit =
    () => {
      setEditingPromptId(
        null,
      );
      setDraft(
        EMPTY_DRAFT,
      );
    };

  const saveDraft =
    () => {
      const title =
        draft.title.trim();
      const content =
        draft.content.trim();

      if (
        !title ||
        !content
      ) {
        onMessage(
          "Unable to save prompt: title and content are required.",
        );
        return;
      }

      const tags =
        Array.from(
          new Set(
            draft.tags
              .split(",")
              .map((tag) =>
                tag.trim(),
              )
              .filter(Boolean),
          ),
        );

      if (
        editingPromptId ===
        "new"
      ) {
        const next =
          createPrompt({
            title,
            description:
              draft.description
                .trim(),
            content,
            category:
              draft.category,
            tags,
            favorite:
              false,
          });

        setPrompts(next);
        setSelectedPromptId(
          next[0]?.id ??
            null,
        );
        onMessage(
          "Prompt created successfully.",
        );
      } else if (
        editingPromptId
      ) {
        const next =
          updatePrompt(
            editingPromptId,
            (prompt) => ({
              ...prompt,
              title,
              description:
                draft.description
                  .trim(),
              content,
              category:
                draft.category,
              tags,
            }),
          );

        setPrompts(next);
        onMessage(
          "Prompt updated successfully.",
        );
      }

      cancelEdit();
    };

  const toggleFavorite =
    (
      prompt:
        PromptTemplate,
    ) => {
      const next =
        updatePrompt(
          prompt.id,
          (current) => ({
            ...current,
            favorite:
              !current.favorite,
          }),
        );

      setPrompts(next);
    };

  const duplicatePrompt =
    (
      prompt:
        PromptTemplate,
    ) => {
      const next =
        createPrompt({
          title:
            `${prompt.title} Copy`,
          description:
            prompt.description,
          content:
            prompt.content,
          category:
            prompt.category,
          tags:
            prompt.tags,
          favorite:
            false,
        });

      setPrompts(next);
      setSelectedPromptId(
        next[0]?.id ?? null,
      );
      onMessage(
        "Prompt duplicated successfully.",
      );
    };

  const removePrompt =
    (
      prompt:
        PromptTemplate,
    ) => {
      const confirmed =
        window.confirm(
          `Delete "${prompt.title}"?`,
        );

      if (!confirmed) {
        return;
      }

      const next =
        deletePrompt(
          prompt.id,
        );

      setPrompts(next);
      setSelectedPromptId(
        next[0]?.id ??
          null,
      );
      onMessage(
        "Prompt deleted successfully.",
      );
    };

  const exportJson =
    async () => {
      try {
        const filePath =
          await save({
            defaultPath:
              "ai-os-prompts.json",
            filters: [
              {
                name: "JSON",
                extensions: [
                  "json",
                ],
              },
            ],
          });

        if (!filePath) {
          return;
        }

        const document:
          PromptLibraryExport = {
          schemaVersion: 1,
          exportedAt:
            new Date()
              .toISOString(),
          prompts,
        };

        await writeTextFile(
          filePath,
          JSON.stringify(
            document,
            null,
            2,
          ),
        );

        onMessage(
          `Prompt Library exported to ${filePath}`,
        );
      } catch (error) {
        onMessage(
          `Prompt export failed: ${String(
            error,
          )}`,
        );
      }
    };

  const exportMarkdown =
    async () => {
      try {
        const filePath =
          await save({
            defaultPath:
              "ai-os-prompts.md",
            filters: [
              {
                name:
                  "Markdown",
                extensions: [
                  "md",
                ],
              },
            ],
          });

        if (!filePath) {
          return;
        }

        const markdown =
          prompts
            .map(
              (prompt) =>
                [
                  `# ${prompt.title}`,
                  "",
                  prompt.description,
                  "",
                  `- Category: ${CATEGORY_LABELS[prompt.category]}`,
                  `- Tags: ${prompt.tags.join(", ") || "None"}`,
                  `- Favorite: ${prompt.favorite ? "Yes" : "No"}`,
                  "",
                  "## Prompt",
                  "",
                  prompt.content,
                  "",
                  "---",
                  "",
                ].join(
                  "\n",
                ),
            )
            .join("\n");

        await writeTextFile(
          filePath,
          markdown,
        );

        onMessage(
          `Prompt Library exported to ${filePath}`,
        );
      } catch (error) {
        onMessage(
          `Prompt export failed: ${String(
            error,
          )}`,
        );
      }
    };

  const importJson =
    async () => {
      try {
        const filePath =
          await open({
            multiple: false,
            filters: [
              {
                name: "JSON",
                extensions: [
                  "json",
                ],
              },
            ],
          });

        if (
          !filePath ||
          Array.isArray(
            filePath,
          )
        ) {
          return;
        }

        const content =
          await readTextFile(
            filePath,
          );

        const imported =
          parsePromptImport(
            content,
          );

        const merged =
          [
            ...imported,
            ...prompts.filter(
              (current) =>
                !imported.some(
                  (incoming) =>
                    incoming.id ===
                    current.id,
                ),
            ),
          ];

        savePrompts(merged);
        setPrompts(merged);

        onMessage(
          `Imported ${imported.length} prompt(s) successfully.`,
        );
      } catch (error) {
        onMessage(
          `Prompt import failed: ${String(
            error,
          )}`,
        );
      }
    };

  const resetLibrary =
    () => {
      const confirmed =
        window.confirm(
          "Reset Prompt Library to the built-in templates? Custom prompts will be removed.",
        );

      if (!confirmed) {
        return;
      }

      const next =
        resetDefaultPrompts();

      setPrompts(next);
      setSelectedPromptId(
        next[0]?.id ??
          null,
      );
      onMessage(
        "Prompt Library reset successfully.",
      );
    };

  return (
    <section className="page-section prompt-library-page">
      <div className="page-heading">
        <div>
          <h1>
            Prompt Library
          </h1>
          <p>
            Create, organise and reuse prompts across MultiLLM Compare and Smart Router.
          </p>
        </div>

        <div className="prompt-library-heading-actions">
          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              void importJson();
            }}
          >
            Import JSON
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              void exportMarkdown();
            }}
          >
            Export Markdown
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              void exportJson();
            }}
          >
            Export JSON
          </button>

          <button
            type="button"
            className="action-button"
            onClick={
              startCreate
            }
          >
            ＋ New Prompt
          </button>
        </div>
      </div>

      <div className="prompt-library-toolbar">
        <input
          type="search"
          value={
            searchText
          }
          placeholder="Search prompts, content or tags…"
          onChange={(event) =>
            setSearchText(
              event.target.value,
            )
          }
        />

        <select
          value={
            categoryFilter
          }
          onChange={(event) =>
            setCategoryFilter(
              event.target
                .value as
                | PromptCategory
                | "all",
            )
          }
        >
          <option value="all">
            All categories
          </option>
          {Object.entries(
            CATEGORY_LABELS,
          ).map(
            ([
              category,
              label,
            ]) => (
              <option
                key={category}
                value={
                  category
                }
              >
                {label}
              </option>
            ),
          )}
        </select>

        <label className="prompt-library-favorites-filter">
          <input
            type="checkbox"
            checked={
              favoritesOnly
            }
            onChange={(
              event,
            ) =>
              setFavoritesOnly(
                event.target
                  .checked,
              )
            }
          />
          Favorites only
        </label>

        <span>
          {
            filteredPrompts
              .length
          }{" "}
          prompt(s)
        </span>
      </div>

      <div className="prompt-library-layout">
        <aside
          className="settings-card prompt-library-list"
          style={cardStyle}
        >
          {filteredPrompts.length ===
          0 ? (
            <p className="prompt-library-empty">
              No matching prompts.
            </p>
          ) : (
            filteredPrompts.map(
              (prompt) => (
                <button
                  key={
                    prompt.id
                  }
                  type="button"
                  className={[
                    "prompt-library-list-item",
                    selectedPrompt?.id ===
                    prompt.id
                      ? "prompt-library-list-item-active"
                      : "",
                  ].join(" ")}
                  onClick={() =>
                    setSelectedPromptId(
                      prompt.id,
                    )
                  }
                >
                  <div>
                    <strong>
                      {prompt.favorite
                        ? "★ "
                        : ""}
                      {
                        prompt.title
                      }
                    </strong>

                    <span
                      className={[
                        "prompt-category-badge",
                        `prompt-category-${prompt.category}`,
                      ].join(" ")}
                    >
                      {
                        CATEGORY_LABELS[
                          prompt
                            .category
                        ]
                      }
                    </span>
                  </div>

                  <small>
                    {
                      prompt.description ||
                      "No description"
                    }
                  </small>

                  <span className="prompt-library-list-tags">
                    {prompt.tags
                      .slice(0, 3)
                      .map(
                        (tag) => (
                          <span
                            key={tag}
                          >
                            {tag}
                          </span>
                        ),
                      )}
                  </span>
                </button>
              ),
            )
          )}
        </aside>

        <main
          className="settings-card prompt-library-detail"
          style={cardStyle}
        >
          {editingPromptId ? (
            <div className="prompt-library-editor">
              <div className="prompt-library-editor-heading">
                <div>
                  <h2>
                    {editingPromptId ===
                    "new"
                      ? "New Prompt"
                      : "Edit Prompt"}
                  </h2>
                  <p>
                    Title and prompt content are required.
                  </p>
                </div>
              </div>

              <label>
                <span>
                  Title
                </span>
                <input
                  value={
                    draft.title
                  }
                  onChange={(
                    event,
                  ) =>
                    setDraft(
                      (
                        current,
                      ) => ({
                        ...current,
                        title:
                          event
                            .target
                            .value,
                      }),
                    )
                  }
                />
              </label>

              <label>
                <span>
                  Description
                </span>
                <input
                  value={
                    draft.description
                  }
                  onChange={(
                    event,
                  ) =>
                    setDraft(
                      (
                        current,
                      ) => ({
                        ...current,
                        description:
                          event
                            .target
                            .value,
                      }),
                    )
                  }
                />
              </label>

              <div className="prompt-library-editor-row">
                <label>
                  <span>
                    Category
                  </span>
                  <select
                    value={
                      draft.category
                    }
                    onChange={(
                      event,
                    ) =>
                      setDraft(
                        (
                          current,
                        ) => ({
                          ...current,
                          category:
                            event
                              .target
                              .value as PromptCategory,
                        }),
                      )
                    }
                  >
                    {Object.entries(
                      CATEGORY_LABELS,
                    ).map(
                      ([
                        category,
                        label,
                      ]) => (
                        <option
                          key={
                            category
                          }
                          value={
                            category
                          }
                        >
                          {label}
                        </option>
                      ),
                    )}
                  </select>
                </label>

                <label>
                  <span>
                    Tags
                  </span>
                  <input
                    value={
                      draft.tags
                    }
                    placeholder="comma, separated, tags"
                    onChange={(
                      event,
                    ) =>
                      setDraft(
                        (
                          current,
                        ) => ({
                          ...current,
                          tags:
                            event
                              .target
                              .value,
                        }),
                      )
                    }
                  />
                </label>
              </div>

              <label>
                <span>
                  Prompt
                </span>
                <textarea
                  className="prompt-library-content-editor"
                  value={
                    draft.content
                  }
                  onChange={(
                    event,
                  ) =>
                    setDraft(
                      (
                        current,
                      ) => ({
                        ...current,
                        content:
                          event
                            .target
                            .value,
                      }),
                    )
                  }
                />
              </label>

              <div className="prompt-library-editor-actions">
                <button
                  type="button"
                  className="secondary-button"
                  onClick={
                    cancelEdit
                  }
                >
                  Cancel
                </button>

                <button
                  type="button"
                  className="action-button"
                  onClick={
                    saveDraft
                  }
                >
                  Save Prompt
                </button>
              </div>
            </div>
          ) : selectedPrompt ? (
            <>
              <header className="prompt-library-detail-header">
                <div>
                  <div className="prompt-library-title-row">
                    <h2>
                      {
                        selectedPrompt
                          .title
                      }
                    </h2>

                    <span
                      className={[
                        "prompt-category-badge",
                        `prompt-category-${selectedPrompt.category}`,
                      ].join(" ")}
                    >
                      {
                        CATEGORY_LABELS[
                          selectedPrompt
                            .category
                        ]
                      }
                    </span>

                    {selectedPrompt
                      .builtIn && (
                      <span className="prompt-library-built-in">
                        Built-in
                      </span>
                    )}
                  </div>

                  <p>
                    {
                      selectedPrompt
                        .description
                    }
                  </p>
                </div>

                <div className="prompt-library-detail-actions">
                  <button
                    type="button"
                    className="prompt-library-icon-button"
                    title={
                      selectedPrompt
                        .favorite
                        ? "Remove favorite"
                        : "Add favorite"
                    }
                    onClick={() =>
                      toggleFavorite(
                        selectedPrompt,
                      )
                    }
                  >
                    {selectedPrompt
                      .favorite
                      ? "★"
                      : "☆"}
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() =>
                      startEdit(
                        selectedPrompt,
                      )
                    }
                  >
                    Edit
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() =>
                      duplicatePrompt(
                        selectedPrompt,
                      )
                    }
                  >
                    Duplicate
                  </button>

                  <button
                    type="button"
                    className="danger-button"
                    onClick={() =>
                      removePrompt(
                        selectedPrompt,
                      )
                    }
                  >
                    Delete
                  </button>
                </div>
              </header>

              <div className="prompt-library-tags">
                {selectedPrompt.tags.map(
                  (tag) => (
                    <span
                      key={tag}
                    >
                      {tag}
                    </span>
                  ),
                )}
              </div>

              <pre className="prompt-library-content">
                {
                  selectedPrompt
                    .content
                }
              </pre>

              <div className="prompt-library-use-actions">
                <button
                  type="button"
                  className="action-button"
                  onClick={() =>
                    onUsePrompt(
                      selectedPrompt
                        .content,
                      "compare",
                    )
                  }
                >
                  Use in Compare
                </button>

                <button
                  type="button"
                  className="secondary-button"
                  onClick={() =>
                    onUsePrompt(
                      selectedPrompt
                        .content,
                      "router",
                    )
                  }
                >
                  Use in Smart Router
                </button>

                <button
                  type="button"
                  className="secondary-button"
                  onClick={() => {
                    void navigator
                      .clipboard
                      .writeText(
                        selectedPrompt
                          .content,
                      );

                    onMessage(
                      "Prompt copied successfully.",
                    );
                  }}
                >
                  Copy Prompt
                </button>
              </div>
            </>
          ) : (
            <div className="prompt-library-empty prompt-library-detail-empty">
              <strong>
                No prompt selected
              </strong>
              <p>
                Select a prompt or create a new one.
              </p>
            </div>
          )}
        </main>
      </div>

      <div className="prompt-library-footer-actions">
        <button
          type="button"
          className="danger-button"
          onClick={
            resetLibrary
          }
        >
          Reset Built-in Templates
        </button>
      </div>
    </section>
  );
}

export default PromptLibraryPage;
