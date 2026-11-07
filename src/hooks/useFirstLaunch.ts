import { useEffect, useRef, useState } from "react";
import {
  runFirstLaunchDiagnostics,
  repairFirstLaunchOpenClaw,
  type FirstLaunchDiagnostics,
} from "../services/firstLaunch";

const progressKey = "ai-os.beta7.first-launch";

export function useFirstLaunch(workspaceActive = true) {
  const saved = localStorage.getItem(progressKey);
  const [step, setStep] = useState(saved === "1" ? 1 : saved === "2" || saved === "complete" ? 2 : 0);
  const [visible, setVisible] = useState(saved !== "complete");
  const [diagnostics, setDiagnostics] = useState<FirstLaunchDiagnostics>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const request = useRef(0);

  async function refresh() {
    const id = ++request.current;
    setBusy(true);
    setError(undefined);
    const apply = (result: FirstLaunchDiagnostics) => {
      if (id !== request.current) return;
      setDiagnostics(result);
      if (result.providerConnected && localStorage.getItem(progressKey) !== "complete") {
        setStep(current => {
          if (current !== 1) return current;
          localStorage.setItem(progressKey, "2");
          return 2;
        });
      }
    };
    try {
      apply(await runFirstLaunchDiagnostics(apply));
    } catch {
      if (id === request.current) setError("AI connection check failed. Open My AI or try again.");
    } finally {
      if (id === request.current) setBusy(false);
    }
  }

  useEffect(() => {
    if (workspaceActive && localStorage.getItem(progressKey) !== "complete") {
      setVisible(true);
      if (step > 0) void refresh();
    }
  }, [workspaceActive]);

  async function repairOpenClaw() {
    setBusy(true);
    setError(undefined);
    try {
      setDiagnostics(await repairFirstLaunchOpenClaw());
    } catch {
      setError("OpenClaw repair did not complete. Open Manage Agents to review it.");
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    const changed = () => void refresh();
    window.addEventListener("ai-os:providers-changed", changed);
    return () => window.removeEventListener("ai-os:providers-changed", changed);
  }, []);

  useEffect(() => {
    const open = () => {
      setStep(2);
      setVisible(true);
      void refresh();
    };
    window.addEventListener("ai-os:open-setup-diagnostics", open);
    return () => window.removeEventListener("ai-os:open-setup-diagnostics", open);
  }, []);

  function advance(next: number) {
    localStorage.setItem(progressKey, String(next));
    setStep(next);
  }

  function finish() {
    request.current++;
    localStorage.setItem(progressKey, "complete");
    setVisible(false);
  }

  return { step, visible, setVisible, diagnostics, busy, error, refresh, repairOpenClaw, advance, finish };
}
