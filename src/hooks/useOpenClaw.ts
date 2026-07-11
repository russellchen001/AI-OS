import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import {
  deleteOpenClawServer,
  getActiveOpenClawStatus,
  listOpenClawServers,
  saveOpenClawServer,
  setActiveOpenClawServer,
  testOpenClawConnection,
  testOpenClawConnectionInput,
  toggleOpenClawServer,
  updateOpenClawServer,
} from "../services/openclaw";

import type {
  AsyncStatus,
  OpenClawConnectionResult,
  OpenClawRemoteStatus,
  OpenClawServer,
  OpenClawServerInput,
} from "../types/index";

type UseOpenClawOptions = {
  refreshInterval: number;

  onMessage: (
    message: string,
  ) => void;
};

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

  const refreshServers =
    useCallback(
      async () => {
        try {
          setStatus(
            "loading",
          );

          setError("");

          const result =
            await listOpenClawServers();

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
            active?.id ??
              null,
          );

          setStatus(
            "success",
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to load OpenClaw servers: ${String(
              nextError,
            )}`;

          setStatus(
            "error",
          );

          setError(
            message,
          );
        }
      },
      [],
    );

  const refreshActiveStatus =
    useCallback(
      async () => {
        try {
          const result =
            await getActiveOpenClawStatus();

          setRemoteStatus(
            result,
          );
        } catch {
          setRemoteStatus(
            null,
          );
        }
      },
      [],
    );

  useEffect(() => {
    refreshServers();
  }, [refreshServers]);

  useEffect(() => {
    const active =
      servers.find(
        (
          server,
        ) =>
          server.active &&
          server.enabled &&
          server.autoConnect,
      );

    if (!active) {
      setRemoteStatus(
        null,
      );

      return undefined;
    }

    refreshActiveStatus();

    const interval =
      window.setInterval(
        () => {
          refreshActiveStatus();
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

          onMessage(
            `✅ ${result.message}`,
          );

          await refreshServers();

          return (
            result.server ??
            null
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to add OpenClaw server: ${String(
              nextError,
            )}`;

          setStatus(
            "error",
          );

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
      },
      [
        onMessage,
        refreshServers,
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

          onMessage(
            `✅ ${result.message}`,
          );

          await refreshServers();

          return (
            result.server ??
            null
          );
        } catch (
          nextError
        ) {
          const message =
            `Unable to update OpenClaw server: ${String(
              nextError,
            )}`;

          setStatus(
            "error",
          );

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
      },
      [
        onMessage,
        refreshServers,
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

          onMessage(
            `🗑️ ${result.message}`,
          );

          await refreshServers();

          await refreshActiveStatus();
        } catch (
          nextError
        ) {
          const message =
            `Unable to delete OpenClaw server: ${String(
              nextError,
            )}`;

          setStatus(
            "error",
          );

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
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
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshServers,
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

          onMessage(
            `🔌 ${result.message}`,
          );

          await refreshServers();

          await refreshActiveStatus();
        } catch (
          nextError
        ) {
          const message =
            `Unable to change OpenClaw server status: ${String(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
          setBusyServerId(
            null,
          );
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshServers,
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

          onMessage(
            `⭐ ${result.message}`,
          );

          await refreshServers();

          await refreshActiveStatus();
        } catch (
          nextError
        ) {
          const message =
            `Unable to activate OpenClaw server: ${String(
              nextError,
            )}`;

          setError(
            message,
          );

          onMessage(
            `❌ ${message}`,
          );
        } finally {
          setBusyServerId(
            null,
          );
        }
      },
      [
        onMessage,
        refreshActiveStatus,
        refreshServers,
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

          onMessage(
            result.success
              ? `✅ ${result.message}`
              : `❌ ${result.message}`,
          );

          await refreshServers();

          if (
            id ===
            activeServerId
          ) {
            await refreshActiveStatus();
          }

          return result;
        } catch (
          nextError
        ) {
          const message =
            `OpenClaw connection test failed: ${String(
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
          setTestingServerId(
            null,
          );
        }
      },
      [
        activeServerId,
        onMessage,
        refreshActiveStatus,
        refreshServers,
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
            `OpenClaw connection test failed: ${String(
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
          setTestingServerId(
            null,
          );
        }
      },
      [onMessage],
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
        ) =>
          server.name
            .toLowerCase()
            .includes(
              search,
            ) ||
          server.serverUrl
            .toLowerCase()
            .includes(
              search,
            ) ||
          server
            .connectionState
            .toLowerCase()
            .includes(
              search,
            ),
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
      [servers],
    );

  const connectedCount =
    useMemo(
      () =>
        servers.filter(
          (
            server,
          ) =>
            server
              .connectionState ===
            "connected",
        ).length,
      [servers],
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
      [servers],
    );

  return {
    servers,
    filteredServers,
    activeServer,
    activeServerId,
    busyServerId,
    connectedCount,
    enabledCount,
    testingServerId,
    remoteStatus,
    status,
    searchText,
    error,

    setSearchText,
    refreshServers,
    refreshActiveStatus,
    createServer,
    editServer,
    removeServer,
    setServerEnabled,
    activateServer,
    testSavedServer,
    testUnsavedServer,
  };
}

export default useOpenClaw;