import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  deleteOpenClawServer,
  duplicateOpenClawServer,
  exportOpenClawServers,
  getActiveOpenClawStatus,
  getOpenClawDashboardSummary,
  getOpenClawRuntimeConfig,
  importOpenClawServers,
  invokeActiveOpenClawGateway,
  listOpenClawServers,
  saveOpenClawServer,
  setActiveOpenClawServer,
  testAllOpenClawServers,
  testOpenClawConnection,
  testOpenClawConnectionInput,
  toggleOpenClawServer,
  updateOpenClawServer,
} from "../services/openclaw";

import type {
  AsyncStatus,
  OpenClawConnectionResult,
  OpenClawDashboardSummary,
  OpenClawGatewayRequest,
  OpenClawGatewayResponse,
  OpenClawImportResult,
  OpenClawRemoteStatus,
  OpenClawRuntimeConfig,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

type UseOpenClawOptions = {
  refreshInterval: number;

  onMessage: (
    message: string,
  ) => void;
};

type ImportOptions = {
  json: string;
  replaceExisting: boolean;
};

function errorText(
  value: unknown,
): string {
  if (
    value instanceof Error
  ) {
    return value.message;
  }

  return String(value);
}

async function copyText(
  value: string,
): Promise<void> {
  if (
    navigator.clipboard
    && window.isSecureContext
  ) {
    await navigator.clipboard.writeText(
      value,
    );

    return;
  }

  const textarea =
    document.createElement(
      "textarea",
    );

  textarea.value = value;
  textarea.style.position =
    "fixed";
  textarea.style.opacity =
    "0";
  textarea.style.pointerEvents =
    "none";

  document.body.appendChild(
    textarea,
  );

  textarea.focus();
  textarea.select();

  const copied =
    document.execCommand(
      "copy",
    );

  textarea.remove();

  if (!copied) {
    throw new Error(
      "Clipboard access is unavailable.",
    );
  }
}

function useOpenClaw({
  refreshInterval,
  onMessage,
}: UseOpenClawOptions) {
  const [
    servers,
    setServers,
  ] = useState<
    OpenClawServer[]
  >([]);

  const [
    status,
    setStatus,
  ] = useState<AsyncStatus>(
    "idle",
  );

  const [
    activeServerId,
    setActiveServerId,
  ] = useState<
    string | null
  >(null);

  const [
    busyServerId,
    setBusyServerId,
  ] = useState<
    string | null
  >(null);

  const [
    testingServerId,
    setTestingServerId,
  ] = useState<
    string | null
  >(null);

  const [
    isTestingAll,
    setIsTestingAll,
  ] = useState(false);

  const [
    isImporting,
    setIsImporting,
  ] = useState(false);

  const [
    isExporting,
    setIsExporting,
  ] = useState(false);

  const [
    searchText,
    setSearchText,
  ] = useState("");

  const [
    error,
    setError,
  ] = useState("");

  const [
    remoteStatus,
    setRemoteStatus,
  ] = useState<
    OpenClawRemoteStatus | null
  >(null);

  const [
    dashboardSummary,
    setDashboardSummary,
  ] = useState<
    OpenClawDashboardSummary | null
  >(null);

  const [
    runtimeConfig,
    setRuntimeConfig,
  ] = useState<
    OpenClawRuntimeConfig | null
  >(null);

  const [
    lastExportJson,
    setLastExportJson,
  ] = useState("");

  const isMountedRef =
    useRef(true);

  const activeStatusRequestRef =
    useRef(false);

  const healthFailureCountRef =
    useRef(0);

  useEffect(() => {
    isMountedRef.current =
      true;

    return () => {
      isMountedRef.current =
        false;
    };
  }, []);

  const refreshDashboardSummary =
    useCallback(
      async () => {
        try {
          const result =
            await getOpenClawDashboardSummary();

          if (
            isMountedRef.current
          ) {
            setDashboardSummary(
              result,
            );
          }

          return result;
        } catch (
          nextError
        ) {
          if (
            isMountedRef.current
          ) {
            setDashboardSummary(
              null,
            );
          }

          throw nextError;
        }
      },
      [],
    );

  const refreshRuntimeConfig =
    useCallback(
      async () => {
        try {
          const result =
            await getOpenClawRuntimeConfig();

          if (
            isMountedRef.current
          ) {
            setRuntimeConfig(
              result,
            );
          }

          return result;
        } catch (
          nextError
        ) {
          if (
            isMountedRef.current
          ) {
            setRuntimeConfig(
              null,
            );
          }

          throw nextError;
        }
      },
      [],
    );

  const refreshServers =
    useCallback(
      async (
        showLoading = true,
      ) => {
        try {
          if (
            showLoading
          ) {
            setStatus(
              "loading",
            );
          }

          setError("");

          const result =
            await listOpenClawServers();

          if (
            !isMountedRef.current
          ) {
            return result;
          }

          setServers(
            result,
          );

          const active =
            result.find(
              (
                server,
              ) =>
                server.active,
            );

          setActiveServerId(
            active?.id
              ?? null,
          );

          setStatus(
            "success",
          );

          return result;
        } catch (
          nextError
        ) {
          const message =
            `Unable to load OpenClaw servers: ${errorText(
              nextError,
            )}`;

          if (
            isMountedRef.current
          ) {
            setStatus(
              "error",
            );

            setError(
              message,
            );
          }

          throw new Error(
            message,
          );
        }
      },
      [],
    );

  const refreshAllMetadata =
    useCallback(
      async (
        showLoading = false,
      ) => {
        const tasks = [
          refreshServers(
            showLoading,
          ),
          refreshDashboardSummary(),
          refreshRuntimeConfig(),
        ];

        const results =
          await Promise.allSettled(
            tasks,
          );

        const rejected =
          results.find(
            (
              result,
            ) =>
              result.status
              === "rejected",
          );

        if (
          rejected
          && rejected.status
          === "rejected"
        ) {
          throw rejected.reason;
        }
      },
      [
        refreshDashboardSummary,
        refreshRuntimeConfig,
        refreshServers,
      ],
    );

  const refreshActiveStatus =
    useCallback(
      async (
        silent = true,
      ) => {
        if (
          activeStatusRequestRef
            .current
        ) {
          return null;
        }

        activeStatusRequestRef.current =
          true;

        try {
          if (
            !silent
          ) {
            setError("");
          }

          const result =
            await getActiveOpenClawStatus();

          healthFailureCountRef.current =
            result.connected
              ? 0
              : healthFailureCountRef.current + 1;  

          if (
            isMountedRef.current
          ) {
            setRemoteStatus(
              result,
            );
          }

          await Promise.allSettled([
            refreshServers(
              false,
            ),
            refreshDashboardSummary(),
            refreshRuntimeConfig(),
          ]);

          return result;
        } catch (
          nextError
        ) {
          healthFailureCountRef.current += 1;
          
          if (
            isMountedRef.current
          ) {
            setRemoteStatus(
              null,
            );
          }

          if (
            !silent
          ) {
            const message =
              `Unable to refresh active OpenClaw status: ${errorText(
                nextError,
              )}`;

            setError(
              message,
            );

            onMessage(
              `❌ ${message}`,
            );
          }

          return null;
        } finally {
          activeStatusRequestRef.current =
            false;
        }
      },
      [
        onMessage,
        refreshDashboardSummary,
        refreshRuntimeConfig,
        refreshServers,
      ],
    );

  useEffect(() => {
    void refreshAllMetadata(
      true,
    ).catch(() => {
      // 错误已写入 Hook 状态。
    });
  }, [
    refreshAllMetadata,
  ]);

  useEffect(() => {
    const active =
      servers.find(
        (
          server,
        ) =>
          server.active
          && server.enabled
          && server.autoConnect,
      );

    if (!active) {
      setRemoteStatus(
        null,
      );

      return undefined;
    }

    void refreshActiveStatus(
      true,
    );

    const interval =
      window.setInterval(
        () => {
          void refreshActiveStatus(
            true,
          );
        },
        Math.max(
          refreshInterval,
          5,
        ) * 1000,
      );

    return () => {
      window.clearInterval(
        interval,
      );
    };
  }, [
    refreshActiveStatus,
    refreshInterval,
    servers,
  ]);

  const createServer =
    useCallback(
      async (
        server:
          OpenClawServerInput,
      ): Promise<
        OpenClawServer | null
      > => {
        try {
          setStatus(
            "loading",
          );

          setError("");

          const result =
            await saveOpenClawServer(
              server,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          onMessage(
            `✅ ${result.message}`,
          );

          return (
            result.server
            ?? null
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to add OpenClaw server: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          setStatus(
            "error",
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setStatus(
              (
                current,
              ) =>
                current ===
                "error"
                  ? "error"
                  : "success",
            );
          }
        }
      },
      [
        onMessage,
        refreshAllMetadata,
      ],
    );

  const editServer =
    useCallback(
      async (
        id: string,
        server:
          OpenClawServerInput,
      ): Promise<
        OpenClawServer | null
      > => {
        try {
          setBusyServerId(
            id,
          );

          setStatus(
            "loading",
          );

          setError("");

          const result =
            await updateOpenClawServer(
              id,
              server,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          onMessage(
            `✅ ${result.message}`,
          );

          return (
            result.server
            ?? null
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to update OpenClaw server: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          setStatus(
            "error",
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setBusyServerId(
              null,
            );

            setStatus(
              (
                current,
              ) =>
                current ===
                "error"
                  ? "error"
                  : "success",
            );
          }
        }
      },
      [
        onMessage,
        refreshAllMetadata,
      ],
    );

  const removeServer =
    useCallback(
      async (
        id: string,
      ) => {
        try {
          setBusyServerId(
            id,
          );

          setStatus(
            "loading",
          );

          setError("");

          const result =
            await deleteOpenClawServer(
              id,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          await refreshActiveStatus(
            true,
          );

          onMessage(
            `🗑️ ${result.message}`,
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to delete OpenClaw server: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          setStatus(
            "error",
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setBusyServerId(
              null,
            );

            setStatus(
              (
                current,
              ) =>
                current ===
                "error"
                  ? "error"
                  : "success",
            );
          }
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const duplicateServer =
    useCallback(
      async (
        id: string,
      ): Promise<
        OpenClawServer | null
      > => {
        try {
          setBusyServerId(
            id,
          );

          setError("");

          const result =
            await duplicateOpenClawServer(
              id,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          onMessage(
            `📋 ${result.message}`,
          );

          return (
            result.server
            ?? null
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to duplicate OpenClaw server: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setBusyServerId(
              null,
            );
          }
        }
      },
      [
        onMessage,
        refreshAllMetadata,
      ],
    );

  const setServerEnabled =
    useCallback(
      async (
        id: string,
        enabled: boolean,
      ) => {
        try {
          setBusyServerId(
            id,
          );

          setError("");

          const result =
            await toggleOpenClawServer(
              id,
              enabled,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          await refreshActiveStatus(
            true,
          );

          onMessage(
            `🔌 ${result.message}`,
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to change OpenClaw server status: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setBusyServerId(
              null,
            );
          }
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const activateServer =
    useCallback(
      async (
        id: string,
      ) => {
        try {
          setBusyServerId(
            id,
          );

          setError("");

          const result =
            await setActiveOpenClawServer(
              id,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          await refreshActiveStatus(
            true,
          );

          onMessage(
            `⭐ ${result.message}`,
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to activate OpenClaw server: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setBusyServerId(
              null,
            );
          }
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const testSavedServer =
    useCallback(
      async (
        id: string,
      ): Promise<
        OpenClawConnectionResult
      > => {
        try {
          setTestingServerId(
            id,
          );

          setError("");

          const result =
            await testOpenClawConnection(
              id,
            );

          await refreshAllMetadata(
            false,
          );

          if (
            id
            === activeServerId
          ) {
            await refreshActiveStatus(
              true,
            );
          }

          onMessage(
            result.success
              ? `✅ ${result.message}`
              : `❌ ${result.message}`,
          );

          return result;
        } catch (
          nextError
        ) {
          const message =
            `OpenClaw connection test failed: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setTestingServerId(
              null,
            );
          }
        }
      },
      [
        activeServerId,
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const testUnsavedServer =
    useCallback(
      async (
        server:
          OpenClawServerInput,
      ): Promise<
        OpenClawConnectionResult
      > => {
        try {
          setTestingServerId(
            "__new__",
          );

          setError("");

          const result =
            await testOpenClawConnectionInput(
              server,
            );

          onMessage(
            result.success
              ? `✅ ${result.message}`
              : `❌ ${result.message}`,
          );

          return result;
        } catch (
          nextError
        ) {
          const message =
            `OpenClaw connection test failed: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setTestingServerId(
              null,
            );
          }
        }
      },
      [
        onMessage,
      ],
    );

  const testAllServers =
    useCallback(
      async (): Promise<
        OpenClawConnectionResult[]
      > => {
        try {
          setIsTestingAll(
            true,
          );

          setError("");

          const results =
            await testAllOpenClawServers();

          await refreshAllMetadata(
            false,
          );

          await refreshActiveStatus(
            true,
          );

          const successful =
            results.filter(
              (
                result,
              ) =>
                result.success,
            ).length;

          const failed =
            results.length
            - successful;

          onMessage(
            failed === 0
              ? `✅ Tested ${results.length} OpenClaw server(s); all connected.`
              : `⚠️ Tested ${results.length} server(s): ${successful} connected, ${failed} failed.`,
          );

          return results;
        } catch (
          nextError
        ) {
          const message =
            `Unable to test all OpenClaw servers: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setIsTestingAll(
              false,
            );
          }
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const copyServerUrl =
    useCallback(
      async (
        server:
          OpenClawServer,
      ) => {
        try {
          await copyText(
            server.serverUrl,
          );

          onMessage(
            `📋 Copied ${server.name} URL.`,
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to copy server URL: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        }
      },
      [
        onMessage,
      ],
    );

  const exportServers =
    useCallback(
      async (
        includeSecrets = false,
      ): Promise<string> => {
        try {
          setIsExporting(
            true,
          );

          setError("");

          const result =
            await exportOpenClawServers(
              includeSecrets,
            );

          if (
            !result.success
            || !result.json
          ) {
            throw new Error(
              result.message,
            );
          }

          setLastExportJson(
            result.json,
          );

          onMessage(
            `✅ ${result.message}${
              includeSecrets
                ? " Export includes Gateway Tokens."
                : " Tokens were excluded."
            }`,
          );

          return result.json;
        } catch (
          nextError
        ) {
          const message =
            `Unable to export OpenClaw servers: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setIsExporting(
              false,
            );
          }
        }
      },
      [
        onMessage,
      ],
    );

  const copyExportJson =
    useCallback(
      async (
        includeSecrets = false,
      ): Promise<string> => {
        const json =
          await exportServers(
            includeSecrets,
          );

        await copyText(
          json,
        );

        onMessage(
          includeSecrets
            ? "📋 Export JSON copied with Gateway Tokens."
            : "📋 Export JSON copied without Gateway Tokens.",
        );

        return json;
      },
      [
        exportServers,
        onMessage,
      ],
    );

  const importServers =
    useCallback(
      async ({
        json,
        replaceExisting,
      }: ImportOptions): Promise<
        OpenClawImportResult
      > => {
        try {
          if (
            !json.trim()
          ) {
            throw new Error(
              "Import JSON is empty.",
            );
          }

          setIsImporting(
            true,
          );

          setError("");

          const result =
            await importOpenClawServers(
              json,
              replaceExisting,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          await refreshAllMetadata(
            false,
          );

          await refreshActiveStatus(
            true,
          );

          onMessage(
            `✅ ${result.message}`,
          );

          return result;
        } catch (
          nextError
        ) {
          const message =
            `Unable to import OpenClaw servers: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );

          throw new Error(
            message,
          );
        } finally {
          if (
            isMountedRef.current
          ) {
            setIsImporting(
              false,
            );
          }
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshAllMetadata,
      ],
    );

  const invokeGateway =
    useCallback(
      async <
        T = unknown,
      >(
        request:
          OpenClawGatewayRequest,
      ): Promise<
        OpenClawGatewayResponse<T>
      > => {
        try {
          setError("");

          const result =
            await invokeActiveOpenClawGateway<T>(
              request,
            );

          if (
            !result.success
          ) {
            throw new Error(
              result.message,
            );
          }

          return result;
        } catch (
          nextError
        ) {
          const message =
            `OpenClaw Gateway request failed: ${errorText(
              nextError,
            )}`;

          setError(
            message,
          );

          throw new Error(
            message,
          );
        }
      },
      [],
    );

  const filteredServers =
    useMemo(() => {
      const search =
        searchText
          .trim()
          .toLowerCase();

      if (!search) {
        return servers;
      }

      return servers.filter(
        (
          server,
        ) => {
          const values = [
            server.name,
            server.serverUrl,
            server.connectionState,
            server.connectionMessage,
            server.version
              ?? "",
            server.gatewayId
              ?? "",
          ];

          return values.some(
            (
              value,
            ) =>
              value
                .toLowerCase()
                .includes(
                  search,
                ),
          );
        },
      );
    }, [
      searchText,
      servers,
    ]);

  const activeServer =
    useMemo(
      () =>
        servers.find(
          (
            server,
          ) =>
            server.active,
        ) ?? null,
      [
        servers,
      ],
    );

  const connectedCount =
    useMemo(
      () =>
        servers.filter(
          (
            server,
          ) =>
            server
              .connectionState
            === "connected",
        ).length,
      [
        servers,
      ],
    );

  const enabledCount =
    useMemo(
      () =>
        servers.filter(
          (
            server,
          ) =>
            server.enabled,
        ).length,
      [
        servers,
      ],
    );

  const autoConnectCount =
    useMemo(
      () =>
        servers.filter(
          (
            server,
          ) =>
            server.enabled
            && server.autoConnect,
        ).length,
      [
        servers,
      ],
    );

  const averageLatencyMs =
    useMemo(() => {
      const latencies =
  servers
    .filter(
      (server) =>
        server.connectionState === "connected" &&
        typeof server.latencyMs === "number",
    )
    .map(
      (server) => server.latencyMs ?? 0,
    );

      if (
        latencies.length
        === 0
      ) {
        return null;
      }

      const total =
        latencies.reduce(
          (
            sum,
            value,
          ) =>
            sum + value,
          0,
        );

      return Math.round(
        total
        / latencies.length,
      );
    }, [
      servers,
    ]);

  const isBusy =
    status === "loading"
    || busyServerId
      !== null
    || testingServerId
      !== null
    || isTestingAll
    || isImporting
    || isExporting;

  return {
    servers,
    filteredServers,

    activeServer,
    activeServerId,
    busyServerId,
    testingServerId,

    connectedCount,
    enabledCount,
    autoConnectCount,
    averageLatencyMs,

    remoteStatus,
    dashboardSummary,
    runtimeConfig,

    status,
    isBusy,
    isTestingAll,
    isImporting,
    isExporting,

    searchText,
    error,
    lastExportJson,

    setSearchText,
    setError,

    refreshServers,
    refreshAllMetadata,
    refreshActiveStatus,
    refreshDashboardSummary,
    refreshRuntimeConfig,

    createServer,
    editServer,
    removeServer,
    duplicateServer,

    setServerEnabled,
    activateServer,

    testSavedServer,
    testUnsavedServer,
    testAllServers,

    copyServerUrl,

    exportServers,
    copyExportJson,
    importServers,

    invokeGateway,
  };
}

export default useOpenClaw;