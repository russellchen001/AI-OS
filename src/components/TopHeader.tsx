import type {
  Settings,
} from "../types";

type TopHeaderProps = {
  isChecking: boolean;
  lastUpdated: string;
  settings: Settings;
  onSettingsChange: React.Dispatch<
    React.SetStateAction<Settings>
  >;
};

function TopHeader({
  isChecking,
  lastUpdated,
  settings,
  onSettingsChange,
}: TopHeaderProps) {
  function toggleTheme() {
    onSettingsChange((current) => ({
      ...current,
      theme:
        current.theme === "dark"
          ? "light"
          : "dark",
    }));
  }

  return (
    <header className="top-header">
      <div>
        <h1>AI OS</h1>

        <p>
          Your Personal AI Workspace
        </p>
      </div>

      <div className="header-actions">
        <div className="update-status">
          <span
            className={`status-light ${
              isChecking
                ? "checking"
                : ""
            }`}
          />

          {isChecking
            ? "Checking services..."
            : `Updated ${lastUpdated}`}
        </div>

        <button
          className="theme-button"
          onClick={toggleTheme}
          title="Switch theme"
        >
          {settings.theme === "dark"
            ? "☀️"
            : "🌙"}
        </button>
      </div>
    </header>
  );
}

export default TopHeader;