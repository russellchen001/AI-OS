type MetricCardProps = {
  label: string;
  value: string;
  percent: number;
  icon: string;
};

function MetricCard({
  label,
  value,
  percent,
  icon,
}: MetricCardProps) {
  const safePercent = Math.min(
    Math.max(percent, 0),
    100,
  );

  return (
    <article className="metric-card">
      <div className="metric-header">
        <span>{icon}</span>

        <div>
          <span>{label}</span>
          <strong>{value}</strong>
        </div>
      </div>

      <div className="metric-track">
        <div
          className="metric-progress"
          style={{
            width: `${safePercent}%`,
          }}
        />
      </div>
    </article>
  );
}

export default MetricCard;