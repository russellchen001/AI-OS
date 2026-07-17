import {
  useCallback,
  useEffect,
  useState,
  type CSSProperties,
} from "react";

import "./App.css";

import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import CommandPalette from "./components/CommandPalette";

import BackupPage from "./pages/BackupPage";
import DashboardPage from "./pages/DashboardPage";
import LogsPage from "./pages/LogsPage";
import McpPage from "./pages/McpPage";
import ModelsPage from "./pages/ModelsPage";
import MultiLlmPage from "./pages/MultiLlmPage";
import PromptLibraryPage from "./pages/PromptLibraryPage";
import ArtifactsPage from "./pages/ArtifactsPage";
import AiCouncilPage from "./pages/AiCouncilPage";
import OpenClawPage from "./pages/OpenClawPage";
import ServicesPage from "./pages/ServicesPage";
import SettingsPage from "./pages/SettingsPage";

import useBackup from "./hooks/useBackup";
import useLogs from "./hooks/useLogs";
import useMcp from "./hooks/useMcp";
import useMetrics from "./hooks/useMetrics";
import useModels from "./hooks/useModels";
import useOpenClaw from "./hooks/useOpenClaw";
import useServices from "./hooks/useServices";
import useSettings from "./hooks/useSettings";

import type {
  PageName,
} from "./types/index";

