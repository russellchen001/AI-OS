type HeaderProps = {
  isChecking: boolean;
  lastUpdated: string;
};

function Header({
  isChecking,
  lastUpdated,
}: HeaderProps) {
  return (
    <header className="top-header">
      <div>
        <h1>AI OS</h1>

        <p>
          Your Personal AI Workspace
        </p>
      </div>

      <div className="updated-badge">
        <span
          className={[
            "updated-dot",
            isChecking
              ? "updated-dot-checking"
              : "",
          ]
            .filter(Boolean)
            .join(" ")}
        />

        {isChecking
          ? "Checking services..."
          : `Updated ${lastUpdated}`}
      </div>
    </header>
  );
}

export default Header;