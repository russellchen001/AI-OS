type ServiceToggleProps = {
  checked: boolean;
  disabled?: boolean;
  loading?: boolean;
  label: string;
  large?: boolean;
  onChange: () => void;
};

function ServiceToggle({
  checked,
  disabled = false,
  loading = false,
  label,
  large = false,
  onChange,
}: ServiceToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={[
        "service-toggle",
        checked
          ? "service-toggle-on"
          : "service-toggle-off",
        large
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
        {label}
      </span>
    </button>
  );
}

export default ServiceToggle;