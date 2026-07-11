import type {
  CSSProperties,
} from "react";

type MetricCardProps = {
  title: string;
  icon: string;
  value: string;
  progress: number;
  accent: string;
  cardStyle: CSSProperties;
};

function MetricCard({
  title,
  icon,
  value,
  progress,
  accent,
  cardStyle,
}: MetricCardProps) {
  const safeProgress = Math.min(
    Math.max(progress, 0),
    100,
  );

  return (
    <div
      className="metric-card"
      style={cardStyle}
    >
      <div className="metric-header">
        <div>
          <span className="metric-title">
            {title}
          </span>

          <div className="metric-value">
            {value}
          </div>
        </div>

        <span className="metric-icon">
          {icon}
        </span>
      </div>

      <div className="metric-track">
        <div
          className="metric-progress"
          style={{
            width: `${safeProgress}%`,
            background: accent,
            boxShadow:
              `0 0 14px ${accent}55`,
          }}
        />
      </div>

      <div className="metric-percent">
        {safeProgress.toFixed(1)}%
      </div>
    </div>
  );
}

export default MetricCard;