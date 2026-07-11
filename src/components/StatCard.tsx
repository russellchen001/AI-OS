import type {
  CSSProperties,
} from "react";

type StatCardProps = {
  title: string;
  value: number;
  icon: string;
  accent: string;
  cardStyle: CSSProperties;
};

function StatCard({
  title,
  value,
  icon,
  accent,
  cardStyle,
}: StatCardProps) {
  return (
    <div
      className="stat-card"
      style={cardStyle}
    >
      <div
        className="stat-card-glow"
        style={{
          background: accent,
        }}
      />

      <div className="stat-card-header">
        <span>{title}</span>
        <span>{icon}</span>
      </div>

      <div
        className="stat-card-value"
        style={{ color: accent }}
      >
        {value}
      </div>
    </div>
  );
}

export default StatCard;