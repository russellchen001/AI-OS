import type {
  PageName,
  Settings,
} from "../types/index";

type SidebarProps = {
  activePage: PageName;
  settings: Settings;

  onPageChange: (
    page: PageName,
  ) => void;
};

const navItems: Array<{
  name: PageName;
  icon: string;
  label: string;
}> = [
  {
    name: "Dashboard",
    icon: "🏠",
    label: "Dashboard",
  },
  {
    name: "Services",
    icon: "🚀",
    label: "Services",
  },
  {
    name: "OpenClaw",
    icon: "🦞",
    label: "OpenClaw",
  },
  {
    name: "Backup",
    icon: "💾",
    label: "Backup",
  },
  {
    name: "Logs",
    icon: "📜",
    label: "Logs",
  },
  {
    name: "Models",
    icon: "🧠",
    label: "Models",
  },
  {
    name: "MCP",
    icon: "🔌",
    label: "MCP",
  },
  {
    name: "MultiLLM",
    icon: "🧩",
    label: "MultiLLM",
  },
  {
    name: "Prompt Library",
    icon: "📚",
    label: "Prompts",
  },
  {
    name: "Settings",
    icon: "⚙️",
    label: "Settings",
  },
];

function Sidebar({
  activePage,
  settings,
  onPageChange,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-icon">
          🤖
        </div>

        <div>
          <div className="brand-title">
            AI OS
          </div>

          <div className="brand-subtitle">
            Control Center
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
                onClick={() =>
                  onPageChange(
                    item.name,
                  )
                }
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

      <div className="sidebar-footer">
        <div className="refresh-card">
          <div className="refresh-label">
            Auto Refresh
          </div>

          <div className="refresh-value">
            <span
              className="online-dot"
              aria-hidden="true"
            />

            Every{" "}
            {settings.refreshInterval}{" "}
            seconds
          </div>
        </div>

        <div className="sidebar-version">
          AI OS v1.0.0
        </div>
      </div>
    </aside>
  );
}

export default Sidebar;