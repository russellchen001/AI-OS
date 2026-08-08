import { useEffect, useState } from "react";
import type {
  PageName,
} from "../types/index";
import type { Conversation } from "../services/conversations";

type SidebarProps = {
  activePage: PageName;

  onPageChange: (
    page: PageName,
  ) => void;
  onOpenCommandPalette:
    () => void;
  conversations: Conversation[];
  activeConversationId: string;
  onNewConversation: () => void;
  onSelectConversation: (conversationId: string) => void;
  onRenameConversation: (conversationId: string, title: string) => void;
  onDeleteConversation: (conversationId: string) => void;
};

const navItems: Array<{
  name: PageName;
  icon: string;
  label: string;
}> = [
  {
    name: "Chat",
    icon: "✦",
    label: "Workspace",
  },
  {
    name: "Artifacts",
    icon: "◫",
    label: "Files",
  },
  {
    name: "My AI",
    icon: "◎",
    label: "My AI",
  },
  {
    name: "Agents",
    icon: "⌁",
    label: "Agents",
  },
  {
    name: "AI Council",
    icon: "◉",
    label: "AI Council",
  },
  {
    name: "AI Arena",
    icon: "◐",
    label: "AI Arena",
  },
  {
    name: "MCP",
    icon: "◇",
    label: "Skills",
  },
  {
    name: "Settings",
    icon: "○",
    label: "Settings",
  },
];

function Sidebar({
  activePage,
  onPageChange,
  onOpenCommandPalette,
  conversations,
  activeConversationId,
  onNewConversation,
  onSelectConversation,
  onRenameConversation,
  onDeleteConversation,
}: SidebarProps) {
  const [renamingConversationId, setRenamingConversationId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [openMenuConversationId, setOpenMenuConversationId] = useState<string | null>(null);
  const [confirmDeleteConversationId, setConfirmDeleteConversationId] = useState<string | null>(null);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0 });

  useEffect(() => {
    const closeMenu = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest(".conversation-history-menu")
      ) {
        return;
      }
      setOpenMenuConversationId(null);
      setConfirmDeleteConversationId(null);
    };

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenMenuConversationId(null);
        setConfirmDeleteConversationId(null);
      }
    };

    document.addEventListener("pointerdown", closeMenu);
    document.addEventListener("keydown", closeOnEscape);

    return () => {
      document.removeEventListener("pointerdown", closeMenu);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, []);

  const finishRename = () => {
    const title = renameValue.trim();
    if (renamingConversationId && title) {
      onRenameConversation(renamingConversationId, title);
    }
    setRenamingConversationId(null);
    setRenameValue("");
  };

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-icon">
          A
        </div>

        <div>
          <div className="brand-title">
            AI‑OS
          </div>

          <div className="brand-subtitle">
            Personal workspace
          </div>
        </div>
      </div>

      <nav
        className="nav-list"
        aria-label="Main navigation"
      >
        {navItems.map(
          (item) => {
            const active =
              activePage ===
              item.name;

            return (
              <button
                key={item.name}
                type="button"
                className={[
                  "nav-item",
                  active
                    ? "nav-item-active"
                    : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                aria-current={
                  active
                    ? "page"
                    : undefined
                }
                onClick={() => onPageChange(item.name)}
              >
                <span
                  className="nav-item-icon"
                  aria-hidden="true"
                >
                  {item.icon}
                </span>

                <span>
                  {item.label}
                </span>
              </button>
            );
          },
        )}
      </nav>

      <section className="conversation-history" aria-label="Conversation history">
        <div className="conversation-history-heading">
          <span>Recent</span>
          <button type="button" aria-label="New conversation" onClick={onNewConversation}>+</button>
        </div>
        <div className="conversation-history-list">
          {conversations.length === 0 && (
            <p className="conversation-history-empty">Your conversations will appear here.</p>
          )}
          {conversations.slice(0, 20).map((conversation) => (
            <div
              key={conversation.id}
              className={
                conversation.id === activeConversationId
                  ? "conversation-history-item conversation-history-active"
                  : "conversation-history-item"
              }
            >
              {renamingConversationId === conversation.id ? (
                <input
                  className="conversation-history-rename-input"
                  value={renameValue}
                  autoFocus
                  aria-label={`Rename ${conversation.title}`}
                  onChange={(event) => setRenameValue(event.target.value)}
                  onBlur={finishRename}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      finishRename();
                    }
                    if (event.key === "Escape") {
                      setRenamingConversationId(null);
                      setRenameValue("");
                    }
                  }}
                />
              ) : (
                <button
                  type="button"
                  className="conversation-history-title"
                  onClick={() => onSelectConversation(conversation.id)}
                >
                  {conversation.title}
                </button>
              )}
              <div className="conversation-history-actions conversation-history-menu">
                <button
                  type="button"
                  aria-label={`More actions for ${conversation.title}`}
                  aria-haspopup="menu"
                  aria-expanded={openMenuConversationId === conversation.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    const rect = event.currentTarget.getBoundingClientRect();
                    setMenuPosition({
                      top: Math.min(rect.top, window.innerHeight - 180),
                      left: rect.right + 12,
                    });
                    setConfirmDeleteConversationId(null);
                    setOpenMenuConversationId((current) =>
                      current === conversation.id ? null : conversation.id
                    );
                  }}
                >
                  ···
                </button>

                {openMenuConversationId === conversation.id && (
                  <div
                    className="conversation-history-menu-popover"
                    role="menu"
                    style={{
                      top: `${menuPosition.top}px`,
                      left: `${menuPosition.left}px`,
                    }}
                    aria-label={`Actions for ${conversation.title}`}
                    onPointerDown={(event) => event.stopPropagation()}
                  >
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setOpenMenuConversationId(null);
                        setConfirmDeleteConversationId(null);
                        setRenamingConversationId(conversation.id);
                        setRenameValue(conversation.title);
                      }}
                    >
                      Rename
                    </button>

                    {confirmDeleteConversationId === conversation.id ? (
                      <button
                        type="button"
                        role="menuitem"
                        className="conversation-history-menu-danger-confirm"
                        onClick={() => {
                          onDeleteConversation(conversation.id);
                          setOpenMenuConversationId(null);
                          setConfirmDeleteConversationId(null);
                        }}
                      >
                        Confirm delete
                      </button>
                    ) : (
                      <button
                        type="button"
                        role="menuitem"
                        className="conversation-history-menu-danger"
                        onClick={() =>
                          setConfirmDeleteConversationId(conversation.id)
                        }
                      >
                        Delete
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      </section>

      <button
        type="button"
        className="sidebar-command-button"
        onClick={
          onOpenCommandPalette
        }
      >
<span>
          <span aria-hidden="true">⌕</span>
          <span>Search</span>
        </span>
                <kbd>⌘K</kbd>
      </button>

      <div className="sidebar-footer">
        <div className="sidebar-version">
          AI‑OS · Local first
        </div>
      </div>
    </aside>
  );
}

export default Sidebar;
