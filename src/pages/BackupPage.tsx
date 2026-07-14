import {
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import ConfirmDialog from "../components/ConfirmDialog";
import type {
  BackupRecord,
  BackupStatus,
  Settings,
} from "../types/index";

type BackupPageProps = {
  settings: Settings;
  backups: BackupRecord[];
  status: BackupStatus;
  selectedBackup: string | null;
  error: string;
  cardStyle: CSSProperties;

  onUpdateSetting: <
    K extends keyof Settings,
  >(
    key: K,
    value: Settings[K],
  ) => void;

  onCreateBackup: () => void;

  onRestoreBackup: (
    archivePath: string,
    restoreOpenClawConfig: boolean,
    restoreAiOsSettings: boolean,
  ) => void;

  onRefresh: () => void;

  onReveal: (
    archivePath: string,
  ) => void;

  onDelete: (
    archivePath: string,
  ) => void;
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

function BackupPage({
  settings,
  backups,
  status,
  selectedBackup,
  error,
  cardStyle,
  onUpdateSetting,
  onCreateBackup,
  onRestoreBackup,
  onRefresh,
  onReveal,
  onDelete,
}: BackupPageProps) {
  const [
    restoreOpenClaw,
    setRestoreOpenClaw,
  ] = useState(true);

  const [
    restoreSettings,
    setRestoreSettings,
  ] = useState(true);

  const [
    confirmingDelete,
    setConfirmingDelete,
  ] = useState<string | null>(
    null,
  );

  const isCreating =
    status === "creating";

  const isRestoring =
    status === "restoring";

  const totalSize =
    useMemo(
      () =>
        backups.reduce(
          (
            total,
            backup,
          ) =>
            total +
            backup.sizeBytes,
          0,
        ),
      [backups],
    );

  return (
    <section className="page-section">
      <div className="section-header">
        <div>
          <h2>
            Backup & Restore
          </h2>

          <p>
            Protect your OpenClaw
            configuration and AI OS
            settings.
          </p>
        </div>

        <button
          type="button"
          className="secondary-button"
          disabled={
            isCreating ||
            isRestoring
          }
          onClick={onRefresh}
        >
          ↻ Refresh
        </button>
      </div>

      <div className="backup-layout">
        <div
          className="settings-card backup-create-card"
          style={cardStyle}
        >
          <div className="backup-card-heading">
            <div>
              <h3>
                Create Backup
              </h3>

              <p>
                Backups are saved as
                timestamped archives.
                Ollama model files are
                excluded.
              </p>
            </div>

            <span className="backup-card-icon">
              💾
            </span>
          </div>

          <label className="setting-field">
            <span>
              Destination Directory
            </span>

            <small>
              Enter an absolute folder
              path, for example
              /Users/your-name/Backups.
            </small>

            <input
              type="text"
              value={
                settings
                  .backupDirectory
              }
              placeholder="/Users/your-name/Backups"
              disabled={
                isCreating ||
                isRestoring
              }
              onChange={(
                event,
              ) =>
                onUpdateSetting(
                  "backupDirectory",
                  event.target.value,
                )
              }
            />
          </label>

          <div className="backup-options">
            <label className="backup-checkbox-row">
              <input
                type="checkbox"
                checked={
                  settings
                    .includeOpenClawConfig
                }
                disabled={
                  isCreating ||
                  isRestoring
                }
                onChange={(
                  event,
                ) =>
                  onUpdateSetting(
                    "includeOpenClawConfig",
                    event.target
                      .checked,
                  )
                }
              />

              <span>
                <strong>
                  OpenClaw
                  configuration
                </strong>

                <small>
                  Includes the local
                  OpenClaw config folder
                  when it exists.
                </small>
              </span>
            </label>

            <label className="backup-checkbox-row">
              <input
                type="checkbox"
                checked={
                  settings
                    .includeAiOsSettings
                }
                disabled={
                  isCreating ||
                  isRestoring
                }
                onChange={(
                  event,
                ) =>
                  onUpdateSetting(
                    "includeAiOsSettings",
                    event.target
                      .checked,
                  )
                }
              />

              <span>
                <strong>
                  AI OS settings
                </strong>

                <small>
                  Includes refresh,
                  URLs, theme and
                  backup preferences.
                </small>
              </span>
            </label>
          </div>

          <button
            type="button"
            className="action-button backup-button backup-primary-button"
            disabled={
              isCreating ||
              isRestoring
            }
            onClick={
              onCreateBackup
            }
          >
            {isCreating
              ? "⏳ Creating Backup..."
              : "💾 Create Backup"}
          </button>

          {error && (
            <div
              className="backup-error"
              role="alert"
            >
              {error}
            </div>
          )}
        </div>

        <div
          className="settings-card backup-summary-card"
          style={cardStyle}
        >
          <div className="backup-card-heading">
            <div>
              <h3>
                Backup Summary
              </h3>

              <p>
                Local backup archive
                overview.
              </p>
            </div>

            <span className="backup-card-icon">
              📦
            </span>
          </div>

          <div className="backup-summary-grid">
            <div className="backup-summary-item">
              <span>
                Archives
              </span>

              <strong>
                {backups.length}
              </strong>
            </div>

            <div className="backup-summary-item">
              <span>
                Total Size
              </span>

              <strong>
                {formatBytes(
                  totalSize,
                )}
              </strong>
            </div>

            <div className="backup-summary-item">
              <span>
                OpenClaw
              </span>

              <strong>
                {settings
                  .includeOpenClawConfig
                  ? "Included"
                  : "Excluded"}
              </strong>
            </div>

            <div className="backup-summary-item">
              <span>
                AI OS
              </span>

              <strong>
                {settings
                  .includeAiOsSettings
                  ? "Included"
                  : "Excluded"}
              </strong>
            </div>
          </div>

          <div className="backup-note">
            <strong>
              Restore options
            </strong>

            <label className="backup-inline-option">
              <input
                type="checkbox"
                checked={
                  restoreOpenClaw
                }
                onChange={(
                  event,
                ) =>
                  setRestoreOpenClaw(
                    event.target
                      .checked,
                  )
                }
              />

              Restore OpenClaw
              configuration
            </label>

            <label className="backup-inline-option">
              <input
                type="checkbox"
                checked={
                  restoreSettings
                }
                onChange={(
                  event,
                ) =>
                  setRestoreSettings(
                    event.target
                      .checked,
                  )
                }
              />

              Restore AI OS settings
            </label>
          </div>
        </div>
      </div>

      <div className="section-header backup-history-header">
        <div>
          <h2>
            Backup History
          </h2>

          <p>
            Restore, reveal or delete
            existing archives.
          </p>
        </div>
      </div>

      {backups.length === 0 ? (
        <div
          className="backup-empty-state"
          style={cardStyle}
        >
          <span>
            🗂️
          </span>

          <h3>
            No backups found
          </h3>

          <p>
            Enter a destination
            directory and create your
            first backup.
          </p>
        </div>
      ) : (
        <div className="backup-list">
          {backups.map(
            (backup) => {
              const busy =
                selectedBackup ===
                backup.path;

              const deleting =
                confirmingDelete ===
                backup.path;

              return (
                <article
                  key={backup.id}
                  className="backup-record"
                  style={cardStyle}
                >
                  <div className="backup-record-main">
                    <span className="backup-record-icon">
                      📦
                    </span>

                    <div>
                      <h3>
                        {
                          backup.fileName
                        }
                      </h3>

                      <p>
                        {formatDate(
                          backup.createdAt,
                        )}
                      </p>

                      <small>
                        {formatBytes(
                          backup.sizeBytes,
                        )}
                      </small>
                    </div>
                  </div>

                  <div className="backup-record-actions">
                    <button
                      type="button"
                      className="secondary-button"
                      disabled={
                        busy ||
                        isCreating ||
                        isRestoring
                      }
                      onClick={() =>
                        onReveal(
                          backup.path,
                        )
                      }
                    >
                      Show
                    </button>

                    <button
                      type="button"
                      className="action-button health-button"
                      disabled={
                        busy ||
                        isCreating ||
                        isRestoring ||
                        (!restoreOpenClaw &&
                          !restoreSettings)
                      }
                      onClick={() =>
                        onRestoreBackup(
                          backup.path,
                          restoreOpenClaw,
                          restoreSettings,
                        )
                      }
                    >
                      {busy &&
                      isRestoring
                        ? "Restoring..."
                        : "Restore"}
                    </button>

                    {deleting ? (
                      <>
                        <button
                          type="button"
                          className="danger-button"
                          disabled={
                            busy
                          }
                          onClick={() => {
                            onDelete(
                              backup.path,
                            );

                            setConfirmingDelete(
                              null,
                            );
                          }}
                        >
                          Confirm Delete
                        </button>

                        <button
                          type="button"
                          className="secondary-button"
                          onClick={() =>
                            setConfirmingDelete(
                              null,
                            )
                          }
                        >
                          Cancel
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        className="danger-button"
                        disabled={
                          busy ||
                          isCreating ||
                          isRestoring
                        }
                        onClick={() =>
                          setConfirmingDelete(
                            backup.path,
                          )
                        }
                      >
                        Delete
                      </button>
                    )}
                  </div>
                </article>
              );
            },
          )}
        </div>
      )}

      <ConfirmDialog
        open={confirmingDelete !== null}
        title="Delete backup?"
        message={
          confirmingDelete
            ? `This will permanently delete "${
                backups.find(
                  (backup) =>
                    backup.path ===
                    confirmingDelete,
                )?.fileName ??
                "this backup"
              }". This action cannot be undone.`
            : ""
        }
        confirmLabel="Confirm Delete"
        busy={
          confirmingDelete !== null &&
          selectedBackup ===
            confirmingDelete
        }
        onCancel={() =>
          setConfirmingDelete(null)
        }
        onConfirm={() => {
          if (!confirmingDelete) {
            return;
          }

          onDelete(confirmingDelete);
          setConfirmingDelete(null);
        }}
      />
    </section>
  );
}

export default BackupPage;