import "./ServiceToggle.css";

type ServiceToggleProps = {
  checked: boolean;
  disabled?: boolean;
  loading?: boolean;
  label?: string;
  onChange: () => void;
  size?: "normal" | "large";
};

function ServiceToggle({
  checked,
  disabled = false,
  loading = false,
  label,
  onChange,
  size = "normal",
}: ServiceToggleProps) {
  const displayLabel =
    label ?? (checked ? "Running" : "Stopped");

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={[
        "service-toggle",
        checked
          ? "service-toggle-on"
          : "service-toggle-off",
        size === "large"
          ? "service-toggle-large"
          : "",
        loading
          ? "service-toggle-loading"
          : "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={onChange}
    >
      <span className="service-toggle-track">
        <span className="service-toggle-thumb">
          {loading && (
            <span className="toggle-spinner" />
          )}
        </span>
      </span>

      <span className="service-toggle-label">
        {displayLabel}
      </span>
    </button>
  );
}

export default ServiceToggle;