function App() {
  const [
    activePage,
    setActivePage,
  ] = useState<PageName>(
    "Dashboard",
  );
  const [
    commandPaletteOpen,
    setCommandPaletteOpen,
  ] = useState(false);

  useEffect(() => {
    const handleCommandPaletteShortcut =
      (event: KeyboardEvent) => {
        if (
          (
            event.metaKey ||
            event.ctrlKey
          ) &&
          event.key.toLowerCase() ===
            "k"
        ) {
          event.preventDefault();

          setCommandPaletteOpen(
            (current) =>
              !current,
          );
          return;
        }

        if (
          event.key ===
            "Escape" &&
          commandPaletteOpen
        ) {
          setCommandPaletteOpen(
            false,
          );
        }
      };

    window.addEventListener(
      "keydown",
      handleCommandPaletteShortcut,
    );

    return () => {
      window.removeEventListener(
        "keydown",
        handleCommandPaletteShortcut,
      );
    };
  }, [
    commandPaletteOpen,
  ]);

  type ToastType =
    | "success"
    | "error"
    | "warning"
    | "info";

  type ToastMessage = {
    text: string;
    type: ToastType;
  };

  const [
    messages,
    setMessages,
  ] = useState<
    Array<
      ToastMessage & {
        id: string;
      }
    >
  >([]);

  const handleMessage =
    useCallback(
      (
        nextMessage: string,
      ) => {
        const normalized =
          nextMessage.toLowerCase();

        const type: ToastType =
          normalized.includes("error") ||
          normalized.includes("failed") ||
          normalized.includes("unable")
            ? "error"
            : normalized.includes("warning") ||
                normalized.includes("warn")
              ? "warning"
              : normalized.includes("success") ||
                  normalized.includes("saved") ||
                  normalized.includes("created") ||
                  normalized.includes("updated") ||
                  normalized.includes("deleted") ||
                  normalized.includes("completed") ||
                  normalized.includes("exported") ||
                  normalized.includes("imported") ||
                  normalized.includes("moved")
                ? "success"
                : "info";

        const id =
          crypto.randomUUID();

        setMessages(
          (current) => [
            ...current,
            {
              id,
              text:
                nextMessage,
              type,
            },
          ].slice(-4),
        );

        window.setTimeout(
          () => {
            setMessages(
              (current) =>
                current.filter(
                  (item) =>
                    item.id !==
                    id,
                ),
            );
          },
          4500,
        );
      },
      [],
    );


  const {
    settings,
    updateSetting,
    mergeSettings,
    resetSettings,
  } = useSettings(
    handleMessage,
  );

  const {
    metrics,
    refreshMetrics,
  } = useMetrics();

  const openClaw =
  useOpenClaw({
    refreshInterval:
      settings.refreshInterval,

    onMessage:
      handleMessage,
  });

  const {
    services,
    lastUpdated,
    isChecking,
    globalAction,
    serviceAction,
    openAction,
    isBusy,
    runningCount,
    stoppedCount,
    unknownCount,
    allRunning,
    healthCheck,
    startService,
    stopService,
    openService,
    handleGlobalToggle,
  } = useServices({
  settings,

  activeOpenClawUrl:
    openClaw.activeServer
      ?.serverUrl,

  onMessage:
    handleMessage,
});

  const backup =
    useBackup({
      settings,

      onMessage:
        handleMessage,

      onSettingsRestored:
        mergeSettings,
    });

  const logs =
    useLogs({
      lineLimit:
        settings.logLineLimit,

      refreshInterval:
        settings.refreshInterval,

      onMessage:
        handleMessage,
    });

  const models =
    useModels({
      refreshInterval:
        settings.refreshInterval,

      onMessage:
        handleMessage,
    });

  const mcp =
    useMcp({
      onMessage:
        handleMessage,
    });

  useEffect(() => {
    void healthCheck(
      false,
    );

    void refreshMetrics();

    const interval =
      window.setInterval(
        () => {
          void healthCheck(
            false,
          );

          void refreshMetrics();
        },
        Math.max(
          settings
            .refreshInterval,
          2,
        ) * 1000,
      );

    return () => {
      window.clearInterval(
        interval,
      );
    };
  }, [
    healthCheck,
    refreshMetrics,
    settings.refreshInterval,
  ]);

  const appStyle:
    CSSProperties = {
    display: "flex",
    minHeight: "100vh",

    background:
      settings.theme ===
      "dark"
        ? "radial-gradient(circle at top right,#172554 0%,#0f172a 38%,#020617 100%)"
        : "linear-gradient(135deg,#f8fafc,#e2e8f0)",

    color:
      settings.theme ===
      "dark"
        ? "#ffffff"
        : "#0f172a",

    fontFamily:
      "Inter,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif",
  };

  const cardStyle:
    CSSProperties = {
    borderRadius:
      "16px",

    background:
      settings.theme ===
      "dark"
        ? "rgba(30,41,59,.76)"
        : "rgba(255,255,255,.82)",

    border:
      settings.theme ===
      "dark"
        ? "1px solid rgba(148,163,184,.12)"
        : "1px solid rgba(148,163,184,.24)",

    boxShadow:
      "0 10px 30px rgba(15,23,42,.14)",

    backdropFilter:
      "blur(14px)",
  };

  return (
    <div style={appStyle}>
      <Sidebar
        activePage={
          activePage
        }
        settings={
          settings
        }
        onPageChange={
          setActivePage
        }
        onOpenCommandPalette={() =>
          setCommandPaletteOpen(
            true,
          )
        }
      />

      <main className="main-content">
        <Header
          isChecking={
            isChecking
          }
          lastUpdated={
            lastUpdated
          }
        />

        {activePage ===
          "Dashboard" && (
          <DashboardPage
            services={
              services
            }
            metrics={
              metrics
            }
            cardStyle={
              cardStyle
            }
            runningCount={
              runningCount
            }
            stoppedCount={
              stoppedCount
            }
            unknownCount={
              unknownCount
            }
            allRunning={
              allRunning
            }
            isBusy={
              isBusy
            }
            isChecking={
              isChecking
            }
            globalAction={
              globalAction
            }
            serviceAction={
              serviceAction
            }
            openAction={
              openAction
            }
            onGlobalToggle={
              handleGlobalToggle
            }
            onStartService={
              startService
            }
            onStopService={
              stopService
            }
            onOpenService={
              openService
            }
            onRefreshMetrics={
              refreshMetrics
            }
            onHealthCheck={() => {
              void healthCheck(
                true,
              );
            }}
            onBackup={() =>
              setActivePage(
                "Backup",
              )
            }
          />
        )}

        {activePage ===
          "Services" && (
          <ServicesPage
            services={
              services
            }
            cardStyle={
              cardStyle
            }
            allRunning={
              allRunning
            }
            isBusy={
              isBusy
            }
            globalAction={
              globalAction
            }
            serviceAction={
              serviceAction
            }
            openAction={
              openAction
            }
            onGlobalToggle={
              handleGlobalToggle
            }
            onStartService={
              startService
            }
            onStopService={
              stopService
            }
            onOpenService={
              openService
            }
          />
        )}

        {activePage ===
          "OpenClaw" && (
          <OpenClawPage
            servers={
              openClaw
                .filteredServers
            }
            activeServer={
              openClaw
                .activeServer
            }
            enabledCount={
              openClaw
                .enabledCount
            }
            connectedCount={
              openClaw
                .connectedCount
            }
            autoConnectCount={
              openClaw
                .autoConnectCount
            }
            averageLatencyMs={
              openClaw
                .averageLatencyMs
            }
            status={
              openClaw.status
            }
            busyServerId={
              openClaw
                .busyServerId
            }
            testingServerId={
              openClaw
                .testingServerId
            }
            isTestingAll={
              openClaw
                .isTestingAll
            }
            isImporting={
              openClaw
                .isImporting
            }
            isExporting={
              openClaw
                .isExporting
            }
            remoteStatus={
              openClaw
                .remoteStatus
            }
            runtimeConfig={
              openClaw
                .runtimeConfig
            }
            searchText={
              openClaw
                .searchText
            }
            error={
              openClaw.error
            }
            cardStyle={
              cardStyle
            }
            onSearchChange={
              openClaw
                .setSearchText
            }
            onRefresh={() => {
              void openClaw
                .refreshAllMetadata(
                  true,
                );
            }}
            onCreate={
              openClaw
                .createServer
            }
            onUpdate={
              openClaw
                .editServer
            }
            onDelete={
              openClaw
                .removeServer
            }
            onDuplicate={
              openClaw
                .duplicateServer
            }
            onToggle={
              openClaw
                .setServerEnabled
            }
            onActivate={
              openClaw
                .activateServer
            }
            onTestSaved={
              openClaw
                .testSavedServer
            }
            onTestUnsaved={
              openClaw
                .testUnsavedServer
            }
            onTestAll={
              openClaw
                .testAllServers
            }
            onCopyUrl={
              openClaw
                .copyServerUrl
            }
            onExport={
              openClaw
                .exportServers
            }
            onImport={
              openClaw
                .importServers
            }
          />
        )}

        {activePage ===
          "Backup" && (
          <BackupPage
            settings={
              settings
            }
            backups={
              backup.backups
            }
            status={
              backup.status
            }
            selectedBackup={
              backup
                .selectedBackup
            }
            error={
              backup.error
            }
            cardStyle={
              cardStyle
            }
            onUpdateSetting={
              updateSetting
            }
            onCreateBackup={
              backup.runBackup
            }
            onCancelBackup={
              backup.cancelCurrentBackup
            }
            onRestoreBackup={(
              archivePath,
              restoreOpenClawConfig,
              restoreAiOsSettings,
            ) =>
              backup.runRestore({
                archivePath,
                restoreOpenClawConfig,
                restoreAiOsSettings,
              })
            }
            onRefresh={
              backup
                .refreshBackups
            }
            onReveal={
              backup
                .openBackupLocation
            }
            onDelete={
              backup
                .removeBackup
            }
          />
        )}

        {activePage ===
          "Logs" && (
          <LogsPage
            logs={
              logs
                .filteredLogs
            }
            selectedSource={
              logs
                .selectedSource
            }
            selectedLevel={
              logs
                .selectedLevel
            }
            searchText={
              logs
                .searchText
            }
            isLoading={
              logs
                .isLoading
            }
            isAutoRefresh={
              logs
                .isAutoRefresh
            }
            error={
              logs.error
            }
            cardStyle={
              cardStyle
            }
            onSourceChange={
              logs
                .setSelectedSource
            }
            onLevelChange={
              logs
                .setSelectedLevel
            }
            onSearchChange={
              logs
                .setSearchText
            }
            onAutoRefreshChange={
              logs
                .setIsAutoRefresh
            }
            onRefresh={() => {
              void logs.refreshLogs(
                true,
              );
            }}
            onClear={
              logs.removeLogs
            }
          />
        )}

        {activePage ===
          "Models" && (
          <ModelsPage
            models={
              models
                .filteredModels
            }
            totalSize={
              models
                .totalSize
            }
            status={
              models.status
            }
            activeModel={
              models
                .activeModel
            }
            pullProgress={
              models
                .pullProgress
            }
            error={
              models.error
            }
            searchText={
              models
                .searchText
            }
            cardStyle={
              cardStyle
            }
            onSearchChange={
              models
                .setSearchText
            }
            onRefresh={
              models
                .refreshModels
            }
            onPull={
              models.pullModel
            }
            onDelete={
              models
                .removeModel
            }
            onTest={
              models.testModel
            }
            onInspect={
              models
                .inspectModel
            }
          />
        )}

        {activePage ===
          "MCP" && (
          <McpPage
            servers={
              mcp
                .filteredServers
            }
            enabledCount={
              mcp
                .enabledCount
            }
            status={
              mcp.status
            }
            activeServerId={
              mcp
                .activeServerId
            }
            searchText={
              mcp
                .searchText
            }
            error={
              mcp.error
            }
            cardStyle={
              cardStyle
            }
            onSearchChange={
              mcp
                .setSearchText
            }
            onRefresh={
              mcp
                .refreshServers
            }
            onCreate={
              mcp
                .createServer
            }
            onUpdate={
              mcp
                .editServer
            }
            onToggle={
              mcp
                .setServerEnabled
            }
            onDelete={
              mcp
                .removeServer
            }
          />
        )}

        {activePage ===
          "MultiLLM" && (
          <MultiLlmPage
            cardStyle={
              cardStyle
            }
            onMessage={
              handleMessage
            }
          />
        )}

        {activePage ===
          "Prompt Library" && (
          <PromptLibraryPage
            cardStyle={
              cardStyle
            }
            onMessage={
              handleMessage
            }
            onUsePrompt={(
              content,
              target,
            ) => {
              localStorage.setItem(
                "ai-os.prompt-library.pending.v1",
                JSON.stringify({
                  content,
                  target,
                  createdAt:
                    Date.now(),
                }),
              );

              setActivePage(
                "MultiLLM",
              );

              handleMessage(
                target ===
                "compare"
                  ? "Prompt loaded into MultiLLM Compare."
                  : "Prompt loaded into Smart Router.",
              );
            }}
          />
        )}

        {activePage ===
          "Artifacts" && (
          <ArtifactsPage
            cardStyle={
              cardStyle
            }
            onMessage={
              handleMessage
            }
          />
        )}

        {activePage ===
          "AI Council" && (
          <AiCouncilPage
            cardStyle={
              cardStyle
            }
            onMessage={
              handleMessage
            }
          />
        )}

        {activePage ===
          "Settings" && (
          <SettingsPage
            settings={
              settings
            }
            cardStyle={
              cardStyle
            }
            onUpdateSetting={
              updateSetting
            }
            onReset={
              resetSettings
            }
          />
        )}

        {messages.length > 0 && (
          <div className="message-stack">
            {messages.map(
              (message) => (
                <section
                  key={
                    message.id
                  }
                  className={[
                    "message-panel",
                    `message-panel-${message.type}`,
                  ].join(" ")}
                  role={
                    message.type ===
                    "error"
                      ? "alert"
                      : "status"
                  }
                >
                  <span
                    className="message-panel-icon"
                    aria-hidden="true"
                  >
                    {message.type ===
                    "success"
                      ? "✓"
                      : message.type ===
                          "error"
                        ? "!"
                        : message.type ===
                            "warning"
                          ? "⚠"
                          : "i"}
                  </span>

                  <span className="message-panel-text">
                    {
                      message.text
                    }
                  </span>

                  <button
                    type="button"
                    aria-label="Dismiss message"
                    onClick={() =>
                      setMessages(
                        (current) =>
                          current.filter(
                            (item) =>
                              item.id !==
                              message.id,
                          ),
                      )
                    }
                  >
                    ×
                  </button>
                </section>
              ),
            )}
          </div>
        )}
        <CommandPalette
          open={
            commandPaletteOpen
          }
          activePage={
            activePage
          }
          onClose={() =>
            setCommandPaletteOpen(
              false,
            )
          }
          onPageChange={
            setActivePage
          }
          onMessage={
            handleMessage
          }
        />
      </main>
    </div>
  );
}

export default App;