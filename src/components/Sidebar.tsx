import { NAV_ITEMS } from "../constants";
import type {
  PageName,
} from "../types";

type SidebarProps = {
  activePage: PageName;
  refreshInterval: number;
  onPageChange: (
    page: PageName,
  ) => void;
};

function Sidebar({
  activePage,
  refreshInterval,
  onPageChange,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-icon">
          🤖
        </div>

        <div>
          <div className="brand-name">
            AI OS
          </div>

          <div className="brand-subtitle">
            Control Center
          </div>
        </div>
      </div>

      <nav className="navigation">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.name}
            className={`nav-item ${
              activePage === item.name
                ? "nav-item-active"
                : ""
            }`}
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
        <div className="refresh-title">
          Auto Refresh
        </div>

        <div className="refresh-value">
          <span className="live-dot" />

          Every {refreshInterval} seconds
        </div>
      </div>

      <div className="sidebar-version">
        Sprint 3.1 · v0.3.1
      </div>
    </aside>
  );
}

export default Sidebar;