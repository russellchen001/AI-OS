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
}> = [
  {
    name: "Dashboard",
    icon: "🏠",
  },
  {
    name: "Services",
    icon: "🚀",
  },
  {
    name: "Backup",
    icon: "💾",
  },
  {
    name: "Logs",
    icon: "📜",
  },
  {
    name: "Settings",
    icon: "⚙️",
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

      <nav className="nav-list">
        {navItems.map((item) => (
          <button
            key={item.name}
            type="button"
            className={[
              "nav-item",
              activePage === item.name
                ? "nav-item-active"
                : "",
            ]
              .filter(Boolean)
              .join(" ")}
            onClick={() =>
              onPageChange(item.name)
            }
          >
            <span>{item.icon}</span>
            <span>{item.name}</span>
          </button>
        ))}
      </nav>

      <div className="refresh-card">
        <div className="refresh-label">
          Auto Refresh
        </div>

        <div className="refresh-value">
          <span className="online-dot" />
          Every {settings.refreshInterval} seconds
        </div>
      </div>
    </aside>
  );
}

export default Sidebar;