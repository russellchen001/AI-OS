import "./InlineAlert.css";

type InlineAlertProps = {
  message: string;
  type?: "error" | "warning" | "success" | "info";
};

function InlineAlert({
  message,
  type = "error",
}: InlineAlertProps) {
  if (!message) {
    return null;
  }

  return (
    <div
      className={`inline-alert inline-alert-${type}`}
      role={type === "error" ? "alert" : "status"}
    >
      <span
        className="inline-alert-icon"
        aria-hidden="true"
      >
        {type === "error"
          ? "⚠️"
          : type === "warning"
            ? "⚠️"
            : type === "success"
              ? "✓"
              : "ℹ️"}
      </span>

      <span>{message}</span>
    </div>
  );
}

export default InlineAlert;
