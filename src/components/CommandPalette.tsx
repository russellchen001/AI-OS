import {
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  PageName,
} from "../types/index";
import {
  loadArtifacts,
  loadArtifactProjects,
} from "../services/artifacts";
import {
  loadCouncilSessions,
} from "../services/council";

type CommandKind =
  | "page"
  | "action"
  | "prompt"
  | "artifact"
  | "project"
  | "council";

type CommandItem = {
  id: string;
  label: string;
  description: string;
  icon: string;
  kind: CommandKind;
  keywords: string;
  run: () => void;
};

type CommandPaletteProps = {
  open: boolean;
  activePage: PageName;
  onClose: () => void;
  onPageChange: (
    page: PageName,
  ) => void;
  onMessage: (
    message: string,
  ) => void;
};

const RECENT_KEY =
  "ai-os.command-palette.recent.v1";

const PAGE_COMMANDS:
  Array<{
    page: PageName;
    icon: string;
    label: string;
    description: string;
  }> = [
    {
      page: "Chat",
      icon: "✦",
      label: "Open Workspace",
      description: "AI conversation workspace",
    },
    {
      page: "My AI",
      icon: "◎",
      label: "Open My AI",
      description: "Providers, accounts and models",
    },
    {
      page: "Models",
      icon: "🧠",
      label: "Open Models",
      description: "Manage local models",
    },
    {
      page: "MCP",
      icon: "🔌",
      label: "Open Skills",
      description: "Configure MCP servers",
    },
    {
      page: "Artifacts",
      icon: "🧱",
      label: "Open Files",
      description: "Browse generated artifacts",
    },
    {
      page: "AI Council",
      icon: "🏛",
      label: "Open AI Council",
      description: "Multi-agent collaboration",
    },
    {
      page: "AI Arena",
      icon: "◐",
      label: "Open AI Arena",
      description: "Compare AI capabilities",
    },
    {
      page: "Agents",
      icon: "⌁",
      label: "Open Agents",
      description: "Manage AI agents",
    },
    {
      page: "Settings",
      icon: "⚙",
      label: "Open Settings",
      description: "System configuration",
    },
  ];


function loadRecentIds():
  string[] {
  try {
    const parsed: unknown =
      JSON.parse(
        localStorage.getItem(
          RECENT_KEY,
        ) ?? "[]",
      );

    return Array.isArray(parsed)
      ? parsed.filter(
          (
            item,
          ): item is string =>
            typeof item ===
            "string",
        )
      : [];
  } catch {
    return [];
  }
}

function saveRecentId(
  id: string,
): void {
  const next = [
    id,
    ...loadRecentIds().filter(
      (current) =>
        current !== id,
    ),
  ].slice(0, 8);

  localStorage.setItem(
    RECENT_KEY,
    JSON.stringify(next),
  );
}

