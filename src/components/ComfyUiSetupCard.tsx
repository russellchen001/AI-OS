import { useState } from "react";

import {
  setupComfyUiManagedProfile,
  type ComfyUiManagedProfileSetupResult,
} from "../services/generativeMedia";

function actionLabel(
  result: ComfyUiManagedProfileSetupResult,
): string {
  switch (result.action) {
    case "installed":
      return "Installed";
    case "repaired":
      return "Repaired";
    case "adopted":
      return "Verified existing model";
    case "already-installed":
      return "Already verified";
  }
}

export default function ComfyUiSetupCard() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  async function setup() {
    const confirmed = window.confirm(
      "AI-OS will download and verify about 2.13 GB for the local image-generation bootstrap profile. Interrupted downloads can resume the next time you run Setup / Repair. Continue?",
    );

    if (!confirmed) {
      return;
    }

    setBusy(true);
    setMessage(
      "Downloading and verifying the managed ComfyUI checkpoint…",
    );
    setError("");

    try {
      const result =
        await setupComfyUiManagedProfile(true);

      setMessage(
        `${actionLabel(result)} · workflow, model, nodes and SHA-256 integrity are verified. Real generation/output validation is the next stage.`,
      );
    } catch (nextError) {
      setMessage("");
      setError(
        nextError instanceof Error
          ? nextError.message
          : String(nextError),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="models-download-card">
      <div className="models-download-heading">
        <div>
          <h3>Local Image Generation</h3>
          <p>
            Set up or repair the AI-OS managed ComfyUI
            compatibility profile. Bootstrap model: Stable
            Diffusion v1.5 FP16 · about 2.13 GB.
          </p>
        </div>

        <span>◫</span>
      </div>

      <div className="models-download-form">
        <div>
          <strong>ComfyUI managed profile</strong>
          <small>
            SHA-256 pinned · CreativeML OpenRAIL-M ·
            resumable installation
          </small>
        </div>

        <button
          type="button"
          className="action-button backup-button"
          disabled={busy}
          onClick={() => void setup()}
        >
          {busy ? "Setting up…" : "Setup / Repair"}
        </button>
      </div>

      {message && (
        <p className="provider-success-status">
          {message}
        </p>
      )}

      {error && (
        <p className="provider-error-status">
          {error}
        </p>
      )}
    </div>
  );
}
