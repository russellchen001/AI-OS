import {
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from "react";
import {
  save,
} from "@tauri-apps/plugin-dialog";
import {
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import {
  deleteArtifact,
  loadArtifacts,
  updateArtifact,
} from "../services/artifacts";
import type {
  ArtifactRecord,
} from "../types/artifact";

type ArtifactsPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

const EXTENSIONS:
  Record<string, string> = {
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
  code: "txt",
};

function safeFilename(
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
      .slice(0, 80) ||
    "artifact"
  );
}

function ArtifactsPage({
  cardStyle,
  onMessage,
}: ArtifactsPageProps) {
  const [
    artifacts,
    setArtifacts,
  ] = useState<
    ArtifactRecord[]
  >(loadArtifacts);

  const [
    searchText,
    setSearchText,
  ] = useState("");

  const [
    languageFilter,
    setLanguageFilter,
  ] = useState("all");

  const [
    favoritesOnly,
    setFavoritesOnly,
  ] = useState(false);

  const [
    selectedId,
    setSelectedId,
  ] = useState<
    string | null
  >(null);

  const [
    editingTitle,
    setEditingTitle,
  ] = useState(false);

  const [
    titleDraft,
    setTitleDraft,
  ] = useState("");

  useEffect(() => {
    const refresh =
      () => {
        setArtifacts(
          loadArtifacts(),
        );
      };

    window.addEventListener(
      "ai-os:artifact-created",
      refresh,
    );

    return () => {
      window.removeEventListener(
        "ai-os:artifact-created",
        refresh,
      );
    };
  }, []);

  const languages =
    useMemo(
      () =>
        Array.from(
          new Set(
            artifacts.map(
              (artifact) =>
                artifact.language,
            ),
          ),
        ).sort(),
      [artifacts],
    );

  const filteredArtifacts =
    useMemo(() => {
      const query =
        searchText
          .trim()
          .toLowerCase();

      return [...artifacts]
        .filter(
          (artifact) => {
            if (
              languageFilter !==
                "all" &&
              artifact.language !==
                languageFilter
            ) {
              return false;
            }

            if (
              favoritesOnly &&
              !artifact.favorite
            ) {
              return false;
            }

            if (!query) {
              return true;
            }

            return (
              artifact.title
                .toLowerCase()
                .includes(query) ||
              artifact.content
                .toLowerCase()
                .includes(query) ||
              artifact.source
                .toLowerCase()
                .includes(query) ||
              artifact.language
                .includes(query)
            );
          },
        )
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
      artifacts,
      favoritesOnly,
      languageFilter,
      searchText,
    ]);

  const selected =
    artifacts.find(
      (artifact) =>
        artifact.id ===
        selectedId,
    ) ??
    filteredArtifacts[0] ??
    null;

  const toggleFavorite =
    (
      artifact:
        ArtifactRecord,
    ) => {
      const next =
        updateArtifact(
          artifact.id,
          (current) => ({
            ...current,
            favorite:
              !current.favorite,
          }),
        );

      setArtifacts(next);
    };

  const saveTitle =
    (
      artifact:
        ArtifactRecord,
    ) => {
      const title =
        titleDraft.trim();

      if (!title) {
        return;
      }

      const next =
        updateArtifact(
          artifact.id,
          (current) => ({
            ...current,
            title,
          }),
        );

      setArtifacts(next);
      setEditingTitle(false);
      onMessage(
        "Artifact renamed successfully.",
      );
    };

  const removeArtifact =
    (
      artifact:
        ArtifactRecord,
    ) => {
      const confirmed =
        window.confirm(
          `Delete "${artifact.title}"?`,
        );

      if (!confirmed) {
        return;
      }

      const next =
        deleteArtifact(
          artifact.id,
        );

      setArtifacts(next);
      setSelectedId(
        next[0]?.id ??
          null,
      );

      onMessage(
        "Artifact deleted successfully.",
      );
    };

  const exportArtifact =
    async (
      artifact:
        ArtifactRecord,
    ) => {
      try {
        const extension =
          EXTENSIONS[
            artifact.language
          ] ?? "txt";

        const filePath =
          await save({
            defaultPath:
              `${safeFilename(
                artifact.title,
              )}.${extension}`,
            filters: [
              {
                name:
                  artifact.language,
                extensions: [
                  extension,
                ],
              },
            ],
          });

        if (!filePath) {
          return;
        }

        await writeTextFile(
          filePath,
          artifact.content,
        );

        onMessage(
          `Artifact exported to ${filePath}`,
        );
      } catch (error) {
        onMessage(
          `Artifact export failed: ${String(
            error,
          )}`,
        );
      }
    };

  return (
    <section className="page-section artifacts-page">
      <div className="page-heading">
        <div>
          <h1>
            Artifacts
          </h1>
          <p>
            Save, organise and export code generated by MultiLLM.
          </p>
        </div>
      </div>

      <div className="artifacts-toolbar">
        <input
          type="search"
          value={
            searchText
          }
          placeholder="Search artifacts…"
          onChange={(event) =>
            setSearchText(
              event.target.value,
            )
          }
        />

        <select
          value={
            languageFilter
          }
          onChange={(event) =>
            setLanguageFilter(
              event.target.value,
            )
          }
        >
          <option value="all">
            All languages
          </option>

          {languages.map(
            (language) => (
              <option
                key={language}
                value={language}
              >
                {language}
              </option>
            ),
          )}
        </select>

        <label>
          <input
            type="checkbox"
            checked={
              favoritesOnly
            }
            onChange={(event) =>
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
            filteredArtifacts
              .length
          }{" "}
          artifact(s)
        </span>
      </div>

      <div className="artifacts-layout">
        <aside
          className="settings-card artifacts-list"
          style={cardStyle}
        >
          {filteredArtifacts.length ===
          0 ? (
            <div className="artifacts-empty">
              No saved artifacts yet.
            </div>
          ) : (
            filteredArtifacts.map(
              (artifact) => (
                <button
                  key={
                    artifact.id
                  }
                  type="button"
                  className={[
                    "artifacts-list-item",
                    selected?.id ===
                    artifact.id
                      ? "artifacts-list-item-active"
                      : "",
                  ].join(" ")}
                  onClick={() =>
                    setSelectedId(
                      artifact.id,
                    )
                  }
                >
                  <strong>
                    {artifact.favorite
                      ? "★ "
                      : ""}
                    {
                      artifact.title
                    }
                  </strong>

                  <span>
                    {
                      artifact.language
                    }
                    {" · "}
                    {
                      artifact.source
                    }
                  </span>

                  <small>
                    {new Date(
                      artifact.updatedAt,
                    ).toLocaleString()}
                  </small>
                </button>
              ),
            )
          )}
        </aside>

        <main
          className="settings-card artifacts-detail"
          style={cardStyle}
        >
          {selected ? (
            <>
              <header className="artifacts-detail-header">
                <div>
                  {editingTitle ? (
                    <input
                      value={
                        titleDraft
                      }
                      autoFocus
                      onChange={(
                        event,
                      ) =>
                        setTitleDraft(
                          event.target
                            .value,
                        )
                      }
                      onKeyDown={(
                        event,
                      ) => {
                        if (
                          event.key ===
                          "Enter"
                        ) {
                          saveTitle(
                            selected,
                          );
                        }

                        if (
                          event.key ===
                          "Escape"
                        ) {
                          setEditingTitle(
                            false,
                          );
                        }
                      }}
                    />
                  ) : (
                    <h2>
                      {
                        selected.title
                      }
                    </h2>
                  )}

                  <p>
                    {
                      selected.language
                    }
                    {" · "}
                    {
                      selected.source
                    }
                  </p>
                </div>

                <div className="artifacts-actions">
                  <button
                    type="button"
                    className="prompt-library-icon-button"
                    onClick={() =>
                      toggleFavorite(
                        selected,
                      )
                    }
                  >
                    {selected.favorite
                      ? "★"
                      : "☆"}
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => {
                      setTitleDraft(
                        selected.title,
                      );
                      setEditingTitle(
                        true,
                      );
                    }}
                  >
                    Rename
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => {
                      void navigator
                        .clipboard
                        .writeText(
                          selected.content,
                        );

                      onMessage(
                        "Artifact copied successfully.",
                      );
                    }}
                  >
                    Copy
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => {
                      void exportArtifact(
                        selected,
                      );
                    }}
                  >
                    Export
                  </button>

                  <button
                    type="button"
                    className="danger-button"
                    onClick={() =>
                      removeArtifact(
                        selected,
                      )
                    }
                  >
                    Delete
                  </button>
                </div>
              </header>

              <pre className="artifacts-code">
                <code>
                  {
                    selected.content
                  }
                </code>
              </pre>
            </>
          ) : (
            <div className="artifacts-empty">
              Select an artifact saved from a MultiLLM code block.
            </div>
          )}
        </main>
      </div>
    </section>
  );
}

export default ArtifactsPage;
