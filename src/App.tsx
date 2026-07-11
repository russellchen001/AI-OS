import {
  useCallback,
  useEffect,
  useState,
  type CSSProperties,
} from "react";

import "./App.css";

import Header from "./components/Header";
import Sidebar from "./components/Sidebar";

import DashboardPage from "./pages/DashboardPage";
import ServicesPage from "./pages/ServicesPage";
import SettingsPage from "./pages/SettingsPage";

import useMetrics from "./hooks/useMetrics";
import useServices from "./hooks/useServices";
import useSettings from "./hooks/useSettings";

import type {
  PageName,
} from "./types";

function App() {
  const [
    activePage,
    setActivePage,
  ] = useState<PageName>(
    "Dashboard",
  );

  const [
    message,
    setMessage,
  ] = useState("");

  const handleMessage =
    useCallback(
      (
        nextMessage: string,
      ) => {
        setMessage(nextMessage);
      },
      [],
    );

  const {
    settings,
    updateSetting,
    resetSettings,
  } = useSettings(
    handleMessage,
  );

  const {
    metrics,
    refreshMetrics,
  } = useMetrics();

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
    onMessage:
      handleMessage,
  });

  useEffect(() => {
    healthCheck(false);
    refreshMetrics();

    const interval =
      window.setInterval(
        () => {
          healthCheck(false);
          refreshMetrics();
        },
        Math.max(
          settings.refreshInterval,
          2,
        ) * 1000,
      );

    return () =>
      window.clearInterval(
        interval,
      );
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
      settings.theme === "dark"
        ? "radial-gradient(circle at top right,#172554 0%,#0f172a 38%,#020617 100%)"
        : "linear-gradient(135deg,#f8fafc,#e2e8f0)",

    color:
      settings.theme === "dark"
        ? "#ffffff"
        : "#0f172a",

    fontFamily:
      "Inter,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif",
  };

  const cardStyle:
    CSSProperties = {
    borderRadius: "16px",

    background:
      settings.theme === "dark"
        ? "rgba(30,41,59,.76)"
        : "rgba(255,255,255,.82)",

    border:
      settings.theme === "dark"
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
        activePage={activePage}
        settings={settings}
        onPageChange={
          setActivePage
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
            onHealthCheck={() =>
              healthCheck(true)
            }
            onBackup={() =>
              setMessage(
                "💾 Backup will be implemented in Sprint 4.",
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

        {message && (
          <section className="message-panel">
            {message}
          </section>
        )}
      </main>
    </div>
  );
}

export default App;