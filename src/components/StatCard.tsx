type StatCardProps = {
  title: string;
  value: number;
  icon: string;
  tone:
    | "blue"
    | "green"
    | "red"
    | "yellow";
};

function StatCard({
  title,
  value,
  icon,
  tone,
}: StatCardProps) {
  return (
    <article
      className={`stat-card stat-${tone}`}
    >
      <div className="stat-header">
        <span>{title}</span>
        <span>{icon}</span>
      </div>

      <strong>{value}</strong>
    </article>
  );
}

export default StatCard;