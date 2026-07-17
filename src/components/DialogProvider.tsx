import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import "./DialogProvider.css";

type DialogTone =
  | "default"
  | "danger"
  | "warning";

type BaseDialogOptions = {
  title: string;
  message?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: DialogTone;
};

type PromptDialogOptions =
  BaseDialogOptions & {
    initialValue?: string;
    placeholder?: string;
    required?: boolean;
    validate?: (
      value: string,
    ) => string | null;
  };

type SelectOption = {
  value: string;
  label: string;
  description?: string;
};

type SelectDialogOptions =
  BaseDialogOptions & {
    options: SelectOption[];
    initialValue?: string;
  };

type DialogRequest =
  | {
      type: "confirm";
      options:
        BaseDialogOptions;
      resolve: (
        value: boolean,
      ) => void;
    }
  | {
      type: "prompt";
      options:
        PromptDialogOptions;
      resolve: (
        value:
          | string
          | null,
      ) => void;
    }
  | {
      type: "select";
      options:
        SelectDialogOptions;
      resolve: (
        value:
          | string
          | null,
      ) => void;
    }
  | {
      type: "alert";
      options:
        BaseDialogOptions;
      resolve: () => void;
    };

type DialogContextValue = {
  confirm: (
    options:
      BaseDialogOptions,
  ) => Promise<boolean>;

  prompt: (
    options:
      PromptDialogOptions,
  ) => Promise<
    string | null
  >;

  select: (
    options:
      SelectDialogOptions,
  ) => Promise<
    string | null
  >;

  alert: (
    options:
      BaseDialogOptions,
  ) => Promise<void>;
};

const DialogContext =
  createContext<
    DialogContextValue | null
  >(null);

export function useDialog():
  DialogContextValue {
  const value =
    useContext(
      DialogContext,
    );

  if (!value) {
    throw new Error(
      "useDialog must be used inside DialogProvider.",
    );
  }

  return value;
}

type DialogProviderProps = {
  children: ReactNode;
};

