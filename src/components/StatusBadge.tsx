import type {
  ServiceStatus,
} from "../types";

type StatusBadgeProps = {
  status: ServiceStatus;
};

function StatusBadge({
  status,
}: StatusBadgeProps) {
  return (
    <span
      className={`status-badge status-${status.toLowerCase()}`}
    >
      <span>●</span>
      {status}
    </span>
  );
}

export default StatusBadge;