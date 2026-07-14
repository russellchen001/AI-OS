import {
  useEffect,
  useRef,
} from "react";

import "./ConfirmDialog.css";

type ConfirmDialogProps = {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm Delete",
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const dialogRef =
    useRef<HTMLDivElement | null>(null);

  const cancelButtonRef =
    useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    const previousFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    const previousOverflow =
      document.body.style.overflow;

    document.body.style.overflow = "hidden";

    requestAnimationFrame(() => {
      cancelButtonRef.current?.focus();
    });

    function handleKeyDown(
      event: KeyboardEvent,
    ) {
      if (
        event.key === "Escape" &&
        !busy
      ) {
        event.preventDefault();
        onCancel();
        return;
      }

      if (
        event.key !== "Tab" ||
        !dialogRef.current
      ) {
        return;
      }

      const focusable =
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        );

      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }

      const first = focusable[0];
      const last =
        focusable[focusable.length - 1];

      if (
        event.shiftKey &&
        document.activeElement === first
      ) {
        event.preventDefault();
        last.focus();
      } else if (
        !event.shiftKey &&
        document.activeElement === last
      ) {
        event.preventDefault();
        first.focus();
      }
    }

    window.addEventListener(
      "keydown",
      handleKeyDown,
    );

    return () => {
      window.removeEventListener(
        "keydown",
        handleKeyDown,
      );

      document.body.style.overflow =
        previousOverflow;

      previousFocus?.focus();
    };
  }, [
    open,
    busy,
    onCancel,
  ]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="confirm-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (
          event.target ===
            event.currentTarget &&
          !busy
        ) {
          onCancel();
        }
      }}
    >
      <div
        ref={dialogRef}
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        aria-describedby="confirm-dialog-message"
      >
        <div
          className="confirm-dialog-icon"
          aria-hidden="true"
        >
          ⚠️
        </div>

        <h2 id="confirm-dialog-title">
          {title}
        </h2>

        <p id="confirm-dialog-message">
          {message}
        </p>

        <div className="confirm-dialog-actions">
          <button
            ref={cancelButtonRef}
            type="button"
            className="secondary-button"
            disabled={busy}
            onClick={onCancel}
          >
            Cancel
          </button>

          <button
            type="button"
            className="danger-button"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy && (
              <span
                className="confirm-dialog-spinner"
                aria-hidden="true"
              />
            )}

            <span>
              {busy
                ? "Working..."
                : confirmLabel}
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}

export default ConfirmDialog;
