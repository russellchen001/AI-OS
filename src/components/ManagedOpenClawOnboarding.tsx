import {
  useEffect,
  useState,
} from "react";

import {
  useDialog,
} from "./DialogProvider";

import {
  adoptExistingOpenClaw,
  completeManagedOpenClawSetup,
  finishManagedOpenClawSetup,
  getManagedOpenClawStatus,
  installManagedOpenClaw,
  repairManagedOpenClaw,
  startManagedOpenClaw,
  type ManagedOpenClawInstallResult,
  type ManagedOpenClawStatus,
} from "../services/managedOpenClaw";

type ProgressState = {
  title: string;
  message: string;
};

let onboardingPromptStarted = false;

export default function ManagedOpenClawOnboarding() {
  const {
    confirm,
    prompt,
    alert,
  } = useDialog();

  const [
    progress,
    setProgress,
  ] = useState<ProgressState | null>(null);

  useEffect(() => {
    let active = true;

    const showResult = async (
      result: ManagedOpenClawInstallResult,
      successTitle: string,
      failureTitle: string,
    ) => {
      if (!active) return;
      setProgress(null);
      await alert({
        title: result.success
          ? successTitle
          : failureTitle,
        message: result.message,
      });
    };

    const runAction = async (
      nextProgress: ProgressState,
      action: () => Promise<ManagedOpenClawInstallResult>,
      successTitle: string,
      failureTitle: string,
    ) => {
      if (!active) return;
      setProgress(nextProgress);

      try {
        await showResult(
          await action(),
          successTitle,
          failureTitle,
        );
      } catch {
        if (!active) return;
        setProgress(null);
        await alert({
          title: failureTitle,
          message:
            "AI-OS could not complete the OpenClaw action. Existing OpenClaw configuration and credentials were not intentionally replaced.",
        });
      }
    };

    const handleStatus = async (
      status: ManagedOpenClawStatus,
    ) => {
      if (
        !active ||
        onboardingPromptStarted ||
        status.state === "ready"
      ) {
        return;
      }

      onboardingPromptStarted = true;

      if (status.state === "not-installed") {
        const approved = await confirm({
          title: "Set up OpenClaw?",
          message:
            "AI-OS can install the official OpenClaw runtime, create a local managed Gateway, store its secret in OpenClaw Secret Store and macOS Keychain, and verify the connection.",
          confirmLabel: "Install & Set Up",
          cancelLabel: "Not now",
          tone: "default",
        });

        if (active && approved) {
          await runAction(
            {
              title: "Installing OpenClaw",
              message:
                "Installing the official runtime, configuring secure credentials and verifying the local Gateway.",
            },
            installManagedOpenClaw,
            "OpenClaw Ready",
            "OpenClaw setup failed",
          );
        }
        return;
      }

      if (status.state === "installed-not-configured") {
        const approved = await confirm({
          title: "Complete OpenClaw Setup?",
          message:
            "OpenClaw is installed but has no existing user configuration. AI-OS can create a secure managed local Gateway and verify it.",
          confirmLabel: "Complete Setup",
          cancelLabel: "Not now",
          tone: "default",
        });

        if (active && approved) {
          await runAction(
            {
              title: "Completing OpenClaw Setup",
              message:
                "Writing SecretRef configuration, installing the Gateway service and verifying RPC readiness.",
            },
            completeManagedOpenClawSetup,
            "OpenClaw Ready",
            "OpenClaw setup failed",
          );
        }
        return;
      }

      if (status.state === "configured-no-gateway") {
        const approved = await confirm({
          title: "Finish OpenClaw Setup?",
          message:
            "The secure AI-OS managed configuration exists, but its Gateway service is missing. AI-OS can install and start only that service, then verify it.",
          confirmLabel: "Finish Setup",
          cancelLabel: "Not now",
          tone: "default",
        });

        if (active && approved) {
          await runAction(
            {
              title: "Finishing OpenClaw Setup",
              message:
                "Installing the managed Gateway service without replacing configuration or credentials.",
            },
            finishManagedOpenClawSetup,
            "OpenClaw Ready",
            "OpenClaw setup failed",
          );
        }
        return;
      }

      if (status.state === "gateway-stopped") {
        const approved = await confirm({
          title: "Start OpenClaw?",
          message:
            "The managed OpenClaw Gateway is configured but not reachable. AI-OS can start the existing service and verify it.",
          confirmLabel: "Start OpenClaw",
          cancelLabel: "Not now",
          tone: "default",
        });

        if (active && approved) {
          await runAction(
            {
              title: "Starting OpenClaw",
              message:
                "Starting the existing Gateway service and waiting for a verified connection.",
            },
            startManagedOpenClaw,
            "OpenClaw Ready",
            "OpenClaw start failed",
          );
        }
        return;
      }

      if (status.state === "existing-unmanaged") {
        const token = await prompt({
          title: "Adopt Existing OpenClaw?",
          message:
            status.diagnostic ??
            "AI-OS found an existing OpenClaw installation. Enter its current Gateway token to verify and adopt it without changing native OpenClaw configuration or service settings.",
          placeholder: "Existing Gateway token",
          required: status.credentialRequired,
          confirmLabel: "Adopt Existing OpenClaw",
          cancelLabel: "Not now",
          tone: "default",
        });

        if (active && token !== null) {
          await runAction(
            {
              title: "Adopting Existing OpenClaw",
              message:
                "Verifying the existing Gateway without reinstalling OpenClaw or changing its native configuration.",
            },
            () => adoptExistingOpenClaw(token),
            "OpenClaw Adopted",
            "OpenClaw adoption failed",
          );
        }
        return;
      }

      const approved = await confirm({
        title: "Repair OpenClaw?",
        message:
          `${status.diagnostic ?? "AI-OS found an inconsistent OpenClaw state."}\n\nRepair first performs a bounded diagnosis. AI-OS will not automatically reinstall OpenClaw, rotate credentials, replace the Gateway service or overwrite user configuration.`,
        confirmLabel: "Repair OpenClaw",
        cancelLabel: "Not now",
        tone: "warning",
      });

      if (active && approved) {
        await runAction(
          {
            title: "Diagnosing OpenClaw",
            message:
              "Checking the bounded repair policy without resetting configuration or credentials.",
          },
          repairManagedOpenClaw,
          "OpenClaw Repaired",
          "OpenClaw needs attention",
        );
      }
    };

    void (async () => {
      try {
        await handleStatus(
          await getManagedOpenClawStatus(),
        );
      } catch {
        if (!active || onboardingPromptStarted) return;
        onboardingPromptStarted = true;
        await alert({
          title: "OpenClaw status unavailable",
          message:
            "AI-OS could not inspect OpenClaw safely. No installation, configuration or credential changes were made.",
        });
      }
    })();

    return () => {
      active = false;
    };
  }, [
    alert,
    confirm,
    prompt,
  ]);

  if (!progress) {
    return null;
  }

  return (
    <div
      className="provider-setup-backdrop"
      role="presentation"
    >
      <section
        className="provider-setup-dialog"
        role="status"
        aria-live="polite"
        aria-busy="true"
      >
        <header>
          <div className="provider-setup-mark">
            O
          </div>

          <div>
            <p>AI-OS Runtime</p>
            <h2>{progress.title}</h2>
          </div>
        </header>

        <div className="provider-setup-body">
          <p>{progress.message}</p>
          <p>Keep AI-OS open until this step completes.</p>
        </div>
      </section>
    </div>
  );
}
