type BackupPageProps = {
  onExport: () => void;
  onRestore: () => void;
  onReset: () => void;
};

type FeatureCardProps = {
  icon: string;
  title: string;
  description: string;
  buttonLabel: string;
  buttonClass: string;
  onClick: () => void;
};

function FeatureCard({
  icon,
  title,
  description,
  buttonLabel,
  buttonClass,
  onClick,
}: FeatureCardProps) {
  return (
    <article className="feature-card">
      <div className="feature-icon">
        {icon}
      </div>

      <h3>{title}</h3>

      <p>{description}</p>

      <button
        className={`action-button ${buttonClass}`}
        onClick={onClick}
      >
        {buttonLabel}
      </button>
    </article>
  );
}

function BackupPage({
  onExport,
  onRestore,
  onReset,
}: BackupPageProps) {
  return (
    <section className="page-card">
      <div className="page-title">
        <div>
          <h2>Backup & Restore</h2>

          <p>
            Export settings and logs to a JSON file.
          </p>
        </div>
      </div>

      <div className="feature-grid">
        <FeatureCard
          icon="📤"
          title="Export Backup"
          description="Save current settings and logs as a JSON backup file."
          buttonLabel="Export JSON"
          buttonClass="action-blue"
          onClick={onExport}
        />

        <FeatureCard
          icon="📥"
          title="Restore Backup"
          description="Restore settings and logs from an exported JSON file."
          buttonLabel="Choose Backup"
          buttonClass="action-green"
          onClick={onRestore}
        />

        <FeatureCard
          icon="♻️"
          title="Reset Settings"
          description="Restore all application settings to their default values."
          buttonLabel="Reset Defaults"
          buttonClass="action-red"
          onClick={onReset}
        />
      </div>
    </section>
  );
}

export default BackupPage;