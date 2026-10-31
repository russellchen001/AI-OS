import {
  assessInstalledLocalModels,
  assessLocalModel,
  deleteOmlxModel,
  listOmlxAdminModels,
  pullOmlxModel,
  recommendLocalModels,
  showOmlxModel,
  showOmlxModelInFinder,
  type ModelFitAssessment,
  type LocalModelRecommendationReport,
  type OmlxAdminModel,
} from "../services/models";

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import { useDialog } from "../components/DialogProvider";
import SectionTabs from "../components/SectionTabs";
import type { PageName } from "../types/index";
import type {
  ProviderAdapterDescriptor,
  ProviderModelEntry,
  ProviderInstance,
} from "../types/provider";
import {
  getProviderAdapter,
  listProviderAdapters,
} from "../services/providerAdapters";
import {
  createProviderInstance,
  cancelProviderOAuth,
  deleteProviderCredential,
  listProviderInstances,
  initializeProviderInstances,
  verifyProviderModel,
  removeProviderInstance,
  saveProviderApiKey,
  saveProviderInstance,
  isProviderOAuthConfigured,
  startProviderOAuth,
  type ProviderOAuthCompletedEvent,
  type ProviderOAuthErrorEvent,
} from "../services/providers";
import {
  SOFTWARE_UPDATES_EVENT,
  checkSoftwareUpdates,
  loadSoftwareUpdates,
  type SoftwareUpdateSnapshot,
} from "../services/softwareUpdateMonitor";
import {
  CLI_ACCOUNT_PROVIDER_IDS,
  describeCliInstallPlan,
  type CliVersionReport,
  getCliProviderAdapter,
  installCliProvider,
  prepareCliProviderInstall,
} from "../services/cliProviderManager";

const OAUTH_FRONTEND_TIMEOUT_MS = 5 * 60 * 1000;
type ClaudeCodeStatus = { installed: boolean; authenticated: boolean; message: string };
const CLI_ACCOUNT_PROVIDERS = CLI_ACCOUNT_PROVIDER_IDS;
type OmlxRuntimeStatus = { supported: boolean; installed: boolean; running: boolean };
type DeviceAuth = {
  state: string;
  verificationUri: string;
  verificationUriComplete?: string;
  userCode: string;
  expiresIn: number;
};

function assessmentBadge(assessment?: ModelFitAssessment): string | null {
  if (!assessment) return null;
  if (assessment.providerCompatible === false) return "Not recommended";
  switch (assessment.fit) {
    case "fit": return "Compatible";
    case "marginal": return "Marginal";
    case "not-fit": return "Not recommended";
    default: return "Fit unknown";
  }
}

function assessmentDetails(assessment: ModelFitAssessment): string {
  const lines = [
    `Recommendation: ${assessmentBadge(assessment) ?? assessment.fitLabel}`,
    `Model: ${assessment.resolvedModelId ?? assessment.requestedModelId}`,
    `Estimated memory: ${assessment.estimatedMemoryGb == null ? "Unknown" : `${assessment.estimatedMemoryGb.toFixed(1)} GB`}`,
    `Suggested quant: ${assessment.quantization ?? "Unknown"}`,
    `Recommended context: ${assessment.recommendedContext == null ? "Unknown" : assessment.recommendedContext.toLocaleString()}`,
    `Expected speed: ${assessment.expectedTokensPerSecond == null ? "Unknown" : `${assessment.expectedTokensPerSecond.toFixed(1)} tok/s`}`,
    `Evidence confidence: ${assessment.confidence}`,
  ];
  if (assessment.evidence.length) lines.push("", assessment.evidence.slice(0, 4).join("\n"));
  return lines.join("\n");
}

type MyAiPageProps = {
  onConnect: (provider: string, method: "account" | "api-key") => void;
  onPageChange: (page: PageName) => void;
};

type CloudProviderCard = {
  id: string;
  mark: string;
  name: string;
  description: string;
  models: string[];
  accountLabel: string;
};

export const cloudProviders: CloudProviderCard[] = [
  {
    id: "openai",
    mark: "O",
    name: "OpenAI",
    description: "GPT and Codex models",
    models: ["GPT‑5.6", "GPT‑5.6 Codex"],
    accountLabel: "Sign in with OpenAI",
  },
  {
    id: "anthropic",
    mark: "A",
    name: "Anthropic",
    description: "Claude models",
    models: ["Claude Opus", "Claude Sonnet"],
    accountLabel: "Sign in with Claude",
  },
  {
    id: "google",
    mark: "G",
    name: "Google",
    description: "Gemini models",
    models: ["Gemini Pro", "Gemini Flash"],
    accountLabel: "Sign in with Google",
  },
  {
    id: "grok",
    mark: "X",
    name: "xAI",
    description: "Grok models",
    models: ["Grok 4.5"],
    accountLabel: "Sign in with Grok",
  },
  {
    id: "deepseek",
    mark: "D",
    name: "DeepSeek",
    description: "DeepSeek chat and reasoning models",
    models: ["DeepSeek V4 Flash", "DeepSeek V4 Pro"],
    accountLabel: "",
  },
  {
    id: "openrouter",
    mark: "R",
    name: "OpenRouter",
    description: "One connection for models from many providers",
    models: ["Auto Router", "Open models"],
    accountLabel: "Sign in with OpenRouter",
  },
  {
    id: "kimi",
    mark: "K",
    name: "Kimi Code",
    description: "Kimi coding subscription and API models",
    models: ["Kimi for Coding"],
    accountLabel: "Sign in with Kimi",
  },
  {
    id: "meta",
    mark: "M",
    name: "Meta",
    description: "Muse Spark through Meta Model API",
    models: ["Muse Spark 1.1"],
    accountLabel: "",
  },
];

const providerCatalog = [
  {
    id: "doubao",
    mark: "豆",
    name: "Doubao",
    description: "Doubao and Seed models",
    connection: "API key",
  },
  {
    id: "compatible",
    mark: "+",
    name: "Other AI",
    description: "Connect another compatible provider",
    connection: "Guided setup",
  },
  {
    id: "omlx",
    mark: "M",
    name: "oMLX",
    description: "Local AI engine optimized for Apple Silicon",
    connection: "API key",
  },
];

function loadConfiguredProviderIds(): Set<string> {
  return new Set(
    listProviderInstances()
      .filter(
        (provider) =>
          provider.connectionState === "connected",
      )
      .map(
        (provider) =>
          provider.providerId,
      ),
  );
}

