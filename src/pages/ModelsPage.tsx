import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import ConfirmDialog from "../components/ConfirmDialog";
import {
  POPULAR_OLLAMA_MODELS,
} from "../config/constants";

import type {
  ModelActionStatus,
  OllamaModel,
  OllamaPullProgress,
} from "../types/index";

type ModelsPageProps = {
  models: OllamaModel[];
  totalSize: number;
  status: ModelActionStatus;
  activeModel: string | null;
  pullProgress:
    | OllamaPullProgress
    | null;
  error: string;
  searchText: string;
  cardStyle: CSSProperties;

  onSearchChange: (
    value: string,
  ) => void;

  onRefresh: () => void;

  onPull: (
    model: string,
  ) => void;

  onDelete: (
    model: string,
  ) => void;

  onTest: (
    model: string,
    prompt: string,
  ) => Promise<string>;

  onInspect: (
    model: string,
  ) => Promise<string>;
};

function formatBytes(
  bytes: number,
): string {
  if (
    !Number.isFinite(bytes) ||
    bytes <= 0
  ) {
    return "0 B";
  }

  const units = [
    "B",
    "KB",
    "MB",
    "GB",
    "TB",
  ];

  const index = Math.min(
    Math.floor(
      Math.log(bytes) /
        Math.log(1024),
    ),
    units.length - 1,
  );

  const value =
    bytes /
    1024 ** index;

  return `${value.toFixed(
    index === 0 ? 0 : 1,
  )} ${units[index]}`;
}

function formatDate(
  value: string,
): string {
  const date =
    new Date(value);

  if (
    Number.isNaN(
      date.getTime(),
    )
  ) {
    return value;
  }

  return date.toLocaleString();
}

function progressPercent(
  progress:
    | OllamaPullProgress
    | null,
): number {
  if (
    !progress?.total ||
    !progress.completed
  ) {
    return 0;
  }

  return Math.min(
    Math.max(
      (progress.completed /
        progress.total) *
        100,
      0,
    ),
    100,
  );
}

