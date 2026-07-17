import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  open,
  save,
} from "@tauri-apps/plugin-dialog";
import {
  readTextFile,
  writeFile,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import JSZip from "jszip";
import mermaid from "mermaid";
import MarkdownRenderer from "../components/MarkdownRenderer";
import {
  addTagsToArtifacts,
  createArtifactProject,
  deleteArtifact,
  deleteArtifactProject,
  deleteArtifacts,
  extensionForLanguage,
  loadArtifactProjects,
  loadArtifacts,
  moveArtifacts,
  replaceArtifactWorkspace,
  updateArtifact,
  updateArtifactProject,
} from "../services/artifacts";
import type {
  ArtifactProject,
  ArtifactRecord,
  ArtifactWorkspaceExport,
} from "../types/artifact";

type ArtifactsPageProps = {
  cardStyle: CSSProperties;
  onMessage: (
    message: string,
  ) => void;
};

type WorkspaceView =
  | "projects"
  | "artifacts"
  | "docs"
  | "code"
  | "exports";

type SortMode =
  | "updated-desc"
  | "updated-asc"
  | "name-asc"
  | "name-desc";

function safeFilename(
  value: string,
): string {
  return (
    value
      .trim()
      .replace(
        /[^a-zA-Z0-9\u4e00-\u9fff_.-]+/g,
        "-",
      )
      .replace(
        /^-+|-+$/g,
        "",
      )
      .slice(0, 90) ||
    "artifact"
  );
}

function MermaidPreview({
  content,
}: {
  content: string;
}) {
  const containerRef =
    useRef<HTMLDivElement | null>(
      null,
    );

  const [
    error,
    setError,
  ] = useState("");

  useEffect(() => {
    let disposed = false;

    const render =
      async () => {
        try {
          mermaid.initialize({
            startOnLoad: false,
            securityLevel:
              "strict",
            theme: "dark",
          });

          const id =
            `mermaid-${crypto
              .randomUUID()
              .replaceAll("-", "")}`;

          const result =
            await mermaid.render(
              id,
              content,
            );

          if (
            disposed ||
            !containerRef.current
          ) {
            return;
          }

          containerRef.current.innerHTML =
            result.svg;

          setError("");
        } catch (nextError) {
          if (!disposed) {
            setError(
              String(nextError),
            );
          }
        }
      };

    void render();

    return () => {
      disposed = true;
    };
  }, [content]);

  if (error) {
    return (
      <div className="artifact-preview-error">
        <strong>
          Mermaid rendering failed
        </strong>
        <pre>{error}</pre>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="artifact-preview-mermaid"
    />
  );
}

function ArtifactPreview({
  artifact,
}: {
  artifact: ArtifactRecord;
}) {
  if (
    artifact.language ===
    "markdown"
  ) {
    return (
      <div className="artifact-preview-markdown">
        <MarkdownRenderer
          content={
            artifact.content
          }
          artifactSource={
            artifact.source
          }
          artifactProvider={
            artifact.provider
          }
        />
      </div>
    );
  }

  if (
    artifact.language ===
    "html"
  ) {
    return (
      <iframe
        className="artifact-preview-frame"
        title={
          artifact.title
        }
        sandbox=""
        srcDoc={
          artifact.content
        }
      />
    );
  }

  if (
    artifact.language ===
    "svg"
  ) {
    return (
      <div
        className="artifact-preview-svg"
        dangerouslySetInnerHTML={{
          __html:
            artifact.content,
        }}
      />
    );
  }

  if (
    artifact.language ===
    "mermaid"
  ) {
    return (
      <MermaidPreview
        content={
          artifact.content
        }
      />
    );
  }

  let content =
    artifact.content;

  if (
    artifact.language ===
    "json"
  ) {
    try {
      content =
        JSON.stringify(
          JSON.parse(
            artifact.content,
          ),
          null,
          2,
        );
    } catch {
      // Keep raw invalid JSON.
    }
  }

  return (
    <pre className="artifacts-code">
      <code>{content}</code>
    </pre>
  );
}

function ArtifactsPage({
  cardStyle,
  onMessage,
}: ArtifactsPageProps) {
  const [
    projects,
    setProjects,
  ] = useState<
    ArtifactProject[]
  >(loadArtifactProjects);

  const [
    artifacts,
    setArtifacts,
  ] = useState<
    ArtifactRecord[]
  >(loadArtifacts);

  const [
    view,
    setView,
  ] = useState<
    WorkspaceView
  >("projects");

  const [
    searchText,
    setSearchText,
  ] = useState("");

  const [
    languageFilter,
    setLanguageFilter,
  ] = useState("all");

  const [
    sourceFilter,
    setSourceFilter,
  ] = useState("all");

  const [
    providerFilter,
    setProviderFilter,
  ] = useState("all");

  const [
    tagFilter,
    setTagFilter,
  ] = useState("all");

  const [
    favoritesOnly,
    setFavoritesOnly,
  ] = useState(false);

  const [
    sortMode,
    setSortMode,
  ] = useState<
    SortMode
  >("updated-desc");

  const [
    selectedProjectId,
    setSelectedProjectId,
  ] = useState<
    string | null
  >(
    loadArtifactProjects()[0]
      ?.id ??
      null,
  );

  const [
    selectedArtifactId,
    setSelectedArtifactId,
  ] = useState<
    string | null
  >(null);

  const [
    selectedIds,
    setSelectedIds,
  ] = useState<
    Set<string>
  >(new Set());
  const [
    editingProjectId,
    setEditingProjectId,
  ] = useState<string | null>(
    null,
  );
  const [
    creatingProject,
    setCreatingProject,
  ] = useState(false);

  const [
    newProjectTitle,
    setNewProjectTitle,
  ] = useState("");


  const [
    projectTitleDraft,
    setProjectTitleDraft,
  ] = useState("");


  const [
    detailMode,
    setDetailMode,
  ] = useState<
    "preview" | "source" | "edit"
  >("preview");
  const [
    draggedArtifactId,
    setDraggedArtifactId,
  ] = useState<string | null>(
    null,
  );
  const draggedArtifactIdRef =
    useRef<string | null>(
      null,
    );

  const [
    dragOverProjectId,
    setDragOverProjectId,
  ] = useState<string | null>(
    null,
  );
  const [
    pointerDraggedArtifactId,
    setPointerDraggedArtifactId,
  ] = useState<string | null>(
    null,
  );

  const [
    pointerDragPosition,
    setPointerDragPosition,
  ] = useState({
    x: 0,
    y: 0,
  });



  const [
    contentDraft,
    setContentDraft,
  ] = useState("");

  const [
    pathDraft,
    setPathDraft,
  ] = useState("");

  useEffect(() => {
    if (!pointerDraggedArtifactId) {
      return;
    }

    const handlePointerMove =
      (event: PointerEvent) => {
        setPointerDragPosition({
          x: event.clientX,
          y: event.clientY,
        });

        const element =
          document.elementFromPoint(
            event.clientX,
            event.clientY,
          );

        const target =
          element?.closest<HTMLElement>(
            "[data-artifact-project-id]",
          );

        setDragOverProjectId(
          target?.dataset
            .artifactProjectId ??
            null,
        );
      };

    const handlePointerUp =
      (event: PointerEvent) => {
        const element =
          document.elementFromPoint(
            event.clientX,
            event.clientY,
          );

        const target =
          element?.closest<HTMLElement>(
            "[data-artifact-project-id]",
          );

        const projectId =
          target?.dataset
            .artifactProjectId;

        const project =
          projects.find(
            (item) =>
              item.id ===
              projectId,
          );

        if (project) {
          moveDraggedArtifact(
            project,
            pointerDraggedArtifactId,
          );
        }

        setPointerDraggedArtifactId(
          null,
        );

        setDragOverProjectId(
          null,
        );
      };

    window.addEventListener(
      "pointermove",
      handlePointerMove,
    );

    window.addEventListener(
      "pointerup",
      handlePointerUp,
    );

    window.addEventListener(
      "pointercancel",
      handlePointerUp,
    );

    return () => {
      window.removeEventListener(
        "pointermove",
        handlePointerMove,
      );

      window.removeEventListener(
        "pointerup",
        handlePointerUp,
      );

      window.removeEventListener(
        "pointercancel",
        handlePointerUp,
      );
    };
  }, [
    pointerDraggedArtifactId,
    projects,
    artifacts,
  ]);

  const refresh =
    () => {
      setProjects(
        loadArtifactProjects(),
      );
      setArtifacts(
        loadArtifacts(),
      );
    };

  useEffect(() => {
    const openCreateProject =
      () => {
        setCreatingProject(
          true,
        );

        setNewProjectTitle(
          "",
        );
      };

    window.addEventListener(
      "ai-os:create-artifact-project",
      openCreateProject,
    );

    window.addEventListener(
      "ai-os:artifact-created",
      refresh,
    );

    return () => {
      window.removeEventListener(
        "ai-os:create-artifact-project",
        openCreateProject,
      );

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
              (item) =>
                item.language,
            ),
          ),
        ).sort(),
      [artifacts],
    );

  const sources =
    useMemo(
      () =>
        Array.from(
          new Set(
            artifacts.map(
              (item) =>
                item.source,
            ),
          ),
        ).sort(),
      [artifacts],
    );

  const providers =
    useMemo(
      () =>
        Array.from(
          new Set(
            artifacts
              .map(
                (item) =>
                  item.provider,
              )
              .filter(
                (
                  value,
                ): value is string =>
                  Boolean(value),
              ),
          ),
        ).sort(),
      [artifacts],
    );

  const tags =
    useMemo(
      () =>
        Array.from(
          new Set(
            artifacts.flatMap(
              (item) =>
                item.tags,
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

      const next =
        artifacts.filter(
          (artifact) => {
            if (
              selectedProjectId &&
              view ===
                "projects" &&
              artifact.projectId !==
                selectedProjectId
            ) {
              return false;
            }

            if (
              view === "docs" &&
              ![
                "document",
                "diagram",
                "data",
              ].includes(
                artifact.kind,
              )
            ) {
              return false;
            }

            if (
              view === "code" &&
              [
                "document",
                "diagram",
              ].includes(
                artifact.kind,
              )
            ) {
              return false;
            }

            if (
              languageFilter !==
                "all" &&
              artifact.language !==
                languageFilter
            ) {
              return false;
            }

            if (
              sourceFilter !==
                "all" &&
              artifact.source !==
                sourceFilter
            ) {
              return false;
            }

            if (
              providerFilter !==
                "all" &&
              artifact.provider !==
                providerFilter
            ) {
              return false;
            }

            if (
              tagFilter !==
                "all" &&
              !artifact.tags.includes(
                tagFilter,
              )
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

            return [
              artifact.title,
              artifact.filename,
              artifact.path,
              artifact.content,
              artifact.source,
              artifact.provider ??
                "",
              artifact.tags.join(
                " ",
              ),
            ].some((value) =>
              value
                .toLowerCase()
                .includes(query),
            );
          },
        );

      return next.sort(
        (left, right) => {
          const favoriteOrder =
            Number(
              right.favorite,
            ) -
            Number(
              left.favorite,
            );

          if (
            favoriteOrder !== 0
          ) {
            return favoriteOrder;
          }

          switch (sortMode) {
            case "updated-asc":
              return (
                left.updatedAt -
                right.updatedAt
              );

            case "name-asc":
              return left.path.localeCompare(
                right.path,
              );

            case "name-desc":
              return right.path.localeCompare(
                left.path,
              );

            default:
              return (
                right.updatedAt -
                left.updatedAt
              );
          }
        },
      );
    }, [
      artifacts,
      favoritesOnly,
      languageFilter,
      providerFilter,
      searchText,
      selectedProjectId,
      sortMode,
      sourceFilter,
      tagFilter,
      view,
    ]);

  const selectedProject =
    projects.find(
      (project) =>
        project.id ===
        selectedProjectId,
    ) ??
    projects[0] ??
    null;

  const selectedArtifact =
    artifacts.find(
      (artifact) =>
        artifact.id ===
        selectedArtifactId,
    ) ??
    filteredArtifacts[0] ??
    null;

  const createProject =
    () => {
      const title =
        newProjectTitle.trim();

      if (!title) {
        onMessage(
          "Unable to create project: name is required.",
        );
        return;
      }

      const project =
        createArtifactProject(
          title,
        );

      refresh();

      setSelectedProjectId(
        project.id,
      );

      setView(
        "projects",
      );

      setCreatingProject(
        false,
      );

      setNewProjectTitle(
        "",
      );

      onMessage(
        "Artifact project created successfully.",
      );
    };

  const saveProjectTitle =
    (
      project:
        ArtifactProject,
    ) => {
      const title =
        projectTitleDraft.trim();

      if (!title) {
        onMessage(
          "Unable to rename project: name is required.",
        );
        return;
      }

      setProjects(
        updateArtifactProject(
          project.id,
          (current) => ({
            ...current,
            title,
          }),
        ),
      );

      setEditingProjectId(
        null,
      );

      setProjectTitleDraft(
        "",
      );

      onMessage(
        "Project renamed successfully.",
      );
    };

  const saveEditedArtifact =
    () => {
      if (!selectedArtifact) {
        return;
      }

      const normalizedPath =
        pathDraft.trim();

      if (!normalizedPath) {
        onMessage(
          "Unable to save Artifact: path is required.",
        );
        return;
      }

      setArtifacts(
        updateArtifact(
          selectedArtifact.id,
          (current) => ({
            ...current,
            path:
              normalizedPath,
            filename:
              normalizedPath
                .split("/")
                .pop() ||
              current.filename,
            title:
              normalizedPath
                .split("/")
                .pop() ||
              current.title,
            content:
              contentDraft,
          }),
        ),
      );

      setDetailMode(
        "preview",
      );

      onMessage(
        "Artifact updated successfully.",
      );
    };

  const beginEditing =
    () => {
      if (!selectedArtifact) {
        return;
      }

      setPathDraft(
        selectedArtifact.path,
      );
      setContentDraft(
        selectedArtifact.content,
      );
      setDetailMode("edit");
    };

  const exportArtifact =
    async (
      artifact:
        ArtifactRecord,
    ) => {
      const extension =
        extensionForLanguage(
          artifact.language,
        );

      const filename =
        artifact.filename
          .includes(".")
          ? artifact.filename
          : `${artifact.filename}.${extension}`;

      const filePath =
        await save({
          defaultPath:
            safeFilename(
              filename,
            ),
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
    };

  const exportProjectZip =
    async (
      project:
        ArtifactProject,
    ) => {
      try {
        const files =
          artifacts.filter(
            (artifact) =>
              artifact.projectId ===
              project.id,
          );

        if (
          files.length === 0
        ) {
          onMessage(
            "Unable to export project: no files.",
          );
          return;
        }

        const filePath =
          await save({
            defaultPath:
              `${safeFilename(
                project.title,
              )}.zip`,
            filters: [
              {
                name: "ZIP",
                extensions: [
                  "zip",
                ],
              },
            ],
          });

        if (!filePath) {
          return;
        }

        const zip =
          new JSZip();

        for (
          const artifact
          of files
        ) {
          zip.file(
            artifact.path,
            artifact.content,
          );
        }

        zip.file(
          "ai-os-project.json",
          JSON.stringify(
            {
              project,
              artifacts: files,
            },
            null,
            2,
          ),
        );

        const bytes =
          await zip.generateAsync({
            type: "uint8array",
            compression:
              "DEFLATE",
            compressionOptions: {
              level: 6,
            },
          });

        await writeFile(
          filePath,
          bytes,
        );

        onMessage(
          `Project exported to ${filePath}`,
        );
      } catch (error) {
        onMessage(
          `Project ZIP export failed: ${String(
            error,
          )}`,
        );
      }
    };

  const exportWorkspace =
    async () => {
      const filePath =
        await save({
          defaultPath:
            "ai-os-artifacts-workspace.json",
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
        ArtifactWorkspaceExport = {
        schemaVersion: 2,
        exportedAt:
          new Date()
            .toISOString(),
        projects,
        artifacts,
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
        `Workspace exported to ${filePath}`,
      );
    };

  const importWorkspace =
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

        const parsed =
          JSON.parse(
            content,
          ) as Partial<ArtifactWorkspaceExport>;

        if (
          !Array.isArray(
            parsed.projects,
          ) ||
          !Array.isArray(
            parsed.artifacts,
          )
        ) {
          throw new Error(
            "Invalid Artifacts Workspace file.",
          );
        }

        const replace =
          window.confirm(
            "Replace the current Workspace?\n\nChoose Cancel to merge imported data.",
          );

        const nextProjects =
          replace
            ? parsed.projects
            : [
                ...parsed.projects,
                ...projects.filter(
                  (current) =>
                    !parsed.projects?.some(
                      (incoming) =>
                        incoming.id ===
                        current.id,
                    ),
                ),
              ];

        const nextArtifacts =
          replace
            ? parsed.artifacts
            : [
                ...parsed.artifacts,
                ...artifacts.filter(
                  (current) =>
                    !parsed.artifacts?.some(
                      (incoming) =>
                        incoming.id ===
                        current.id,
                    ),
                ),
              ];

        replaceArtifactWorkspace(
          nextProjects,
          nextArtifacts,
        );

        refresh();

        onMessage(
          `Imported ${parsed.projects.length} project(s) and ${parsed.artifacts.length} Artifact(s).`,
        );
      } catch (error) {
        onMessage(
          `Workspace import failed: ${String(
            error,
          )}`,
        );
      }
    };

  const toggleSelection =
    (id: string) => {
      setSelectedIds(
        (current) => {
          const next =
            new Set(current);

          if (next.has(id)) {
            next.delete(id);
          } else {
            next.add(id);
          }

          return next;
        },
      );
    };

  const moveDraggedArtifact =
    (
      project:
        ArtifactProject,
      transferredId?: string,
    ) => {
      const artifactId =
        transferredId ||
        draggedArtifactIdRef.current ||
        draggedArtifactId;

      if (!artifactId) {
        onMessage(
          "Unable to move Artifact: drag data was not found.",
        );
        return;
      }

      const artifact =
        artifacts.find(
          (item) =>
            item.id ===
            artifactId,
        );

      if (!artifact) {
        onMessage(
          "Unable to move Artifact: file was not found.",
        );
        return;
      }

      if (
        artifact.projectId ===
        project.id
      ) {
        onMessage(
          `Artifact is already in ${project.title}.`,
        );
      } else {
        const next =
          moveArtifacts(
            [artifactId],
            project.id,
          );

        setArtifacts(next);
        setSelectedProjectId(
          project.id,
        );
        setSelectedArtifactId(
          artifactId,
        );
        setView("projects");

        onMessage(
          `Artifact moved to ${project.title}.`,
        );
      }

      draggedArtifactIdRef.current =
        null;

      setDraggedArtifactId(
        null,
      );

      setDragOverProjectId(
        null,
      );
    };

  const batchMove =
    () => {
      if (
        selectedIds.size ===
        0
      ) {
        return;
      }

      const destination =
        window.prompt(
          [
            "Move selected Artifacts to which project ID?",
            "",
            ...projects.map(
              (project) =>
                `${project.title}: ${project.id}`,
            ),
          ].join("\n"),
          selectedProjectId ??
            "",
        );

      if (
        !destination ||
        !projects.some(
          (project) =>
            project.id ===
            destination,
        )
      ) {
        onMessage(
          "Unable to move Artifacts: invalid project.",
        );
        return;
      }

      setArtifacts(
        moveArtifacts(
          Array.from(
            selectedIds,
          ),
          destination,
        ),
      );

      setSelectedIds(
        new Set(),
      );

      onMessage(
        "Selected Artifacts moved successfully.",
      );
    };

  const batchTag =
    () => {
      if (
        selectedIds.size ===
        0
      ) {
        return;
      }

      const value =
        window.prompt(
          "Tags to add, separated by commas:",
        );

      if (!value) {
        return;
      }

      setArtifacts(
        addTagsToArtifacts(
          Array.from(
            selectedIds,
          ),
          value.split(","),
        ),
      );

      onMessage(
        "Tags added successfully.",
      );
    };

  const batchDelete =
    () => {
      if (
        selectedIds.size ===
          0 ||
        !window.confirm(
          `Delete ${selectedIds.size} selected Artifact(s)?`,
        )
      ) {
        return;
      }

      setArtifacts(
        deleteArtifacts(
          Array.from(
            selectedIds,
          ),
        ),
      );

      setSelectedIds(
        new Set(),
      );

      onMessage(
        "Selected Artifacts deleted.",
      );
    };

  return (
    <section className="page-section artifacts-page artifacts-workspace-v2">
      <div className="page-heading">
        <div>
          <h1>
            Artifacts Workspace
          </h1>
          <p>
            Projects, generated code, documents, diagrams and local exports.
          </p>
        </div>

        <div className="artifacts-heading-actions">
          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              void importWorkspace();
            }}
          >
            Import Workspace
          </button>

          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              void exportWorkspace();
            }}
          >
            Export Workspace
          </button>

          <button
            type="button"
            className="action-button"
            onClick={() => {
              setCreatingProject(
                true,
              );

              setNewProjectTitle(
                "",
              );
            }}
          >
            ＋ New Project
          </button>
        </div>
      </div>

      {creatingProject && (
        <div className="artifact-create-project-panel">
          <div>
            <strong>
              Create Project
            </strong>
            <span>
              Enter a name for the new Artifact project.
            </span>
          </div>

          <input
            value={
              newProjectTitle
            }
            autoFocus
            placeholder="Project name"
            onChange={(event) =>
              setNewProjectTitle(
                event.target.value,
              )
            }
            onKeyDown={(event) => {
              if (
                event.key ===
                "Enter"
              ) {
                event.preventDefault();
                createProject();
              }

              if (
                event.key ===
                "Escape"
              ) {
                setCreatingProject(
                  false,
                );
                setNewProjectTitle(
                  "",
                );
              }
            }}
          />

          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              setCreatingProject(
                false,
              );
              setNewProjectTitle(
                "",
              );
            }}
          >
            Cancel
          </button>

          <button
            type="button"
            className="action-button"
            onClick={
              createProject
            }
          >
            Create
          </button>
        </div>
      )}

      <div className="artifact-workspace-tabs">
        {(
          [
            [
              "projects",
              "Projects",
            ],
            [
              "artifacts",
              "All Artifacts",
            ],
            [
              "docs",
              "Docs",
            ],
            [
              "code",
              "Code",
            ],
            [
              "exports",
              "Exports",
            ],
          ] as Array<
            [
              WorkspaceView,
              string,
            ]
          >
        ).map(
          ([id, label]) => (
            <button
              key={id}
              type="button"
              className={
                view === id
                  ? "artifact-workspace-tab-active"
                  : ""
              }
              onClick={() =>
                setView(id)
              }
            >
              {label}
            </button>
          ),
        )}
      </div>

      <div className="artifacts-toolbar artifacts-toolbar-v2">
        <input
          type="search"
          value={
            searchText
          }
          placeholder="Search filename, content, tags or Provider…"
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
            (value) => (
              <option
                key={value}
                value={value}
              >
                {value}
              </option>
            ),
          )}
        </select>

        <select
          value={sourceFilter}
          onChange={(event) =>
            setSourceFilter(
              event.target.value,
            )
          }
        >
          <option value="all">
            All sources
          </option>
          {sources.map(
            (value) => (
              <option
                key={value}
                value={value}
              >
                {value}
              </option>
            ),
          )}
        </select>

        <select
          value={
            providerFilter
          }
          onChange={(event) =>
            setProviderFilter(
              event.target.value,
            )
          }
        >
          <option value="all">
            All Providers
          </option>
          {providers.map(
            (value) => (
              <option
                key={value}
                value={value}
              >
                {value}
              </option>
            ),
          )}
        </select>

        <select
          value={tagFilter}
          onChange={(event) =>
            setTagFilter(
              event.target.value,
            )
          }
        >
          <option value="all">
            All tags
          </option>
          {tags.map(
            (value) => (
              <option
                key={value}
                value={value}
              >
                {value}
              </option>
            ),
          )}
        </select>

        <select
          value={sortMode}
          onChange={(event) =>
            setSortMode(
              event.target
                .value as SortMode,
            )
          }
        >
          <option value="updated-desc">
            Recently updated
          </option>
          <option value="updated-asc">
            Oldest updated
          </option>
          <option value="name-asc">
            Name A–Z
          </option>
          <option value="name-desc">
            Name Z–A
          </option>
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
          Favorites
        </label>
      </div>

      {selectedIds.size >
        0 && (
        <div className="artifact-bulk-toolbar">
          <strong>
            {selectedIds.size} selected
          </strong>

          <button
            type="button"
            onClick={
              batchMove
            }
          >
            Move
          </button>

          <button
            type="button"
            onClick={
              batchTag
            }
          >
            Add Tags
          </button>

          <button
            type="button"
            className="danger-button"
            onClick={
              batchDelete
            }
          >
            Delete
          </button>

          <button
            type="button"
            onClick={() =>
              setSelectedIds(
                new Set(),
              )
            }
          >
            Clear
          </button>
        </div>
      )}

      <div className="artifact-workspace-layout">
        <aside
          className="settings-card artifact-project-list"
          style={cardStyle}
        >
          <div className="artifact-panel-heading">
            <strong>
              Projects
            </strong>
            <span>
              {projects.length}
            </span>
          </div>

          {projects.map(
            (project) => (
              <div
                key={project.id}
                data-artifact-project-id={
                  project.id
                }
                className={[
                  "artifact-project-item",
                  selectedProject?.id ===
                  project.id
                    ? "artifact-project-item-active"
                    : "",
                  dragOverProjectId ===
                  project.id
                    ? "artifact-project-item-drop-target"
                    : "",
                ].join(" ")}
                onDragEnterCapture={(
                  event,
                ) => {
                  event.preventDefault();
                  event.stopPropagation();

                  setDragOverProjectId(
                    project.id,
                  );
                }}
                onDragOverCapture={(
                  event,
                ) => {
                  event.preventDefault();
                  event.stopPropagation();

                  event.dataTransfer.dropEffect =
                    "move";

                  setDragOverProjectId(
                    project.id,
                  );
                }}
                onDragLeave={(
                  event,
                ) => {
                  const nextTarget =
                    event.relatedTarget;

                  if (
                    nextTarget instanceof
                      Node &&
                    event.currentTarget.contains(
                      nextTarget,
                    )
                  ) {
                    return;
                  }

                  setDragOverProjectId(
                    null,
                  );
                }}
                onDropCapture={(
                  event,
                ) => {
                  event.preventDefault();
                  event.stopPropagation();

                  const transferredId =
                    event.dataTransfer.getData(
                      "application/x-ai-os-artifact",
                    ) ||
                    event.dataTransfer.getData(
                      "text/plain",
                    );

                  moveDraggedArtifact(
                    project,
                    transferredId,
                  );
                }}
              >
                <button
                  type="button"
                  onClick={() => {
                    setSelectedProjectId(
                      project.id,
                    );
                    setView(
                      "projects",
                    );
                  }}
                >
                  {editingProjectId ===
                  project.id ? (
                    <input
                      className="artifact-project-title-input"
                      value={
                        projectTitleDraft
                      }
                      autoFocus
                      onClick={(event) =>
                        event.stopPropagation()
                      }
                      onChange={(event) =>
                        setProjectTitleDraft(
                          event.target.value,
                        )
                      }
                      onKeyDown={(event) => {
                        if (
                          event.key ===
                          "Enter"
                        ) {
                          event.preventDefault();

                          saveProjectTitle(
                            project,
                          );
                        }

                        if (
                          event.key ===
                          "Escape"
                        ) {
                          setEditingProjectId(
                            null,
                          );

                          setProjectTitleDraft(
                            "",
                          );
                        }
                      }}
                    />
                  ) : (
                    <strong>
                      {project.favorite
                        ? "★ "
                        : ""}
                      {project.title}
                    </strong>
                  )}
                  <small>
                    {
                      artifacts.filter(
                        (artifact) =>
                          artifact.projectId ===
                          project.id,
                      ).length
                    }{" "}
                    file(s)
                  </small>
                </button>

                <div>
                  <button
                    type="button"
                    title="Export ZIP"
                    onClick={() => {
                      void exportProjectZip(
                        project,
                      );
                    }}
                  >
                    ZIP
                  </button>

                  <button
                    type="button"
                    title={
                      editingProjectId ===
                      project.id
                        ? "Save name"
                        : "Rename"
                    }
                    onClick={() => {
                      if (
                        editingProjectId ===
                        project.id
                      ) {
                        saveProjectTitle(
                          project,
                        );
                        return;
                      }

                      setEditingProjectId(
                        project.id,
                      );

                      setProjectTitleDraft(
                        project.title,
                      );
                    }}
                  >
                    {editingProjectId ===
                    project.id
                      ? "✓"
                      : "✏️"}
                  </button>

                  <button
                    type="button"
                    title="Delete"
                    onClick={() => {
                      if (
                        !window.confirm(
                          `Delete "${project.title}" and all files?`,
                        )
                      ) {
                        return;
                      }

                      const result =
                        deleteArtifactProject(
                          project.id,
                        );

                      setProjects(
                        result.projects,
                      );
                      setArtifacts(
                        result.artifacts,
                      );
                    }}
                  >
                    🗑
                  </button>
                </div>
              </div>
            ),
          )}
        </aside>

        <aside
          className="settings-card artifact-file-tree"
          style={cardStyle}
        >
          <div className="artifact-panel-heading">
            <strong>
              Files
            </strong>
            <span>
              {
                filteredArtifacts.length
              }
            </span>
          </div>

          {filteredArtifacts.map(
            (artifact) => (
              <div
                key={artifact.id}
                className={[
                  "artifact-tree-row",
                  selectedArtifact?.id ===
                  artifact.id
                    ? "artifact-tree-row-active"
                    : "",
                  draggedArtifactId ===
                  artifact.id
                    ? "artifact-tree-row-dragging"
                    : "",
                ].join(" ")}
                title="Hold and drag this file onto a Project"
                onPointerDown={(
                  event,
                ) => {
                  if (
                    event.button !== 0
                  ) {
                    return;
                  }

                  const target =
                    event.target;

                  if (
                    target instanceof
                      HTMLInputElement
                  ) {
                    return;
                  }

                  event.preventDefault();

                  setPointerDraggedArtifactId(
                    artifact.id,
                  );

                  setPointerDragPosition({
                    x: event.clientX,
                    y: event.clientY,
                  });
                }}
              >
                <input
                  type="checkbox"
                  checked={
                    selectedIds.has(
                      artifact.id,
                    )
                  }
                  onChange={() =>
                    toggleSelection(
                      artifact.id,
                    )
                  }
                />

                <button
                  type="button"
                  onClick={() =>
                    setSelectedArtifactId(
                      artifact.id,
                    )
                  }
                >
                  <span>
                    {artifact.favorite
                      ? "★ "
                      : ""}
                    {artifact.path}
                  </span>
                  <small>
                    {artifact.language}
                    {" · "}
                    {artifact.source}
                    {artifact.provider
                      ? ` · ${artifact.provider}`
                      : ""}
                  </small>
                </button>
              </div>
            ),
          )}
        </aside>

        <main
          className="settings-card artifacts-detail"
          style={cardStyle}
        >
          {selectedArtifact ? (
            <>
              <header className="artifacts-detail-header">
                <div>
                  <h2>
                    {
                      selectedArtifact.path
                    }
                  </h2>
                  <p>
                    {
                      selectedArtifact.language
                    }
                    {" · "}
                    {
                      selectedArtifact.kind
                    }
                    {" · "}
                    {
                      selectedArtifact.source
                    }
                    {selectedArtifact.provider
                      ? ` · ${selectedArtifact.provider}`
                      : ""}
                  </p>

                  <div className="artifact-detail-tags">
                    {selectedArtifact.tags.map(
                      (tag) => (
                        <span key={tag}>
                          {tag}
                        </span>
                      ),
                    )}
                  </div>
                </div>

                <div className="artifacts-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={
                      beginEditing
                    }
                  >
                    Edit
                  </button>

                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() => {
                      void navigator
                        .clipboard
                        .writeText(
                          selectedArtifact.content,
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
                        selectedArtifact,
                      );
                    }}
                  >
                    Export
                  </button>

                  <button
                    type="button"
                    className="danger-button"
                    onClick={() => {
                      if (
                        !window.confirm(
                          `Delete "${selectedArtifact.path}"?`,
                        )
                      ) {
                        return;
                      }

                      setArtifacts(
                        deleteArtifact(
                          selectedArtifact.id,
                        ),
                      );
                      setSelectedArtifactId(
                        null,
                      );
                    }}
                  >
                    Delete
                  </button>
                </div>
              </header>

              <div className="artifact-detail-tabs">
                {(
                  [
                    "preview",
                    "source",
                    "edit",
                  ] as const
                ).map(
                  (mode) => (
                    <button
                      key={mode}
                      type="button"
                      className={
                        detailMode ===
                        mode
                          ? "artifact-detail-tab-active"
                          : ""
                      }
                      onClick={() => {
                        if (
                          mode ===
                          "edit"
                        ) {
                          beginEditing();
                        } else {
                          setDetailMode(
                            mode,
                          );
                        }
                      }}
                    >
                      {mode}
                    </button>
                  ),
                )}
              </div>

              {detailMode ===
              "edit" ? (
                <div className="artifact-editor">
                  <label>
                    <span>
                      Path
                    </span>
                    <input
                      value={
                        pathDraft
                      }
                      onChange={(
                        event,
                      ) =>
                        setPathDraft(
                          event.target
                            .value,
                        )
                      }
                    />
                  </label>

                  <label>
                    <span>
                      Content
                    </span>
                    <textarea
                      value={
                        contentDraft
                      }
                      onChange={(
                        event,
                      ) =>
                        setContentDraft(
                          event.target
                            .value,
                        )
                      }
                    />
                  </label>

                  <div>
                    <button
                      type="button"
                      className="secondary-button"
                      onClick={() =>
                        setDetailMode(
                          "preview",
                        )
                      }
                    >
                      Cancel
                    </button>

                    <button
                      type="button"
                      className="action-button"
                      onClick={
                        saveEditedArtifact
                      }
                    >
                      Save Changes
                    </button>
                  </div>
                </div>
              ) : detailMode ===
                "source" ? (
                <pre className="artifacts-code">
                  <code>
                    {
                      selectedArtifact.content
                    }
                  </code>
                </pre>
              ) : (
                <ArtifactPreview
                  artifact={
                    selectedArtifact
                  }
                />
              )}
            </>
          ) : (
            <div className="artifacts-empty">
              Select an Artifact.
            </div>
          )}
        </main>
      </div>
      {pointerDraggedArtifactId && (
        <div
          className="artifact-pointer-drag-preview"
          style={{
            left:
              pointerDragPosition.x +
              14,
            top:
              pointerDragPosition.y +
              14,
          }}
        >
          {
            artifacts.find(
              (artifact) =>
                artifact.id ===
                pointerDraggedArtifactId,
            )?.path ??
            "Artifact"
          }
        </div>
      )}
    </section>
  );
}

export default ArtifactsPage;