function MyAiPage({
  onConnect,
  onPageChange,
}: MyAiPageProps) {
  const dialog = useDialog();
  const [providerInstances, setProviderInstances] = useState<ProviderInstance[]>([]);
  const [providerAdapters, setProviderAdapters] = useState<
    ProviderAdapterDescriptor[]
  >([]);
  const [claudeCodeStatus, setClaudeCodeStatus] = useState<ClaudeCodeStatus | null>(null);
  const [cliVersions, setCliVersions] = useState<CliVersionReport[]>(() => loadSoftwareUpdates()?.cli ?? []);
  const [cliVersionsCheckedAt, setCliVersionsCheckedAt] = useState(() => loadSoftwareUpdates()?.checkedAt);
  const [cliUpdateBusy, setCliUpdateBusy] = useState<string>();
  const [cliUpdateMessage, setCliUpdateMessage] = useState("");
  const [omlxRuntime, setOmlxRuntime] = useState<OmlxRuntimeStatus | null>(null);
  const [omlxAdminModels, setOmlxAdminModels] = useState<OmlxAdminModel[]>([]);
  const [omlxLoading, setOmlxLoading] = useState(false);
  const [localModelAssessments, setLocalModelAssessments] = useState<Record<string, ModelFitAssessment>>({});
  const [localModelRecommendations, setLocalModelRecommendations] = useState<LocalModelRecommendationReport | null>(null);
  const configuredProviderIds = new Set([
    ...loadConfiguredProviderIds(),
    ...providerInstances
      .filter(
        (instance) =>
          instance.connectionState === "connected",
      )
      .map(
        (instance) =>
          instance.providerId,
      ),
  ]);
  const omlxInstance = providerInstances.find(
    (instance) => instance.id === "omlx-local" && instance.connectionState === "connected",
  );
  const omlxState = !omlxRuntime?.supported
    ? { badge: "Unavailable", description: "oMLX requires an Apple Silicon Mac." }
    : !omlxRuntime.installed
      ? { badge: "Not installed", description: "Install oMLX to use Apple Silicon models." }
      : omlxLoading
        ? { badge: "Checking", description: "AI-OS is checking the oMLX local server." }
        : !omlxRuntime.running
          ? { badge: "Stopped", description: "oMLX is connected and will start when AI-OS opens." }
          : {
              badge: "Ready",
              description: `${omlxInstance?.models.length ?? 0} local model${omlxInstance?.models.length === 1 ? "" : "s"} available`,
            };
  const omlxAssessmentModels = omlxAdminModels.length
    ? omlxAdminModels
    : (omlxInstance?.models ?? []).map((model) => ({
        name: model.remoteModelId,
        displayName: model.displayName,
        size: 0,
        sizeFormatted: "",
      }));
  const omlxAssessmentKey = omlxAssessmentModels.map((model) => model.name).join("|");

  useEffect(() => {
    const models = [
      ...omlxAssessmentModels.map((model) => ({
        providerId: "omlx" as const,
        modelId: model.name,
      })),
    ];
    if (!models.length) {
      setLocalModelAssessments({});
      return;
    }
    let active = true;
    void assessInstalledLocalModels(models)
      .then((report) => {
        if (!active) return;
        setLocalModelAssessments(Object.fromEntries(report.assessments.map((assessment) => [
          `${assessment.providerId}:${assessment.requestedModelId}`,
          assessment,
        ])));
      })
      .catch(() => {
        if (active) setLocalModelAssessments({});
      });
    return () => { active = false; };
  }, [omlxAssessmentKey]);

  useEffect(() => {
    let active = true;
    void recommendLocalModels()
      .then((report) => {
        if (active) setLocalModelRecommendations(report);
      })
      .catch(() => {
        if (active) setLocalModelRecommendations(null);
      });
    return () => { active = false; };
  }, []);

  async function refreshOmlx(startIfStopped = false) {
    setOmlxLoading(true);
    try {
      let status = await invoke<OmlxRuntimeStatus>("get_omlx_runtime_status");
      if (startIfStopped && status.supported && status.installed && !status.running) {
        status = await invoke<OmlxRuntimeStatus>("start_omlx_runtime");
      }
      setOmlxRuntime(status);
      if (status.running && omlxInstance) {
        setOmlxAdminModels(await listOmlxAdminModels());
        const result = await (await getProviderAdapter("omlx")).testConnection("omlx-local");
        const defaultModel = omlxInstance.models.find((model) => model.isDefault)?.remoteModelId;
        const refreshed = await saveProviderInstance({
          ...omlxInstance,
          models: result.discoveredModels.map((model) => ({
            ...model,
            isDefault: model.remoteModelId === defaultModel,
          })),
          updatedAt: new Date().toISOString(),
        });
        setProviderInstances((current) => current.map((instance) =>
          instance.id === "omlx-local" ? refreshed : instance,
        ));
      }
    } catch {
      setOmlxRuntime((current) => current ? { ...current, running: false } : null);
    } finally {
      setOmlxLoading(false);
    }
  }

  async function refreshOmlxRuntime(startIfStopped = false) {
    setOmlxLoading(true);
    try {
      let status = await invoke<OmlxRuntimeStatus>("get_omlx_runtime_status");
      if (startIfStopped && status.supported && status.installed && !status.running) {
        status = await invoke<OmlxRuntimeStatus>("start_omlx_runtime");
      }
      setOmlxRuntime(status);
    } catch {
      setOmlxRuntime((current) => current ? { ...current, running: false } : null);
    } finally {
      setOmlxLoading(false);
    }
  }
  const [verificationModelId, setVerificationModelId] = useState("");
  const [setup, setSetup] = useState<{
    providerId: string;
    provider: string;
    method: "account" | "api-key";
    phase: "credential" | "models" | "manage";
    models: ProviderModelEntry[];
    defaultModelId: string;
    verificationMessage: string;
    liveTested: boolean;
    credentialExpiresAt?: string;
    credentialRefreshable?: boolean;
  } | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [setupError, setSetupError] = useState("");
  const [isConnecting, setIsConnecting] = useState(false);
  const cancelOAuthRef = useRef<(() => void) | null>(null);

  function cancelOAuth() {
    cancelOAuthRef.current?.();
    cancelOAuthRef.current = null;
  }

  function closeSetup() {
    cancelOAuth();
    setIsConnecting(false);
    setSetup(null);
  }

  useEffect(() => () => cancelOAuth(), []);

  useEffect(() => {
    let active = true;

    void initializeProviderInstances()
      .then(() => {
        if (active) {
          setProviderInstances(listProviderInstances());
        }
      });

    const refreshProviders = () => {
      setProviderInstances(listProviderInstances());
    };

    window.addEventListener(
      "ai-os:providers-changed",
      refreshProviders,
    );

    return () => {
      active = false;
      window.removeEventListener(
        "ai-os:providers-changed",
        refreshProviders,
      );
    };
  }, []);

  useEffect(() => {
    if (omlxInstance) void refreshOmlxRuntime(true);
  }, [Boolean(omlxInstance)]);

  useEffect(() => {
    if (!setup) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeSetup();
    };

    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [setup]);

  useEffect(() => {
    let active = true;

    void listProviderAdapters()
      .then((descriptors) => {
        if (active) {
          setProviderAdapters(descriptors);
        }
      })
      .catch(() => {
        if (active) {
          setProviderAdapters([]);
        }
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    void invoke<ClaudeCodeStatus>("get_claude_code_status")
      .then((status) => {
        if (active) setClaudeCodeStatus(status);
      })
      .catch(() => {
        if (active) setClaudeCodeStatus(null);
      });
    return () => {
      active = false;
    };
  }, []);


  async function inspectOmlxModel(model: string) {
    try {
      const [details, assessment] = await Promise.all([
        showOmlxModel(model),
        assessLocalModel({ providerId: "omlx", modelId: model }),
      ]);
      await dialog.alert({
        title: details.displayName,
        message: `${assessmentDetails(assessment)}\n\noMLX details\nModel ID: ${details.name}\nDisk size: ${details.sizeFormatted}`,
        confirmLabel: "Done",
      });
    } catch (error) {
      await dialog.alert({ title: "Could not inspect model", message: String(error) });
    }
  }

  async function removeOmlxModel(model: string) {
    const confirmed = await dialog.confirm({
      title: "Delete local model?",
      message: `AI-OS will permanently remove this oMLX model:\n\n${model}`,
      confirmLabel: "Delete model",
      cancelLabel: "Cancel",
      tone: "warning",
    });
    if (!confirmed) return;
    try {
      await deleteOmlxModel(model);
      await refreshOmlx(false);
    } catch (error) {
      await dialog.alert({ title: "Could not delete model", message: String(error) });
    }
  }

  async function revealOmlxModel(model: string) {
    try {
      await showOmlxModelInFinder(model);
    } catch (error) {
      await dialog.alert({ title: "Could not open Finder", message: String(error) });
    }
  }

  async function downloadOmlxModel() {
    const repoId = await dialog.prompt({
      title: "Download oMLX model",
      message: "Enter a Hugging Face MLX model ID, for example mlx-community/Qwen2.5-7B-Instruct-4bit.",
      confirmLabel: "Download",
      cancelLabel: "Cancel",
      required: true,
    });
    if (!repoId?.trim()) return;
    setOmlxLoading(true);
    try {
      await pullOmlxModel(repoId.trim());
      await refreshOmlx(false);
    } catch (error) {
      await dialog.alert({ title: "Could not download model", message: String(error) });
    } finally {
      setOmlxLoading(false);
    }
  }

  async function disconnectOmlx() {
    const confirmed = await dialog.confirm({
      title: "Disconnect oMLX?",
      message: "AI-OS will remove this connection and its saved API key. Downloaded oMLX models will remain on this Mac.",
      confirmLabel: "Disconnect",
      cancelLabel: "Cancel",
      tone: "warning",
    });
    if (!confirmed) return;
    setOmlxLoading(true);
    try {
      await deleteProviderCredential("omlx-local");
      await removeProviderInstance("omlx-local");
      setProviderInstances(listProviderInstances());
      setOmlxAdminModels([]);
    } catch (error) {
      await dialog.alert({ title: "Could not disconnect oMLX", message: String(error) });
    } finally {
      setOmlxLoading(false);
    }
  }

  function openSetup(provider: string, method: "account" | "api-key", providerId?: string) {
    setApiKey("");
    setSetupError("");
    setSetup({
      providerId: providerId ?? provider.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
      provider,
      method,
      phase: "credential",
      models: [],
      defaultModelId: "",
      verificationMessage: "",
      liveTested: false,
    });
  }

  function providerInstanceId(provider: string): string {
    const normalizedProviderId = provider.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "custom";
    return normalizedProviderId === "omlx" ? "omlx-local" : `${normalizedProviderId}-default`;
  }

  function openManage(instanceId: string) {
    const instance = providerInstances.find((candidate) => candidate.id === instanceId);

    if (!instance) return;
    setVerificationModelId("");
    setApiKey("");
    setSetupError("");
    setSetup({
      providerId: instance.providerId,
      provider: instance.displayName,
      method:
        instance.credential.kind === "oauth" || instance.credential.kind === "local"
          ? "account"
          : "api-key",
      phase: "manage",
      models: instance.models,
      defaultModelId:
        instance.models.find((model) => model.isDefault)?.id ??
        instance.models[0]?.id ??
        "",
      verificationMessage:
        instance.connectionState === "connected"
          ? "Catalog connection verified. Individual model generation is tracked separately."
          : instance.connectionState === "refresh-required"
            ? "This account token has expired and can be refreshed securely."
            : instance.connectionState === "expired"
              ? "This account token has expired. Sign in again to reconnect."
          : "The credential is saved. Live adapter testing is not enabled yet.",
      liveTested: instance.connectionState === "connected",
      credentialExpiresAt: instance.credential.expiresAt,
      credentialRefreshable: instance.credential.refreshable,
    });
  }

  async function refreshAccountCredential() {
    if (!setup || setup.phase !== "manage" || isConnecting) return;
    const instanceId = providerInstanceId(setup.providerId);
    setIsConnecting(true);
    setSetupError("");
    try {
      const adapter = await getProviderAdapter(setup.providerId);
      const refreshed = await adapter.refreshCredential(instanceId);
      setProviderInstances(listProviderInstances());
      setSetup({
        ...setup,
        verificationMessage: "Account access was refreshed successfully.",
        liveTested: refreshed.connectionState === "connected",
        credentialExpiresAt: refreshed.credential.expiresAt,
        credentialRefreshable: refreshed.credential.refreshable,
      });
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : "Provider account sign-in must be renewed.";
      const instance = providerInstances.find((candidate) => candidate.id === instanceId);
      if (instance) {
        await saveProviderInstance({
          ...instance,
          connectionState: message.includes("must be renewed")
            ? "expired"
            : "refresh-required",
          updatedAt: new Date().toISOString(),
        });
        setProviderInstances(listProviderInstances());
      }
      setSetupError(message);
    } finally {
      setIsConnecting(false);
    }
  }

  async function refreshProviderModels() {
    if (!setup || setup.phase !== "manage" || isConnecting) return;
    const instanceId = providerInstanceId(setup.providerId);
    const instance = providerInstances.find((candidate) => candidate.id === instanceId);
    if (!instance) return;
    setIsConnecting(true); setSetupError("");
    try {
      const cliAdapter = instance.credential.kind === "local"
        ? getCliProviderAdapter(setup.providerId)
        : undefined;
      const verification = cliAdapter
        ? await cliAdapter.probe().then(async (status) => {
            if (!status.authenticated) throw new Error(status.message);
            await cliAdapter.verify?.();
            return {
              ok: true, level: "live", checkedAt: new Date().toISOString(),
              message: status.message, discoveredModels: cliAdapter.models(instanceId, status),
            };
          })
        : await (await getProviderAdapter(setup.providerId)).testConnection(instanceId);
      const previous = new Map(instance.models.map((model) => [model.id, model]));
      const currentDefault = instance.models.find((model) => model.isDefault)?.id;
      const models = verification.discoveredModels.map((model) => ({
        ...model,
        availability: previous.get(model.id)?.availability,
        enabled: previous.get(model.id)?.enabled ?? model.enabled,
        isDefault: model.id === currentDefault,
      }));
      const defaultModelId = models.some((model) => model.isDefault)
        ? currentDefault ?? ""
        : models.find((model) => model.enabled)?.id ?? models[0]?.id ?? "";
      const refreshed = await saveProviderInstance({
        ...instance,
        models: models.map((model) => ({ ...model, isDefault: model.id === defaultModelId })),
        connectionState: verification.ok ? "connected" : instance.connectionState,
        lastTestedAt: verification.checkedAt,
        updatedAt: new Date().toISOString(),
      });
      setProviderInstances(listProviderInstances());
      setSetup({
        ...setup,
        models: refreshed.models,
        defaultModelId,
        verificationMessage: `${verification.message} Model catalog refreshed.`,
        liveTested: verification.level === "live" && verification.ok,
      });
    } catch (error) {
      setSetupError(error instanceof Error ? error.message : "Model catalog refresh failed.");
    } finally {
      setIsConnecting(false);
    }
  }

  async function revalidateModel(model: ProviderModelEntry) {
    if (!setup || isConnecting) return;
    setIsConnecting(true); setSetupError("");
    try {
      await verifyProviderModel(model.providerInstanceId, model.remoteModelId);
      setSetup((current) => current ? { ...current, verificationMessage: `${model.displayName}: real generation verified.` } : current);
    } catch (error) {
      setSetupError(error instanceof Error ? error.message : String(error));
    } finally {
      const instances = listProviderInstances();
      setProviderInstances(instances);
      setSetup((current) => current ? { ...current, models: instances.find((instance) => instance.id === model.providerInstanceId)?.models ?? current.models } : current);
      setIsConnecting(false);
    }
  }

  async function continueSetup() {
    if (!setup || isConnecting) return;

    if (setup.method === "account") {
      const instanceId = providerInstanceId(setup.providerId);
      setIsConnecting(true);
      setSetupError("");

      const cliAdapter = getCliProviderAdapter(setup.providerId);
      if (cliAdapter) {
        try {
          let status = await cliAdapter.probe();
          if (!status.installed && cliAdapter.officialPackage) {
            const plan = await prepareCliProviderInstall(cliAdapter.id);
            const approved = await dialog.confirm({
              title: `Install ${cliAdapter.displayName}?`,
              message: describeCliInstallPlan(plan),
              confirmLabel: "Install verified package",
              cancelLabel: "Not now",
              tone: "default",
            });
            if (!approved) throw new Error(`${cliAdapter.displayName} installation was cancelled.`);
            const installed = await installCliProvider(plan);
            if (!installed.success) throw new Error(installed.message);
            status = await cliAdapter.probe();
          }
          if (setup.providerId === "anthropic") setClaudeCodeStatus(status);
          if (!status.installed || !status.authenticated) {
            throw new Error(`${status.message} Sign in with \`${cliAdapter.loginCommand}\`, then retry.`);
          }
          const models = cliAdapter.models(instanceId, status);
          if (!models.length) throw new Error(`${cliAdapter.displayName} reported no available models.`);
          await cliAdapter.verify?.();
          setSetup({
            ...setup,
            phase: "models",
            models,
            defaultModelId: models[0].id,
            verificationMessage: `Official ${cliAdapter.displayName} account connection verified on this Mac.`,
            liveTested: true,
            credentialRefreshable: false,
          });
        } catch (error) {
          setSetupError(error instanceof Error ? error.message : `${cliAdapter.displayName} could not be verified.`);
        } finally {
          setIsConnecting(false);
        }
        return;
      }

      if (setup.providerId === "kimi") {
        const isKimi = setup.providerId === "kimi";
        const commandPrefix = isKimi ? "kimi" : "grok";
        let auth: DeviceAuth | undefined;
        let cancelled = false;
        try {
          auth = await invoke<DeviceAuth>(`begin_${commandPrefix}_device_auth`, {
            query: { providerInstanceId: instanceId },
          });
          setSetup((current) =>
            current
              ? {
                  ...current,
                  verificationMessage: `${isKimi ? "Kimi Code" : "Grok"} verification code: ${auth?.userCode}`,
                }
              : current,
          );
          cancelOAuthRef.current = () => {
            cancelled = true;
            if (auth) {
              void invoke(`cancel_${commandPrefix}_device_auth`, { state: auth.state });
            }
          };
          await openUrl(auth.verificationUriComplete ?? auth.verificationUri);
          const completed = await invoke<{
            expiresAt: string | null;
            refreshable: boolean;
          }>(`complete_${commandPrefix}_device_auth`, { state: auth.state });
          if (cancelled) return;
          const adapter = await getProviderAdapter(setup.providerId);
          const verification = await adapter.testConnection(instanceId);
          const models = verification.discoveredModels;
          setSetup({
            ...setup,
            phase: "models",
            models,
            defaultModelId: models[0]?.id ?? "",
            verificationMessage: verification.message,
            liveTested: verification.level === "live" && verification.ok,
            credentialExpiresAt: completed.expiresAt ?? undefined,
            credentialRefreshable: completed.refreshable,
          });
        } catch (error) {
          if (!cancelled) {
            setSetupError(
              typeof error === "string"
                ? error
                : error instanceof Error
                  ? error.message
                  : `${isKimi ? "Kimi Code" : "Grok"} account sign-in could not be completed.`,
            );
          }
        } finally {
          cancelOAuthRef.current = null;
          setIsConnecting(false);
        }
        return;
      }

      let completedUnlisten: UnlistenFn | undefined;
      let errorUnlisten: UnlistenFn | undefined;
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      let settled = false;
      let cancelled = false;
      let oauthState: string | undefined;
      let resolvePending: ((event: ProviderOAuthCompletedEvent) => void) | undefined;
      let rejectPending: ((reason: Error) => void) | undefined;
      const cleanup = () => {
        completedUnlisten?.();
        completedUnlisten = undefined;
        errorUnlisten?.();
        errorUnlisten = undefined;
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = undefined;
        }
      };

      try {
        const completion = new Promise<ProviderOAuthCompletedEvent>(
          (resolve, reject) => {
            resolvePending = resolve;
            rejectPending = reject;
          },
        );

        cancelOAuthRef.current = () => {
          cancelled = true;
          if (!settled) rejectPending?.(new Error("Account sign-in was cancelled."));
          cleanup();
          if (oauthState) {
            void cancelProviderOAuth({
              providerId: setup.providerId,
              state: oauthState,
            });
          }
        };

        const unlistenCompleted = await listen<ProviderOAuthCompletedEvent>(
          "provider-oauth://completed",
          ({ payload }) => {
            if (
              payload.providerId !== setup.providerId ||
              payload.providerInstanceId !== instanceId ||
              payload.state !== oauthState
            ) return;
            settled = true;
            resolvePending?.(payload);
          },
        );
        if (cancelled) {
          unlistenCompleted();
          return;
        }
        completedUnlisten = unlistenCompleted;

        const unlistenError = await listen<ProviderOAuthErrorEvent>(
          "provider-oauth://error",
          ({ payload }) => {
            if (
              payload.providerId !== setup.providerId ||
              payload.providerInstanceId !== instanceId ||
              payload.state !== oauthState
            ) return;
            settled = true;
            rejectPending?.(new Error(payload.message));
          },
        );
        if (cancelled) {
          unlistenError();
          return;
        }
        errorUnlisten = unlistenError;
        timeoutId = setTimeout(() => {
          settled = true;
          rejectPending?.(new Error("Account sign-in timed out. Please try again."));
        }, OAUTH_FRONTEND_TIMEOUT_MS);

        const oauth = await startProviderOAuth(setup.providerId, instanceId);
        oauthState = oauth.state;
        if (cancelled) {
          await cancelProviderOAuth({
            providerId: setup.providerId,
            state: oauth.state,
          });
          return;
        }
        await openUrl(oauth.authorizationUrl);
        const completed = await completion;

        const adapter = await getProviderAdapter(setup.providerId);
        const verification = await adapter.testConnection(instanceId);
        if (cancelled) return;
        const models = verification.discoveredModels;
        if (setup.providerId === "microsoft-graph") {
          const identity = await invoke<{
            displayName: string;
            principalName?: string;
          }>("get_microsoft_graph_identity");
          const instance = createProviderInstance({
            id: instanceId,
            providerId: setup.providerId,
            displayName: setup.provider,
            credentialKind: "oauth",
            models: [],
            defaultModelId: "",
            liveTested: true,
            credentialExpiresAt: completed.expiresAt ?? undefined,
            credentialRefreshable: completed.refreshable,
          });
          await saveProviderInstance(instance);
          setProviderInstances(listProviderInstances());
          setSetup({
            ...setup,
            phase: "manage",
            models: [],
            defaultModelId: "",
            verificationMessage: `Connected as ${identity.displayName}${identity.principalName ? ` (${identity.principalName})` : ""}.`,
            liveTested: true,
            credentialExpiresAt: completed.expiresAt ?? undefined,
            credentialRefreshable: completed.refreshable,
          });
          onConnect(setup.provider, setup.method);
          return;
        }
        setSetup({
          ...setup,
          phase: "models",
          models,
          defaultModelId: models[0]?.id ?? "",
          verificationMessage: verification.message,
          liveTested: verification.level === "live" && verification.ok,
          credentialExpiresAt: completed.expiresAt ?? undefined,
          credentialRefreshable: completed.refreshable,
        });
      } catch (error) {
        if (oauthState) {
          await cancelProviderOAuth({
            providerId: setup.providerId,
            state: oauthState,
          }).catch(() => false);
        }
        if (!cancelled) {
          const message = error instanceof Error
            ? error.message
            : "AI‑OS could not complete account sign-in.";
          setSetupError(
            setup.providerId === "google" && message.includes("denied")
              ? "Google denied this sign-in. AI‑OS cannot bypass a 403 access_denied response. The OAuth app owner must add this account as a test user or complete Google verification, then retry."
              : message,
          );
        }
      } finally {
        cleanup();
        cancelOAuthRef.current = null;
        setIsConnecting(false);
      }
      return;
    }

    if (!apiKey.trim()) {
      setSetupError("Enter an API key to continue.");
      return;
    }

    setIsConnecting(true);
    setSetupError("");
    let credentialSaved = false;
    try {
      const instanceId = providerInstanceId(setup.providerId);
      await saveProviderApiKey(instanceId, apiKey);
      credentialSaved = true;
      const adapter =
        await getProviderAdapter(
          setup.providerId,
        );
      const verification =
        await adapter.testConnection(
          instanceId,
        );
      const models = verification.discoveredModels;
      setApiKey("");
      setSetup({
        ...setup,
        phase: "models",
        models,
        defaultModelId: models[0]?.id ?? "",
        verificationMessage: verification.message,
        liveTested: verification.level === "live" && verification.ok,
      });
    } catch (error) {
      if (credentialSaved) {
        await deleteProviderCredential(
          providerInstanceId(setup.providerId),
        ).catch(() => undefined);
      }
      setSetupError(
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "AI‑OS could not verify this Provider connection.",
      );
    } finally {
      setIsConnecting(false);
    }
  }

  async function finishProviderSetup() {
    if (!setup || setup.phase !== "models" || !setup.defaultModelId || isConnecting) return;
    setIsConnecting(true);
    setSetupError("");
    try {
      const instance = createProviderInstance({
        id: providerInstanceId(setup.providerId),
        providerId: setup.providerId,
        displayName: setup.provider,
        credentialKind:
          CLI_ACCOUNT_PROVIDERS.has(setup.providerId) && setup.method === "account"
            ? "local"
            : setup.method === "account"
              ? "oauth"
              : "api-key",
        models: setup.models,
        defaultModelId: setup.defaultModelId,
        liveTested: setup.liveTested,
        credentialExpiresAt: setup.credentialExpiresAt,
        credentialRefreshable: setup.credentialRefreshable,
      });
      await saveProviderInstance(instance);
      const selected = instance.models.find((model) => model.id === setup.defaultModelId);
      if (selected) {
        try {
          await verifyProviderModel(instance.id, selected.remoteModelId);
        } catch (error) {
          setProviderInstances(listProviderInstances());
          setSetup({ ...setup, phase: "manage", models: listProviderInstances().find((item) => item.id === instance.id)?.models ?? instance.models,
            verificationMessage: "Catalog saved. A model must pass real generation before it is offered for use." });
          setSetupError(typeof error === "string" ? error : error instanceof Error ? error.message : "Model generation verification failed.");
          return;
        }
      }
      setProviderInstances(listProviderInstances());
      onConnect(setup.provider, setup.method);
      setSetup(null);
    } catch (error) {
      setSetupError(
        typeof error === "string"
          ? error
          : error instanceof Error
          ? error.message
          : "AI‑OS could not save this Provider.",
      );
    } finally {
      setIsConnecting(false);
    }
  }

  async function disconnectProvider() {
    if (!setup || isConnecting) return;
    setIsConnecting(true);
    setSetupError("");
    const instanceId = providerInstanceId(setup.providerId);
    try {
      const instance = providerInstances.find((candidate) => candidate.id === instanceId);
      if (instance?.credential.kind !== "local") {
        await deleteProviderCredential(instanceId);
      }
      await removeProviderInstance(instanceId);
      setProviderInstances(listProviderInstances());
      setSetup(null);
    } catch {
      setSetupError("AI‑OS could not remove this Provider credential from macOS Keychain.");
    } finally {
      setIsConnecting(false);
    }
  }

  useEffect(() => {
    const show = (event: Event) => {
      const snapshot = (event as CustomEvent<SoftwareUpdateSnapshot>).detail;
      setCliVersions(snapshot.cli);
      setCliVersionsCheckedAt(snapshot.checkedAt);
    };
    window.addEventListener(SOFTWARE_UPDATES_EVENT, show);
    return () => window.removeEventListener(SOFTWARE_UPDATES_EVENT, show);
  }, []);

  async function checkCliUpdates() {
    setCliUpdateBusy("check");
    setCliUpdateMessage("");
    try {
      const snapshot = await checkSoftwareUpdates();
      const updates = snapshot.cli.filter((report) => report.updateAvailable).length;
      setCliUpdateMessage(updates ? `${updates} update${updates === 1 ? "" : "s"} available.` : "All installed CLI tools are up to date.");
    } catch (error) {
      setCliUpdateMessage(error instanceof Error ? error.message : "The update check failed.");
    } finally {
      setCliUpdateBusy(undefined);
    }
  }

  async function updateCli(report: CliVersionReport) {
    setCliUpdateBusy(report.providerId);
    setCliUpdateMessage("");
    try {
      const plan = await prepareCliProviderInstall(report.providerId);
      const approved = await dialog.confirm({
        title: `Update ${plan.displayName} to ${plan.version}?`,
        message: describeCliInstallPlan(plan),
        confirmLabel: "Update and verify",
        cancelLabel: "Not now",
        tone: "default",
      });
      if (!approved) return;
      const result = await installCliProvider(plan);
      setCliUpdateMessage(result.message);
      await checkSoftwareUpdates();
    } catch (error) {
      setCliUpdateMessage(error instanceof Error ? error.message : `${report.displayName} could not be updated.`);
    } finally {
      setCliUpdateBusy(undefined);
    }
  }

  const installedCliTools = cliVersions.filter((report) => report.installed);

  return (
    <section className="my-ai-page">

      <header className="my-ai-header">
        <div>
          <p className="settings-kicker">Intelligence</p>
          <h1>My AI</h1>
          <p>Connect the AI accounts and local models you want AI‑OS to use.</p>
        </div>
      </header>

      <SectionTabs
        activePage="My AI"
        label="My AI sections"
        onPageChange={onPageChange}
        items={[
          { label: "AI Models", page: "My AI" },
          { label: "Agents", page: "Agents" },
          { label: "Accounts & Services", page: "Accounts & Services" },
        ]}
      />

      <div className="provider-section-heading">
        <h2>Cloud AI</h2>
        <span>Choose account sign-in or an API key</span>
      </div>

      <div className="provider-grid">
        {cloudProviders.map((provider) => {
          const descriptor = providerAdapters.find(
            (candidate) => candidate.providerId === provider.id,
          );
          const supportsApiKey =
            descriptor?.authenticationMethods.includes("api-key") === true;
          const supportsAccountSignIn =
            CLI_ACCOUNT_PROVIDERS.has(provider.id)
              ? descriptor?.authenticationMethods.includes("cli-account") === true
              : descriptor?.authenticationMethods.some(
                  (method) =>
                    method === "device-code" ||
                    method === "oauth-pkce" ||
                    method === "oauth-loopback" ||
                    method === "imported-credential",
                ) === true;
          const instance = providerInstances.find(
            (candidate) => candidate.providerId === provider.id,
          );
          const legacyConnected = configuredProviderIds.has(provider.id) && !instance;
          const apiKeyOnly = supportsApiKey && !supportsAccountSignIn;

          const statusLabel =
            instance?.connectionState === "connected"
              ? "Connected"
              : instance?.connectionState === "ready-for-test"
                ? "Saved"
                : instance?.connectionState === "refresh-required"
                  ? "Refresh needed"
                  : instance?.connectionState === "expired"
                    ? "Sign in again"
                : legacyConnected
                  ? "Connected"
                  : "Not connected";
          return (
          <article key={provider.id} className="provider-card">
            <div className="provider-card-heading">
              <div className={`provider-mark provider-mark-${provider.id}`}>{provider.mark}</div>
              <div>
                <h3>{provider.name}</h3>
                <p>{provider.description}</p>
              </div>
              <span className={`connection-badge ${configuredProviderIds.has(provider.id) ? "connection-badge-ready" : ""}`}>
                {statusLabel}
              </span>
            </div>

            <div className="provider-models">
              {provider.models.map((model) => <span key={model}>{model}</span>)}
            </div>

            <div className="provider-actions">
<button
                type="button"
                className="provider-primary"
                onClick={() => {
                  if (instance) {
                    openManage(instance.id);
                    return;
                  }

                  if (legacyConnected) {
                    if (supportsAccountSignIn) {
                      openSetup(provider.name, "account", provider.id);
                    } else if (supportsApiKey) {
                      openSetup(provider.name, "api-key", provider.id);
                    } else {
                      console.warn(
                        `[MyAiPage] Provider ${provider.id} is configured but has no instance and no supported setup method.`,
                      );
                    }
                    return;
                  }

                  if (supportsAccountSignIn) {
                    openSetup(provider.name, "account", provider.id);
                    return;
                  }
                  if (supportsApiKey) {
                    openSetup(provider.name, "api-key", provider.id);
                    return;
                  }

                  console.warn(
                    `[MyAiPage] Provider ${provider.id} has no available action.`,
                  );
                }}
              >
                {instance
                  ? "Manage connection"
                  : legacyConnected
                    ? "Reconnect"
                    : supportsAccountSignIn
                      ? provider.accountLabel
                      : "Use API key"}
              </button>
                            {supportsApiKey && !apiKeyOnly && (
                  <button
                    type="button"
                    className="provider-secondary"
                    onClick={() =>
                      openSetup(
                        provider.name,
                        "api-key",
                        provider.id,
                      )
                    }
                  >
                    Use API key
                  </button>
                )}
            </div>
          </article>
          );
        })}
      </div>

      <div className="provider-section-heading local-heading">
        <h2>On this Mac</h2>
        <span>Private models that run locally</span>
      </div>

      {localModelRecommendations?.preferred && (
        <div className="local-model-recommendation" aria-label="Recommended local model">
          <div>
            <strong>Recommended for this Mac</strong>
            <span>{localModelRecommendations.preferred.modelId}</span>
          </div>
          <p>
            {localModelRecommendations.preferred.preferredQuantization ?? "Quant unknown"}
            {localModelRecommendations.preferred.recommendedContext != null
              ? ` · ${localModelRecommendations.preferred.recommendedContext.toLocaleString()} context`
              : ""}
            {localModelRecommendations.preferred.estimatedMemoryGb != null
              ? ` · ${localModelRecommendations.preferred.estimatedMemoryGb.toFixed(1)} GB estimated memory`
              : ""}
          </p>
          <small>Advice only · downloads always require confirmation</small>
        </div>
      )}

      {omlxInstance && omlxRuntime?.supported && (
        <article className="local-provider-card local-provider-card-omlx">
          <div className="provider-card-heading">
            <div className="provider-mark provider-mark-omlx">M</div>
            <div>
              <h3>oMLX</h3>
              <p>{omlxState.description}</p>
            </div>
            <span className={`connection-badge ${omlxRuntime.running ? "connection-badge-ready" : ""}`}>
              {omlxState.badge}
            </span>
          </div>
          <div className="local-model-list">
            {(omlxAdminModels.length
              ? omlxAdminModels
              : omlxInstance.models.map((model) => ({
                  name: model.remoteModelId,
                  displayName: model.displayName,
                  size: 0,
                  sizeFormatted: "",
                }))).slice(0, 4).map((model) => (
              <div key={model.name}>
                <span>{model.displayName}</span>
                <div className="local-model-actions">
                  <button type="button" onClick={() => void inspectOmlxModel(model.name)}>
                    Details
                  </button>
                  <button type="button" onClick={() => void revealOmlxModel(model.name)}>
                    Show in Finder
                  </button>
                  <button type="button" onClick={() => void removeOmlxModel(model.name)}>
                    Delete
                  </button>
                </div>
                {omlxInstance.models.some((entry) => entry.isDefault && (
                  entry.remoteModelId === model.name || entry.displayName === model.displayName
                )) && <small>Default</small>}
                {assessmentBadge(localModelAssessments[`omlx:${model.name}`]) && (
                  <small>{assessmentBadge(localModelAssessments[`omlx:${model.name}`])}</small>
                )}
              </div>
            ))}
          </div>
          <div className="local-provider-actions">
            {!omlxRuntime.running ? (
              <button
                type="button"
                className="manage-models-button local-provider-primary"
                disabled={omlxLoading}
                onClick={() => void refreshOmlx(true)}
              >
                {omlxLoading ? "Starting oMLX…" : "Start oMLX"}
              </button>
            ) : (
              <button
                type="button"
                className="manage-models-button local-provider-primary"
                onClick={() => openManage("omlx-local")}
              >
                Manage connection <span>→</span>
              </button>
            )}
            <button
              type="button"
              className="local-provider-refresh"
              disabled={omlxLoading || !omlxRuntime.running}
              onClick={() => void downloadOmlxModel()}
            >
              Pull model
            </button>
            <button
              type="button"
              className="local-provider-refresh"
              disabled={omlxLoading}
              onClick={() => void refreshOmlx(false)}
            >
              {omlxLoading ? "Checking…" : "Refresh"}
            </button>
            <button
              type="button"
              className="local-provider-refresh"
              disabled={omlxLoading}
              onClick={() => void disconnectOmlx()}
            >
              Disconnect
            </button>
          </div>
        </article>
      )}

      <div className="provider-section-heading cli-tools-heading">
        <h2>Command-line tools</h2>
        <span>Official CLIs that connect your own accounts · checked daily</span>
      </div>

      <div className="cli-tools-list" aria-label="Command-line tool versions">
        {installedCliTools.map((report) => (
          <div className="cli-tool-row" key={report.providerId}>
            <div>
              <strong>{report.displayName}</strong>
              <small>
                {report.installedVersion ?? "Version unknown"}
                {report.sourceLabel && ` · ${report.sourceLabel}`}
                {report.latestVersion && !report.updateAvailable && " · up to date"}
              </small>
            </div>
            {report.updateAvailable && report.canUpdate ? (
              <button
                type="button"
                disabled={cliUpdateBusy !== undefined}
                onClick={() => void updateCli(report)}
              >
                {cliUpdateBusy === report.providerId ? "Updating…" : `Update to ${report.latestVersion}`}
              </button>
            ) : report.updateAvailable ? (
              <small>{report.message}</small>
            ) : null}
          </div>
        ))}
        {!installedCliTools.length && (
          <p className="cli-tools-empty">
            {cliVersionsCheckedAt ? "No supported CLI is installed." : "Not checked yet."}
          </p>
        )}
        <footer>
          <small>
            {cliVersionsCheckedAt ? `Last checked ${new Date(cliVersionsCheckedAt).toLocaleString()}` : ""}
            {cliUpdateMessage && ` · ${cliUpdateMessage}`}
          </small>
          <button type="button" disabled={cliUpdateBusy !== undefined} onClick={() => void checkCliUpdates()}>
            {cliUpdateBusy === "check" ? "Checking…" : "Check for updates"}
          </button>
        </footer>
      </div>

      <div className="provider-section-heading catalog-heading">
        <h2>Add another Provider</h2>
        <span>Pick a service — AI‑OS will guide the setup</span>
      </div>

      <div className="provider-catalog">
        {providerCatalog
          .filter((provider) => !configuredProviderIds.has(provider.id))
          .map((provider) => (
          <article key={provider.id} className="catalog-provider">
            <div className={`catalog-provider-mark catalog-provider-mark-${provider.id}`}>
              {provider.mark}
            </div>
            <div>
              <h3>{provider.name}</h3>
              <p>{provider.description}</p>
              <small>{provider.connection}</small>
            </div>
            <button
              type="button"
              aria-label={`Add ${provider.name}`}
              onClick={() => {
                openSetup(provider.name, "api-key", provider.id);
              }}
            >
              Add
            </button>
          </article>
        ))}
      </div>

      {setup && (() => {
        const setupCli = getCliProviderAdapter(setup.providerId);
        const setupDescriptor = providerAdapters.find(
          (descriptor) =>
            descriptor.providerId === setup.providerId,
        );
        const accountSignInAvailable =
          CLI_ACCOUNT_PROVIDERS.has(setup.providerId)
            ? setupDescriptor?.authenticationMethods.includes("cli-account") === true
            : setupDescriptor?.authenticationMethods.includes("device-code") === true ||
              (isProviderOAuthConfigured(setup.providerId) &&
              setupDescriptor?.authenticationMethods.some(
                (method) =>
                  method === "oauth-pkce" ||
                  method === "oauth-loopback" ||
                  method === "device-code" ||
                  method === "imported-credential",
              ) === true);

        return (
        <div className="provider-setup-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) closeSetup();
        }}>
          <section className="provider-setup-dialog" role="dialog" aria-modal="true" aria-labelledby="provider-setup-title">
            <header>
              <div className="provider-setup-mark">{setup.provider.slice(0, 1)}</div>
              <div>
                <p>Connect Provider</p>
                <h2 id="provider-setup-title">{setup.provider}</h2>
              </div>
              <button type="button" className="provider-setup-close" aria-label="Close Provider setup" onClick={closeSetup}>×</button>
            </header>

            {setup.phase === "credential" && <div className="provider-setup-methods" role="tablist" aria-label="Connection method">
              <button
                type="button"
                role="tab"
                aria-selected={setup.method === "account"}
                aria-disabled={!accountSignInAvailable}
                disabled={!accountSignInAvailable}
                title={
                  accountSignInAvailable
                    ? undefined
                    : "Account sign-in requires an operational Provider authentication adapter."
                }
                onClick={() =>
                  setSetup((current) =>
                    current && {
                      ...current,
                      method: "account",
                    },
                  )
                }
              >
                <span>Account sign-in</span>
                <small>
                  {accountSignInAvailable
                    ? "Connect your Provider account"
                    : "Coming later"}
                </small>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={setup.method === "api-key"}
                onClick={() => setSetup((current) => current && { ...current, method: "api-key" })}
              >
                <span>API key</span>
                <small>Connect a developer account</small>
              </button>
            </div>}

            <div className="provider-setup-body">
              {setup.phase === "manage" ? (
                <>
                  <h3>Provider details</h3>
                  <p>{setup.verificationMessage}</p>
                  <div className="provider-instance-summary">
                    <div>
                      <span>Default model</span>
                      <strong>{setup.models.find((model) => model.id === setup.defaultModelId)?.displayName ?? "Not selected"}</strong>
                    </div>
                    <div>
                      <span>Credential</span>
                      <strong>{setup.method === "account" ? "Account sign-in" : "macOS Keychain"}</strong>
                    </div>
                    {setup.method === "account" && <div>
                      <span>Account access</span>
                      <strong>
                        {setup.credentialExpiresAt
                          ? new Date(setup.credentialExpiresAt).getTime() > Date.now()
                            ? `Until ${new Date(setup.credentialExpiresAt).toLocaleString()} · renews automatically`
                            : setup.credentialRefreshable
                              // The short-lived token renews on the next request.
                              ? "Signed in · renews automatically on next use"
                              : "Expired"
                          : CLI_ACCOUNT_PROVIDERS.has(setup.providerId)
                            ? "Managed by the official CLI"
                            : "No expiry reported"}
                      </strong>
                    </div>}
                  </div>
                  {setup.method === "account" && setup.credentialRefreshable && (
                    <button
                      type="button"
                      className="provider-refresh-button"
                      disabled={isConnecting}
                      onClick={() => void refreshAccountCredential()}
                    >
                      {isConnecting ? "Refreshing…" : "Refresh account access"}
                    </button>
                  )}
                  {setupDescriptor?.supportsModelDiscovery && (
                    <button
                      type="button" className="provider-refresh-button" disabled={isConnecting}
                      onClick={() => void refreshProviderModels()}
                    >
                      {isConnecting ? "Refreshing…" : "Refresh model catalog"}
                    </button>
                  )}
                  {!!setup.models.length && <>
                    <label>Model to verify
                      <select aria-label="Model to verify" disabled={isConnecting}
                        value={verificationModelId || setup.defaultModelId}
                        onChange={(event) => setVerificationModelId(event.target.value)}>
                        {setup.models.map((model) => <option key={model.id} value={model.id}>
                          {model.displayName} — {model.availability ?? "not generation-verified"}
                        </option>)}
                      </select>
                    </label>
                    <button type="button" className="provider-refresh-button" disabled={isConnecting}
                      onClick={() => { const model = setup.models.find((item) => item.id === (verificationModelId || setup.defaultModelId)); if (model) void revalidateModel(model); }}>
                      Verify selected model (real request)
                    </button>
                    <p>Only generation-verified models appear in Chat, Council and Arena.</p>
                  </>}
                  {setup.method === "account" && (
                    <button
                      type="button"
                      className="provider-refresh-button"
                      disabled={
                        setup.providerId === "anthropic"
                          ? claudeCodeStatus?.authenticated !== true
                          : CLI_ACCOUNT_PROVIDERS.has(setup.providerId)
                            ? false
                          : setupDescriptor?.authenticationMethods.includes(
                                "device-code",
                              ) !== true &&
                            !isProviderOAuthConfigured(setup.providerId)
                      }
                      onClick={() => openSetup(setup.provider, "account", setup.providerId)}
                    >
                      Sign in again
                    </button>
                  )}
                  <button
                    type="button"
                    className="provider-disconnect-button"
                    disabled={isConnecting}
                    onClick={() => void disconnectProvider()}
                  >
                    {isConnecting ? "Disconnecting…" : "Disconnect Provider"}
                  </button>
                  {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
                </>
              ) : setup.phase === "models" ? (
                <>
                  <h3>Choose the default model</h3>
                  <p>{CLI_ACCOUNT_PROVIDERS.has(setup.providerId) && setup.method === "account"
                    ? `${getCliProviderAdapter(setup.providerId)?.displayName ?? "The official CLI"} manages the account credential. AI‑OS stores only this connection and your model preference.`
                    : "The credential is safely stored. These models were returned by the Provider connection."}</p>
                  <div className="provider-model-choice">
                    {setup.models.map((model) => (
                      <label key={model.id}>
                        <input
                          type="radio"
                          name="default-provider-model"
                          value={model.id}
                          checked={setup.defaultModelId === model.id}
                          onChange={() => setSetup({ ...setup, defaultModelId: model.id })}
                        />
                        <span>{model.displayName}</span>
                        <small>{setup.defaultModelId === model.id ? "Default" : "Available"}</small>
                      </label>
                    ))}
                  </div>
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p>
                      <strong>{setup.liveTested ? "Connection tested" : "Credential verified"}</strong>
                      <small>{setup.verificationMessage}</small>
                    </p>
                  </div>
                </>
              ) : setup.method === "account" ? (
                <>
                  <h3>{setupCli?.setupTitle ?? "Sign in in your browser"}</h3>
                  <p>{setup.providerId === "anthropic" && claudeCodeStatus?.installed
                    ? "AI‑OS uses the official Claude Code CLI and never reads or stores its subscription credential."
                    : setupCli
                      ? setupCli.setupInstructions
                      : setup.providerId === "kimi"
                        ? "AI‑OS will open Kimi Code's official device authorization page for your coding account."
                      : `AI‑OS will open the official ${setup.provider} sign-in page. Your password is never entered into AI‑OS.`}</p>
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p><strong>Protected connection</strong><small>{setupCli
                      ? setupCli.protectionNote
                        : setup.providerId === "kimi"
                          ? "AI‑OS uses Kimi Code's official device flow. Tokens stay in macOS Keychain and requests use the managed coding API."
                        : "AI‑OS uses PKCE and a one-time local callback. The authorization code and tokens stay in the native security layer."}</small></p>
                  </div>
                  {setup.providerId === "google" && !setupCli && (
                    <div className="provider-security-note">
                      <span>!</span>
                      <p><strong>Google approval may be required</strong><small>If Google shows 403 access_denied, AI‑OS cannot bypass it. The OAuth app owner must add this account as a test user or complete Google verification, then you can retry here.</small></p>
                    </div>
                  )}
                  {setup.providerId === "kimi" && setup.verificationMessage && (
                    <div className="provider-security-note">
                      <span>→</span>
                      <p>
                        <strong>Confirm the code shown by {setup.providerId === "kimi" ? "Kimi Code" : "xAI"}</strong>
                        <small>{setup.verificationMessage}</small>
                      </p>
                    </div>
                  )}
                </>
              ) : (
                <>
                  <h3>Add an API key securely</h3>
                  <p>The key is sent directly to the native AI‑OS security layer and is never stored in browser or local app settings.</p>
                  <label className="secure-key-field">
                    <span>API key</span>
                    <input
                      type="password"
                      autoComplete="off"
                      spellCheck={false}
                      value={apiKey}
                      onChange={(event) => setApiKey(event.target.value)}
                      placeholder="Paste your API key"
                    />
                  </label>
                  {setup.providerId === "google" && (
                    <div className="provider-security-note">
                      <span>→</span>
                      <p><strong>Use your own Gemini API key</strong><small>Create a key with your Google account at aistudio.google.com/apikey. Requests use that key's own quota (Google AI Studio has a free tier); no one else's account or project is billed. Google no longer serves personal accounts through Gemini CLI and recommends an API key for apps like AI‑OS.</small></p>
                    </div>
                  )}
                  <div className="provider-security-note">
                    <span>✓</span>
                    <p><strong>Stored in macOS Keychain</strong><small>AI‑OS can check whether a key exists, but the native layer never returns the secret to the interface.</small></p>
                  </div>
                </>
              )}
              {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
            </div>

            {setup.phase !== "manage" && <footer>
              {setupError && <p className="provider-setup-error" role="alert">{setupError}</p>}
              <button type="button" className="provider-setup-cancel" onClick={closeSetup}>Cancel</button>
              <button
                type="button"
                className="provider-setup-continue"
                disabled={isConnecting || (setup.phase === "credential" && setup.method === "api-key" && !apiKey.trim())}
                onClick={() => setup.phase === "models" ? void finishProviderSetup() : void continueSetup()}
              >
                {isConnecting
                  ? setup.phase === "models"
                    ? "Saving Provider…"
                    : setup.method === "account"
                      ? setupCli
                        ? `Checking ${setupCli.displayName}…`
                        : "Waiting for sign-in…"
                      : "Saving…"
                  : setup.phase === "models"
                    ? "Save Provider"
                    : setup.method === "account"
                      ? setupCli
                        ? `Verify ${setupCli.displayName}`
                        : "Continue in browser"
                      : "Save and choose model"}
                <span>→</span>
              </button>
            </footer>}
          </section>
        </div>
        );
      })()}
    </section>
  );
}

export default MyAiPage;