function ModelsPage({
  models,
  totalSize,
  status,
  activeModel,
  pullProgress,
  error,
  searchText,
  cardStyle,
  onSearchChange,
  onRefresh,
  onPull,
  onDelete,
  onTest,
  onInspect,
}: ModelsPageProps) {
  const [
    modelInput,
    setModelInput,
  ] = useState("");

  const [
    confirmDelete,
    setConfirmDelete,
  ] = useState<string | null>(
    null,
  );

  const [
    selectedModel,
    setSelectedModel,
  ] = useState<string | null>(
    null,
  );

  const [
    testPrompt,
    setTestPrompt,
  ] = useState(
    "Reply with one short sentence confirming that the model is working.",
  );

  const [
    testResult,
    setTestResult,
  ] = useState("");

  const [
    modelDetails,
    setModelDetails,
  ] = useState("");

  const [
    modalError,
    setModalError,
  ] = useState("");

  const isBusy =
    status === "loading" ||
    status === "pulling" ||
    status === "deleting" ||
    status === "running";

  const selectedModelRecord =
    useMemo(
      () =>
        models.find(
          (model) =>
            model.name ===
            selectedModel,
        ) ?? null,
      [
        models,
        selectedModel,
      ],
    );

  const percent =
    progressPercent(
      pullProgress,
    );

  async function openModel(
    model: OllamaModel,
  ) {
    setSelectedModel(
      model.name,
    );

    setTestResult("");
    setModelDetails("");
    setModalError("");

    try {
      const details =
        await onInspect(
          model.name,
        );

      setModelDetails(
        details,
      );
    } catch (nextError) {
      setModalError(
        String(nextError),
      );
    }
  }

  async function runTest() {
    if (!selectedModel) {
      return;
    }

    try {
      setTestResult("");
      setModalError("");

      const result =
        await onTest(
          selectedModel,
          testPrompt,
        );

      setTestResult(
        result,
      );
    } catch (nextError) {
      setModalError(
        String(nextError),
      );
    }
  }

  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>
            Ollama Models
          </h2>

          <p>
            Download, inspect, test and
            remove local AI models.
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          disabled={isBusy}
          onClick={onRefresh}
        >
          {status === "loading"
            ? "Refreshing..."
            : "↻ Refresh"}
        </button>
      </div>

      <div className="models-summary-grid">
        <div
          className="models-summary-card"
          style={cardStyle}
        >
          <span>
            Installed Models
          </span>

          <strong>
            {models.length}
          </strong>
        </div>

        <div
          className="models-summary-card"
          style={cardStyle}
        >
          <span>
            Storage Used
          </span>

          <strong>
            {formatBytes(
              totalSize,
            )}
          </strong>
        </div>

        <div
          className="models-summary-card"
          style={cardStyle}
        >
          <span>
            Runtime
          </span>

          <strong>
            Ollama
          </strong>
        </div>
      </div>

      <div
        className="models-download-card"
        style={cardStyle}
      >
        <div className="models-download-heading">
          <div>
            <h3>
              Download Model
            </h3>

            <p>
              Enter an Ollama model tag,
              such as llama3.2:3b.
            </p>
          </div>

          <span>
            ⬇️
          </span>
        </div>

        <div className="models-download-form">
          <input
            type="text"
            value={modelInput}
            disabled={isBusy}
            placeholder="llama3.2:3b"
            onChange={(
              event,
            ) =>
              setModelInput(
                event.target.value,
              )
            }
            onKeyDown={(
              event,
            ) => {
              if (
                event.key ===
                "Enter"
              ) {
                onPull(
                  modelInput,
                );
              }
            }}
          />

          <button
            type="button"
            className="action-button backup-button"
            disabled={
              isBusy ||
              !modelInput.trim()
            }
            onClick={() =>
              onPull(modelInput)
            }
          >
            {status === "pulling"
              ? "Downloading..."
              : "Download"}
          </button>
        </div>

        <div className="popular-models">
          <span>
            Popular:
          </span>

          {POPULAR_OLLAMA_MODELS.map(
            (model) => (
              <button
                key={model.name}
                type="button"
                className="model-suggestion"
                disabled={isBusy}
                title={
                  model.description
                }
                onClick={() =>
                  setModelInput(
                    model.name,
                  )
                }
              >
                {model.name}
              </button>
            ),
          )}
        </div>

        {status === "pulling" &&
          pullProgress && (
            <div className="model-download-progress">
              <div className="model-progress-header">
                <span>
                  {pullProgress.status}
                </span>

                {percent > 0 && (
                  <strong>
                    {percent.toFixed(
                      1,
                    )}
                    %
                  </strong>
                )}
              </div>

              <div className="metric-track">
                <div
                  className="metric-progress"
                  style={{
                    width:
                      percent > 0
                        ? `${percent}%`
                        : "8%",

                    background:
                      "#3b82f6",
                  }}
                />
              </div>
            </div>
          )}

        {error && (
          <div
            className="models-error"
            role="alert"
          >
            {error}
          </div>
        )}
      </div>

      <div className="section-header models-list-header">
        <div>
          <h2>
            Installed Models
          </h2>

          <p>
            Models currently available
            to Ollama.
          </p>
        </div>

        <input
          type="search"
          className="models-search"
          value={searchText}
          placeholder="Search models..."
          onChange={(
            event,
          ) =>
            onSearchChange(
              event.target.value,
            )
          }
        />
      </div>

      {models.length === 0 ? (
        <div
          className="models-empty-state"
          style={cardStyle}
        >
          <span>
            {searchText.trim() ? "🔍" : "🧠"}
          </span>

          <h3>
            {searchText.trim()
              ? `No models match "${searchText.trim()}"`
              : "No models installed"}
          </h3>

          <p>
            {searchText.trim()
              ? "Try another search term or clear the current search."
              : "Download a model to begin using local AI."}
          </p>

          {searchText.trim() && (
            <button
              type="button"
              className="secondary-button"
              onClick={() => onSearchChange("")}
            >
              Clear Search
            </button>
          )}
        </div>
      ) : (
        <div className="models-grid">
          {models.map(
            (model) => {
              const busy =
                activeModel ===
                model.name;

              return (
                <article
                  key={model.name}
                  className="model-card"
                  style={cardStyle}
                >
                  <div className="model-card-header">
                    <span className="model-card-icon">
                      🧠
                    </span>

                    <div>
                      <h3>
                        {model.name}
                      </h3>

                      <p>
                        {model.details
                          ?.family ??
                          "Ollama model"}
                      </p>
                    </div>
                  </div>

                  <div className="model-meta-grid">
                    <div>
                      <span>
                        Size
                      </span>

                      <strong>
                        {formatBytes(
                          model.size,
                        )}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Parameters
                      </span>

                      <strong>
                        {model.details
                          ?.parameterSize ??
                          "Unknown"}
                      </strong>
                    </div>

                    <div>
                      <span>
                        Quantization
                      </span>

                      <strong>
                        {model.details
                          ?.quantizationLevel ??
                          "Unknown"}
                      </strong>
                    </div>
                  </div>

                  <small className="model-modified">
                    Updated{" "}
                    {formatDate(
                      model.modifiedAt,
                    )}
                  </small>

                  <div className="model-card-actions">
                    <button
                      type="button"
                      className="action-button health-button"
                      disabled={
                        isBusy
                      }
                      onClick={() =>
                        openModel(
                          model,
                        )
                      }
                    >
                      {busy &&
                      status ===
                        "loading"
                        ? "Opening..."
                        : "Test"}
                    </button>

                    <button
                      type="button"
                      className="danger-button"
                      disabled={isBusy}
                      onClick={() =>
                        setConfirmDelete(
                          model.name,
                        )
                      }
                    >
                      Delete
                    </button>
                  </div>
                </article>
              );
            },
          )}
        </div>
      )}

      {selectedModelRecord && (
        <div
          className="model-modal-backdrop"
          role="presentation"
          onMouseDown={(
            event,
          ) => {
            if (
              event.target ===
              event.currentTarget
            ) {
              setSelectedModel(
                null,
              );
            }
          }}
        >
          <div
            className="model-modal"
            style={cardStyle}
            role="dialog"
            aria-modal="true"
            aria-label={`Test ${selectedModelRecord.name}`}
          >
            <div className="model-modal-header">
              <div>
                <h3>
                  {
                    selectedModelRecord.name
                  }
                </h3>

                <p>
                  Test this model with a
                  local prompt.
                </p>
              </div>

              <button
                type="button"
                className="secondary-button"
                disabled={
                  status ===
                  "running"
                }
                onClick={() =>
                  setSelectedModel(
                    null,
                  )
                }
              >
                Close
              </button>
            </div>

            <label className="setting-field">
              <span>
                Test Prompt
              </span>

              <textarea
                value={testPrompt}
                disabled={
                  status ===
                  "running"
                }
                rows={5}
                onChange={(
                  event,
                ) =>
                  setTestPrompt(
                    event.target
                      .value,
                  )
                }
              />
            </label>

            <button
              type="button"
              className="action-button health-button"
              disabled={
                status ===
                  "running" ||
                !testPrompt.trim()
              }
              onClick={runTest}
            >
              {status === "running"
                ? "Running..."
                : "Run Test"}
            </button>

            {modelDetails && (
              <details className="model-details">
                <summary>
                  Model Details
                </summary>

                <pre>
                  {modelDetails}
                </pre>
              </details>
            )}

            {testResult && (
              <div className="model-test-result">
                <strong>
                  Response
                </strong>

                <pre>
                  {testResult}
                </pre>
              </div>
            )}

            {modalError && (
              <div
                className="models-error"
                role="alert"
              >
                {modalError}
              </div>
            )}
          </div>
        </div>
      )}
      <ConfirmDialog
        open={confirmDelete !== null}
        title="Delete model?"
        message={
          confirmDelete
            ? `This will permanently delete "${confirmDelete}". This action cannot be undone.`
            : ""
        }
        confirmLabel="Confirm Delete"
        busy={
          confirmDelete !== null &&
          status === "deleting" &&
          activeModel === confirmDelete
        }
        onCancel={() =>
          setConfirmDelete(null)
        }
        onConfirm={() => {
          if (!confirmDelete) {
            return;
          }

          onDelete(confirmDelete);
          setConfirmDelete(null);
        }}
      />
    </section>
  );
}

export default ModelsPage;