export function DialogProvider({
  children,
}: DialogProviderProps) {
  const [
    request,
    setRequest,
  ] = useState<
    DialogRequest | null
  >(null);

  const [
    inputValue,
    setInputValue,
  ] = useState("");

  const [
    validationError,
    setValidationError,
  ] = useState("");

  const dialogRef =
    useRef<HTMLDivElement | null>(
      null,
    );

  const primaryRef =
    useRef<
      HTMLButtonElement | null
    >(null);

  const inputRef =
    useRef<
      HTMLInputElement | null
    >(null);

  const confirm =
    useCallback(
      (
        options:
          BaseDialogOptions,
      ) =>
        new Promise<boolean>(
          (resolve) => {
            setRequest({
              type: "confirm",
              options,
              resolve,
            });
          },
        ),
      [],
    );

  const prompt =
    useCallback(
      (
        options:
          PromptDialogOptions,
      ) =>
        new Promise<
          string | null
        >((resolve) => {
          setInputValue(
            options.initialValue ??
              "",
          );

          setValidationError(
            "",
          );

          setRequest({
            type: "prompt",
            options,
            resolve,
          });
        }),
      [],
    );

  const select =
    useCallback(
      (
        options:
          SelectDialogOptions,
      ) =>
        new Promise<
          string | null
        >((resolve) => {
          setInputValue(
            options.initialValue ??
              options.options[0]
                ?.value ??
              "",
          );

          setValidationError(
            "",
          );

          setRequest({
            type: "select",
            options,
            resolve,
          });
        }),
      [],
    );

  const alert =
    useCallback(
      (
        options:
          BaseDialogOptions,
      ) =>
        new Promise<void>(
          (resolve) => {
            setRequest({
              type: "alert",
              options,
              resolve,
            });
          },
        ),
      [],
    );

  const closeCancelled =
    useCallback(() => {
      if (!request) {
        return;
      }

      if (
        request.type ===
        "confirm"
      ) {
        request.resolve(
          false,
        );
      } else if (
        request.type ===
        "alert"
      ) {
        request.resolve();
      } else {
        request.resolve(
          null,
        );
      }

      setRequest(null);
      setValidationError(
        "",
      );
    }, [request]);

  const submit =
    useCallback(() => {
      if (!request) {
        return;
      }

      if (
        request.type ===
        "prompt"
      ) {
        const value =
          inputValue.trim();

        if (
          request.options
            .required &&
          !value
        ) {
          setValidationError(
            "This field is required.",
          );
          return;
        }

        const error =
          request.options
            .validate?.(
              value,
            );

        if (error) {
          setValidationError(
            error,
          );
          return;
        }

        request.resolve(
          value,
        );
      } else if (
        request.type ===
        "select"
      ) {
        request.resolve(
          inputValue,
        );
      } else if (
        request.type ===
        "confirm"
      ) {
        request.resolve(
          true,
        );
      } else {
        request.resolve();
      }

      setRequest(null);
      setValidationError(
        "",
      );
    }, [
      inputValue,
      request,
    ]);

  useEffect(() => {
    if (!request) {
      return;
    }

    const previousFocus =
      document.activeElement instanceof
      HTMLElement
        ? document.activeElement
        : null;

    const previousOverflow =
      document.body.style
        .overflow;

    document.body.style.overflow =
      "hidden";

    window.setTimeout(
      () => {
        if (
          request.type ===
          "prompt"
        ) {
          inputRef.current
            ?.focus();

          inputRef.current
            ?.select();
        } else {
          primaryRef.current
            ?.focus();
        }
      },
      20,
    );

    const handleKeyDown =
      (
        event:
          KeyboardEvent,
      ) => {
        if (
          event.key ===
          "Escape"
        ) {
          event.preventDefault();
          closeCancelled();
          return;
        }

        if (
          event.key ===
            "Enter" &&
          !event.shiftKey
        ) {
          const target =
            event.target;

          if (
            target instanceof
              HTMLTextAreaElement
          ) {
            return;
          }

          event.preventDefault();
          submit();
          return;
        }

        if (
          event.key !==
            "Tab" ||
          !dialogRef.current
        ) {
          return;
        }

        const focusable =
          dialogRef.current.querySelectorAll<HTMLElement>(
            'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
          );

        if (
          focusable.length ===
          0
        ) {
          event.preventDefault();
          return;
        }

        const first =
          focusable[0];

        const last =
          focusable[
            focusable.length -
              1
          ];

        if (
          event.shiftKey &&
          document.activeElement ===
            first
        ) {
          event.preventDefault();
          last.focus();
        } else if (
          !event.shiftKey &&
          document.activeElement ===
            last
        ) {
          event.preventDefault();
          first.focus();
        }
      };

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
    closeCancelled,
    request,
    submit,
  ]);

  const tone =
    request?.options.tone ??
    "default";

  return (
    <DialogContext.Provider
      value={{
        confirm,
        prompt,
        select,
        alert,
      }}
    >
      {children}

      {request && (
        <div
          className="unified-dialog-backdrop"
          role="presentation"
          onMouseDown={(
            event,
          ) => {
            if (
              event.target ===
              event.currentTarget
            ) {
              closeCancelled();
            }
          }}
        >
          <section
            ref={dialogRef}
            className={[
              "unified-dialog",
              `unified-dialog-${tone}`,
            ].join(" ")}
            role={
              request.type ===
                "confirm" &&
              tone === "danger"
                ? "alertdialog"
                : "dialog"
            }
            aria-modal="true"
            aria-labelledby="unified-dialog-title"
          >
            <header className="unified-dialog-header">
              <span
                className="unified-dialog-icon"
                aria-hidden="true"
              >
                {tone ===
                "danger"
                  ? "⚠"
                  : tone ===
                      "warning"
                    ? "!"
                    : request.type ===
                        "prompt"
                      ? "✎"
                      : request.type ===
                          "select"
                        ? "↪"
                        : "i"}
              </span>

              <div>
                <h2 id="unified-dialog-title">
                  {
                    request.options.title
                  }
                </h2>

                {request.options.message && (
                  <p>
                    {
                      request.options.message
                    }
                  </p>
                )}
              </div>
            </header>

            {request.type ===
              "prompt" && (
              <div className="unified-dialog-field">
                <input
                  ref={inputRef}
                  value={
                    inputValue
                  }
                  placeholder={
                    request.options
                      .placeholder
                  }
                  onChange={(
                    event,
                  ) => {
                    setInputValue(
                      event.target
                        .value,
                    );

                    setValidationError(
                      "",
                    );
                  }}
                />

                {validationError && (
                  <span className="unified-dialog-error">
                    {
                      validationError
                    }
                  </span>
                )}
              </div>
            )}

            {request.type ===
              "select" && (
              <div className="unified-dialog-options">
                {request.options
                  .options.map(
                    (option) => (
                      <label
                        key={
                          option.value
                        }
                        className={[
                          "unified-dialog-option",
                          inputValue ===
                          option.value
                            ? "unified-dialog-option-selected"
                            : "",
                        ].join(
                          " ",
                        )}
                      >
                        <input
                          type="radio"
                          name="unified-dialog-selection"
                          value={
                            option.value
                          }
                          checked={
                            inputValue ===
                            option.value
                          }
                          onChange={() =>
                            setInputValue(
                              option.value,
                            )
                          }
                        />

                        <span>
                          <strong>
                            {
                              option.label
                            }
                          </strong>

                          {option.description && (
                            <small>
                              {
                                option.description
                              }
                            </small>
                          )}
                        </span>
                      </label>
                    ),
                  )}
              </div>
            )}

            <footer className="unified-dialog-actions">
              {request.type !==
                "alert" && (
                <button
                  type="button"
                  className="secondary-button"
                  onClick={
                    closeCancelled
                  }
                >
                  {request.options.cancelLabel ??
                    "Cancel"}
                </button>
              )}

              <button
                ref={primaryRef}
                type="button"
                className={
                  tone ===
                  "danger"
                    ? "danger-button"
                    : "action-button"
                }
                onClick={
                  submit
                }
              >
                {request.options.confirmLabel ??
                  (request.type ===
                  "alert"
                    ? "OK"
                    : request.type ===
                        "prompt"
                      ? "Save"
                      : request.type ===
                          "select"
                        ? "Select"
                        : "Confirm")}
              </button>
            </footer>
          </section>
        </div>
      )}
    </DialogContext.Provider>
  );
}