function CommandPalette({
  open,
  activePage,
  onClose,
  onPageChange,
  onMessage,
}: CommandPaletteProps) {
  const [
    query,
    setQuery,
  ] = useState("");

  const [
    selectedIndex,
    setSelectedIndex,
  ] = useState(0);

  const inputRef =
    useRef<HTMLInputElement | null>(
      null,
    );

  const commands =
    useMemo<
      CommandItem[]
    >(() => {
      const navigate =
        (
          page: PageName,
        ) => {
          onPageChange(page);
          onClose();
        };

      const pages =
        PAGE_COMMANDS.map(
          (
            item,
          ): CommandItem => ({
            id:
              `page:${item.page}`,
            label:
              item.label,
            description:
              item.page ===
              activePage
                ? `${item.description} · Current page`
                : item.description,
            icon:
              item.icon,
            kind: "page",
            keywords:
              `${item.page} ${item.label} ${item.description}`,
            run: () =>
              navigate(
                item.page,
              ),
          }),
        );

      const actions:
        CommandItem[] = [
        {
          id:
            "action:new-project",
          label:
            "Create Artifact Project",
          description:
            "Open Artifacts and create a new project",
          icon: "＋",
          kind: "action",
          keywords:
            "new create project artifact workspace",
          run: () => {
            onPageChange(
              "Artifacts",
            );

            window.setTimeout(
              () => {
                window.dispatchEvent(
                  new CustomEvent(
                    "ai-os:create-artifact-project",
                  ),
                );
              },
              80,
            );

            onClose();
          },
        },
        {
          id:
            "action:run-council",
          label:
            "Run AI Council",
          description:
            "Open the AI Council workspace",
          icon: "🏛",
          kind: "action",
          keywords:
            "run council agents collaborate judge",
          run: () =>
            navigate(
              "AI Council",
            ),
        },
        {
          id:
            "action:providers",
          label:
            "Open Provider Settings",
          description:
            "Open AI Center provider configuration",
          icon: "🔑",
          kind: "action",
          keywords:
            "provider api key model settings ai center",
          run: () => {
            localStorage.setItem(
              "ai-os.provider.pending-tab.v1",
              "providers",
            );

            navigate(
                "My AI",
              );
          },
        },
        ];

      const artifacts =
        loadArtifacts().map(
          (
            artifact,
          ): CommandItem => ({
            id:
              `artifact:${artifact.id}`,
            label:
              artifact.path,
            description:
              `Artifact · ${artifact.language} · ${artifact.source}${
                artifact.provider
                  ? ` · ${artifact.provider}`
                  : ""
              }`,
            icon:
              artifact.favorite
                ? "★"
                : "🧱",
            kind:
              "artifact",
            keywords: [
              artifact.path,
              artifact.title,
              artifact.content,
              artifact.language,
              artifact.source,
              artifact.provider ??
                "",
              ...artifact.tags,
            ].join(" "),
            run: () => {
              localStorage.setItem(
                "ai-os.artifacts.selected.v1",
                artifact.id,
              );

              navigate(
                "Artifacts",
              );
            },
          }),
        );

      const projects =
        loadArtifactProjects().map(
          (
            project,
          ): CommandItem => ({
            id:
              `project:${project.id}`,
            label:
              project.title,
            description:
              `Artifact Project · ${project.description || "No description"}`,
            icon:
              project.favorite
                ? "★"
                : "📁",
            kind:
              "project",
            keywords: [
              project.title,
              project.description,
              ...project.tags,
            ].join(" "),
            run: () => {
              localStorage.setItem(
                "ai-os.artifacts.selected-project.v1",
                project.id,
              );

              navigate(
                "Artifacts",
              );
            },
          }),
        );

      const council =
        loadCouncilSessions().map(
          (
            session,
          ): CommandItem => ({
            id:
              `council:${session.id}`,
            label:
              session.title,
            description:
              `Council Session · ${new Date(
                session.createdAt,
              ).toLocaleString()}`,
            icon:
              session.favorite
                ? "★"
                : "🏛",
            kind:
              "council",
            keywords: [
              session.title,
              session.prompt,
              session.finalAnswer,
            ].join(" "),
            run: () => {
              localStorage.setItem(
                "ai-os.council.selected.v1",
                session.id,
              );

              navigate(
                "AI Council",
              );
            },
          }),
        );

      return [
        ...actions,
        ...pages,
        ...projects,
        ...artifacts,
        ...council,
      ];
    }, [
      activePage,
      onClose,
      onPageChange,
    ]);

  const visibleCommands =
    useMemo(() => {
      const normalized =
        query
          .trim()
          .toLowerCase();

      const recentIds =
        loadRecentIds();

      if (!normalized) {
        return [
          ...commands
            .filter((command) =>
              recentIds.includes(
                command.id,
              ),
            )
            .sort(
              (left, right) =>
                recentIds.indexOf(
                  left.id,
                ) -
                recentIds.indexOf(
                  right.id,
                ),
            ),
          ...commands.filter(
            (command) =>
              !recentIds.includes(
                command.id,
              ),
          ),
        ].slice(0, 30);
      }

      return commands
        .map((command) => {
          const haystack =
            `${command.label} ${command.description} ${command.keywords}`
              .toLowerCase();

          const starts =
            command.label
              .toLowerCase()
              .startsWith(
                normalized,
              );

          const includes =
            haystack.includes(
              normalized,
            );

          return {
            command,
            score:
              starts
                ? 3
                : includes
                  ? 1
                  : 0,
          };
        })
        .filter(
          (item) =>
            item.score > 0,
        )
        .sort(
          (left, right) =>
            right.score -
            left.score,
        )
        .map(
          (item) =>
            item.command,
        )
        .slice(0, 50);
    }, [
      commands,
      query,
    ]);

  useEffect(() => {
    if (!open) {
      return;
    }

    setQuery("");
    setSelectedIndex(0);

    window.setTimeout(
      () => {
        inputRef.current
          ?.focus();
      },
      20,
    );
  }, [open]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  if (!open) {
    return null;
  }

  const execute =
    (
      command:
        CommandItem,
    ) => {
      saveRecentId(
        command.id,
      );

      try {
        command.run();
      } catch (error) {
        onMessage(
          `Command failed: ${String(
            error,
          )}`,
        );
      }
    };

  return (
    <div
      className="command-palette-backdrop"
      role="presentation"
      onMouseDown={(
        event,
      ) => {
        if (
          event.target ===
          event.currentTarget
        ) {
          onClose();
        }
      }}
    >
      <section
        className="command-palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command Palette"
      >
        <div className="command-palette-search">
          <span
            aria-hidden="true"
          >
            ⌕
          </span>

          <input
            ref={inputRef}
            type="search"
            value={query}
            placeholder="Search commands, projects, Artifacts or Council sessions…"
            onChange={(
              event,
            ) =>
              setQuery(
                event.target
                  .value,
              )
            }
            onKeyDown={(
              event,
            ) => {
              if (
                event.key ===
                "Escape"
              ) {
                event.preventDefault();
                onClose();
                return;
              }

              if (
                event.key ===
                "ArrowDown"
              ) {
                event.preventDefault();

                setSelectedIndex(
                  (current) =>
                    Math.min(
                      current +
                        1,
                      Math.max(
                        visibleCommands.length -
                          1,
                        0,
                      ),
                    ),
                );

                return;
              }

              if (
                event.key ===
                "ArrowUp"
              ) {
                event.preventDefault();

                setSelectedIndex(
                  (current) =>
                    Math.max(
                      current -
                        1,
                      0,
                    ),
                );

                return;
              }

              if (
                event.key ===
                  "Enter" &&
                visibleCommands[
                  selectedIndex
                ]
              ) {
                event.preventDefault();

                execute(
                  visibleCommands[
                    selectedIndex
                  ],
                );
              }
            }}
          />

          <kbd>Esc</kbd>
        </div>

        <div className="command-palette-results">
          {visibleCommands.length ===
          0 ? (
            <div className="command-palette-empty">
              No matching commands.
            </div>
          ) : (
            visibleCommands.map(
              (
                command,
                index,
              ) => (
                <button
                  key={
                    command.id
                  }
                  type="button"
                  className={[
                    "command-palette-item",
                    index ===
                    selectedIndex
                      ? "command-palette-item-active"
                      : "",
                  ].join(" ")}
                  onMouseEnter={() =>
                    setSelectedIndex(
                      index,
                    )
                  }
                  onClick={() =>
                    execute(
                      command,
                    )
                  }
                >
                  <span className="command-palette-icon">
                    {
                      command.icon
                    }
                  </span>

                  <span className="command-palette-copy">
                    <strong>
                      {
                        command.label
                      }
                    </strong>

                    <small>
                      {
                        command.description
                      }
                    </small>
                  </span>

                  <span className="command-palette-kind">
                    {
                      command.kind
                    }
                  </span>
                </button>
              ),
            )
          )}
        </div>

        <footer className="command-palette-footer">
          <span>
            ↑↓ Navigate
          </span>
          <span>
            ↵ Open
          </span>
          <span>
            ⌘K Toggle
          </span>
        </footer>
      </section>
    </div>
  );
}

export default CommandPalette;
