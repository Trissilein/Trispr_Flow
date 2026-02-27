import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { settings, ollamaPullProgress } from "./state";
import { showToast } from "./toast";
import { isExactModelTagMatch, normalizeModelTag } from "./ollama-tag-utils";
import { applyHelpTooltip } from "./ai-refinement-help";
import type {
  OllamaImportResult,
  OllamaPullProgress as OllamaPullProgressType,
  OllamaRuntimeDetectResult,
  OllamaRuntimeDownloadResult,
  OllamaRuntimeHealth,
  OllamaRuntimeInstallComplete,
  OllamaRuntimeInstallError,
  OllamaRuntimeInstallProgress,
  OllamaRuntimeInstallResult,
  OllamaRuntimeStartResult,
  OllamaRuntimeVerifyResult,
} from "./types";

const DEFAULT_LOCAL_ENDPOINT = "http://localhost:11434";
const DEFAULT_RUNTIME_VERSION = "0.17.0";
const AUTOSTART_WARNING_COOLDOWN_MS = 60_000;
const LOCAL_OLLAMA_HOSTS = new Set(["localhost", "127.0.0.1"]);
const BACKGROUND_START_POLL_INTERVAL_MS = 2_000;
const BACKGROUND_START_POLL_MAX_MS = 30_000;

type WizardStage =
  | "not_detected"
  | "download_runtime"
  | "install_runtime"
  | "start_runtime"
  | "verify_runtime"
  | "failed"
  | "select_model_source"
  | "ready";

type CardStatus = "available" | "downloaded" | "active";

export const OLLAMA_RECOMMENDED_MODELS = [
  {
    name: "qwen3:8b",
    label: "Qwen3 8B",
    size_gb: 5.2,
    profile: "Fast Fallback",
    description: "Fastest recommended model. Ideal for low latency.",
  },
  {
    name: "qwen3:14b",
    label: "Qwen3 14B",
    size_gb: 9.0,
    profile: "Primary",
    description: "Recommended main model. Best balance of quality and speed.",
  },
  {
    name: "mistral-small3.1:24b",
    label: "Mistral Small 3.1 24B",
    size_gb: 15.0,
    profile: "Quality",
    description: "Highest quality. Requires 16+ GB RAM. Optional.",
  },
];

let installedOllamaModels: Array<{ name: string; size_bytes: number }> = [];
const activeOllamaPulls = new Set<string>();

let runtimeDetect: OllamaRuntimeDetectResult | null = null;
let runtimeVerify: OllamaRuntimeVerifyResult | null = null;
let runtimeInstallProgress: OllamaRuntimeInstallProgress | null = null;
let runtimeInstallError: OllamaRuntimeInstallError | null = null;
let runtimeHealth: OllamaRuntimeHealth | null = null;
let runtimeBusyAction: string | null = null;
let runtimeStateRefreshInFlight: Promise<void> | null = null;
let runtimeStateLastRefreshMs = 0;
let lastAutostartWarningMs = 0;
let runtimeBackgroundStartUntilMs = 0;
let runtimeBackgroundStartPollInFlight: Promise<void> | null = null;

let renderFrame: number | null = null;
const PASSIVE_RUNTIME_REFRESH_TTL_MS = 1500;

const RUNTIME_ACTION_LABELS: Record<string, string> = {
  detect: "Detecting runtime...",
  install: "Installing local runtime...",
  "ensure-runtime": "Preparing local runtime...",
  "use-system": "Switching to system runtime...",
  start: "Starting runtime...",
  verify: "Verifying runtime...",
  import: "Importing model...",
  refresh: "Refreshing runtime and models...",
};

type OllamaRuntimePrimaryAction = "install" | "start" | "ready";
type RuntimeStateRefreshOptions = {
  verify?: boolean;
  skipDetect?: boolean;
  force?: boolean;
};

export type OllamaRuntimeCardState = {
  detected: boolean;
  healthy: boolean;
  source: string;
  version: string;
  endpoint: string;
  busy: boolean;
  backgroundStarting: boolean;
  busyAction: string | null;
  stage: WizardStage;
  primaryAction: OllamaRuntimePrimaryAction;
  primaryLabel: string;
  primaryDisabled: boolean;
  detail: string;
};

function isOllamaProvider(): boolean {
  return settings?.ai_fallback?.provider === "ollama";
}

function isStrictLocalEndpoint(endpoint: string): boolean {
  try {
    const parsed = new URL(endpoint);
    if (parsed.protocol !== "http:") return false;
    if (!LOCAL_OLLAMA_HOSTS.has(parsed.hostname.toLowerCase())) return false;
    if (parsed.port && parsed.port !== "11434") return false;
    if (parsed.pathname && parsed.pathname !== "/") return false;
    if (parsed.search || parsed.hash) return false;
    return true;
  } catch {
    return false;
  }
}

function isLocalEndpoint(endpoint: string): boolean {
  try {
    const parsed = new URL(endpoint);
    if (parsed.protocol !== "http:") return false;
    if (!LOCAL_OLLAMA_HOSTS.has(parsed.hostname.toLowerCase())) return false;
    if (parsed.pathname && parsed.pathname !== "/") return false;
    if (parsed.search || parsed.hash) return false;
    return true;
  } catch {
    return false;
  }
}

function shouldAutoStartLocalRuntime(): boolean {
  if (!settings?.ai_fallback || !settings.providers?.ollama) return false;

  const aiFallback = settings.ai_fallback;
  if (!aiFallback.enabled) return false;
  if (aiFallback.execution_mode !== "local_primary") return false;
  if (aiFallback.provider !== "ollama") return false;

  const endpoint = settings.providers.ollama.endpoint || DEFAULT_LOCAL_ENDPOINT;
  if (aiFallback.strict_local_mode) {
    return isStrictLocalEndpoint(endpoint);
  }
  return isLocalEndpoint(endpoint);
}

function showAutostartWarning(trigger: "bootstrap" | "enable_toggle", message: string): void {
  const now = Date.now();
  if (now - lastAutostartWarningMs < AUTOSTART_WARNING_COOLDOWN_MS) {
    return;
  }
  lastAutostartWarningMs = now;
  showToast({
    type: "warning",
    title: "Local runtime not started",
    message:
      trigger === "enable_toggle"
        ? `Could not auto-start local Ollama: ${message}`
        : `Local Ollama auto-start skipped: ${message}`,
    duration: 5200,
  });
}

function runtimeIsHealthy(): boolean {
  return Boolean(runtimeVerify?.ok || runtimeHealth?.ok);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isRuntimeBackgroundStarting(): boolean {
  return runtimeBackgroundStartUntilMs > Date.now() && !runtimeIsHealthy();
}

function startRuntimeBackgroundPolling(reason: "autostart" | "manual"): void {
  runtimeBackgroundStartUntilMs = Date.now() + BACKGROUND_START_POLL_MAX_MS;
  renderOllamaModelManager();

  if (runtimeBackgroundStartPollInFlight) {
    return;
  }

  runtimeBackgroundStartPollInFlight = (async () => {
    while (Date.now() < runtimeBackgroundStartUntilMs) {
      await refreshOllamaRuntimeState({ force: true, verify: true });
      if (runtimeIsHealthy()) {
        await refreshOllamaInstalledModels();
        runtimeBackgroundStartUntilMs = 0;
        return;
      }
      await sleep(BACKGROUND_START_POLL_INTERVAL_MS);
    }

    if (!runtimeIsHealthy() && reason === "manual") {
      showToast({
        type: "warning",
        title: "Runtime still starting",
        message: "Local runtime did not become reachable yet. You can keep working and retry verify.",
        duration: 4200,
      });
    }
  })().finally(() => {
    runtimeBackgroundStartPollInFlight = null;
    renderOllamaModelManager();
  });
}

function effectiveRuntimeBusyAction(): string | null {
  if (!runtimeBusyAction) return null;
  if ((runtimeBusyAction === "start" || runtimeBusyAction === "ensure-runtime") && runtimeIsHealthy()) {
    return null;
  }
  return runtimeBusyAction;
}

export async function autoStartLocalRuntimeIfNeeded(
  trigger: "bootstrap" | "enable_toggle"
): Promise<void> {
  if (!shouldAutoStartLocalRuntime()) {
    return;
  }

  await refreshOllamaRuntimeState({ force: true });
  const card = getOllamaRuntimeCardState();
  if (card.busy || card.healthy || !card.detected) {
    return;
  }

  if (runtimeBusyAction) {
    return;
  }

  runtimeBusyAction = "start";
  runtimeInstallError = null;
  renderOllamaModelManager();
  try {
    const startResult = await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
    if (startResult.pending_start) {
      startRuntimeBackgroundPolling("autostart");
      await refreshOllamaRuntimeState({ force: true });
    } else {
      await refreshOllamaRuntimeState({ force: true, verify: true });
      if (getOllamaRuntimeCardState().healthy) {
        await refreshOllamaInstalledModels();
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.warn(`Ollama autostart failed (${trigger}):`, message);
    showAutostartWarning(trigger, message);
  } finally {
    runtimeBusyAction = null;
    renderOllamaModelManager();
  }
}

function scheduleRender(): void {
  if (renderFrame !== null) return;

  if (typeof window === "undefined") {
    renderOllamaModelManagerNow();
    return;
  }

  if (typeof window.requestAnimationFrame === "function") {
    renderFrame = window.requestAnimationFrame(() => {
      renderFrame = null;
      renderOllamaModelManagerNow();
    });
    return;
  }

  renderFrame = window.setTimeout(() => {
    renderFrame = null;
    renderOllamaModelManagerNow();
  }, 16) as unknown as number;
}

function computeWizardStage(): WizardStage {
  const progressStage = runtimeInstallProgress?.stage;
  if (progressStage === "download_runtime") return "download_runtime";
  if (progressStage === "install_runtime") return "install_runtime";
  if (progressStage === "start_runtime") return "start_runtime";
  if (progressStage === "verify_runtime") return "verify_runtime";

  if (runtimeInstallError) return "failed";

  if (!runtimeDetect?.found) return "not_detected";
  if (!runtimeIsHealthy()) return "start_runtime";
  if (installedOllamaModels.length === 0) return "select_model_source";
  return "ready";
}

function stageHint(stage: WizardStage): string {
  if (stage === "not_detected") {
    return "Install local Ollama runtime or use an existing local runtime.";
  }
  if (stage === "download_runtime" || stage === "install_runtime") {
    return "Preparing local runtime. This can take a few minutes on first run.";
  }
  if (stage === "start_runtime") {
    return "Runtime exists but is not reachable yet. Start and verify local endpoint.";
  }
  if (stage === "verify_runtime") {
    return "Checking runtime health and model availability.";
  }
  if (stage === "failed") {
    return runtimeInstallError?.error || "Runtime setup failed. Retry or open advanced tools.";
  }
  if (stage === "select_model_source") {
    return "Install a model from the cards below or import from local files.";
  }
  return "Local AI refinement is configured and ready.";
}

function runtimeActionLabel(action: string | null): string {
  if (!action) return "";
  return RUNTIME_ACTION_LABELS[action] || `Running ${action}...`;
}

function resolvePrimaryAction(): {
  action: OllamaRuntimePrimaryAction;
  label: string;
  disabled: boolean;
} {
  const busyAction = effectiveRuntimeBusyAction();
  if (busyAction) {
    return {
      action: runtimeDetect?.found ? "start" : "install",
      label: runtimeActionLabel(busyAction),
      disabled: true,
    };
  }
  if (runtimeIsHealthy()) {
    return {
      action: "ready",
      label: "Runtime ready",
      disabled: true,
    };
  }
  if (isRuntimeBackgroundStarting()) {
    return {
      action: "start",
      label: "Runtime starting in background...",
      disabled: false,
    };
  }
  if (runtimeDetect?.found) {
    return {
      action: "start",
      label: "Start local runtime",
      disabled: false,
    };
  }
  return {
    action: "install",
    label: "Install local runtime",
    disabled: false,
  };
}

export function getOllamaRuntimeCardState(): OllamaRuntimeCardState {
  const source = runtimeDetect?.source || settings?.providers?.ollama?.runtime_source || "manual";
  const version = runtimeDetect?.version || settings?.providers?.ollama?.runtime_version || "unknown";
  const endpoint = settings?.providers?.ollama?.endpoint || DEFAULT_LOCAL_ENDPOINT;
  const busyAction = effectiveRuntimeBusyAction();
  const backgroundStarting = isRuntimeBackgroundStarting();
  const stage = computeWizardStage();
  const primary = resolvePrimaryAction();
  const detail = runtimeInstallProgress?.message
    || (busyAction
      ? runtimeActionLabel(busyAction)
      : backgroundStarting
        ? "Starting runtime in background..."
        : stageHint(stage));

  return {
    detected: Boolean(runtimeDetect?.found),
    healthy: runtimeIsHealthy(),
    source,
    version: version || "unknown",
    endpoint,
    busy: Boolean(busyAction),
    backgroundStarting,
    busyAction,
    stage,
    primaryAction: primary.action,
    primaryLabel: primary.label,
    primaryDisabled: primary.disabled,
    detail,
  };
}

function isModelInstalled(name: string): boolean {
  const target = normalizeModelTag(name);
  if (!target) return false;
  return installedOllamaModels.some((m) => isExactModelTagMatch(m.name, target));
}

function isModelActive(name: string): boolean {
  return isExactModelTagMatch(name, settings?.ai_fallback?.model);
}

function getInstalledSize(name: string): number {
  const target = normalizeModelTag(name);
  const found = installedOllamaModels.find((m) => isExactModelTagMatch(m.name, target));
  return found ? found.size_bytes : 0;
}

function resolveCardStatus(modelName: string): CardStatus {
  if (!isModelInstalled(modelName)) {
    return "available";
  }
  return isModelActive(modelName) ? "active" : "downloaded";
}

async function persistCurrentSettings(): Promise<void> {
  if (!settings) return;
  await invoke("save_settings", { settings });
}

async function maybePersistWizardState(): Promise<void> {
  if (!settings) return;

  settings.setup ??= {
    local_ai_wizard_completed: false,
    local_ai_wizard_pending: true,
    ollama_remote_expert_opt_in: false,
  };

  const shouldComplete = runtimeIsHealthy() && installedOllamaModels.length > 0;
  const shouldPending = !shouldComplete;

  if (
    settings.setup.local_ai_wizard_completed === shouldComplete &&
    settings.setup.local_ai_wizard_pending === shouldPending
  ) {
    return;
  }

  settings.setup.local_ai_wizard_completed = shouldComplete;
  settings.setup.local_ai_wizard_pending = shouldPending;
  await persistCurrentSettings();
}

async function runRuntimeAction(action: string, task: () => Promise<void>): Promise<void> {
  if (runtimeBusyAction) return;
  runtimeBusyAction = action;
  runtimeInstallError = null;
  renderOllamaModelManager();

  try {
    await task();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    runtimeInstallError = {
      stage: action,
      error: message,
    };
    showToast({
      type: "error",
      title: "Runtime action failed",
      message,
      duration: 6000,
    });
  } finally {
    runtimeBusyAction = null;
    renderOllamaModelManager();
  }
}

async function handleDetectRuntime(): Promise<void> {
  await runRuntimeAction("detect", async () => {
    await refreshOllamaRuntimeState({ force: true });
    if (runtimeDetect?.is_serving) {
      await refreshOllamaInstalledModels();
    }
    showToast({
      type: runtimeDetect?.found ? "success" : "warning",
      title: runtimeDetect?.found ? "Runtime detected" : "No runtime detected",
      message: runtimeDetect?.found
        ? `${runtimeDetect.source} (${runtimeDetect.version || "unknown version"})`
        : "Install local runtime or select system Ollama.",
      duration: 3500,
    });
  });
}

export async function ensureLocalRuntimeReady(): Promise<void> {
  if (runtimeBusyAction) return;

  runtimeBusyAction = "ensure-runtime";
  runtimeInstallError = null;
  renderOllamaModelManager();

  let installedThisRun = false;
  let flowStage: "detect" | "download_runtime" | "install_runtime" | "start_runtime" | "verify_runtime" = "detect";

  try {
    runtimeInstallProgress = {
      stage: "verify_runtime",
      message: "Checking for local runtime...",
    };
    renderOllamaModelManager();

    runtimeDetect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
    if (!runtimeDetect.found) {
      flowStage = "download_runtime";
      runtimeInstallProgress = {
        stage: "download_runtime",
        message: "Downloading runtime archive...",
      };
      renderOllamaModelManager();

      const download = await invoke<OllamaRuntimeDownloadResult>("download_ollama_runtime", {
        version: DEFAULT_RUNTIME_VERSION,
      });
      if (!download.sha256_ok) {
        throw new Error("Runtime checksum verification failed.");
      }

      flowStage = "install_runtime";
      runtimeInstallProgress = {
        stage: "install_runtime",
        message: "Installing runtime files...",
        version: download.version,
      };
      renderOllamaModelManager();

      await invoke<OllamaRuntimeInstallResult>("install_ollama_runtime", {
        archivePath: download.archive_path,
      });
      installedThisRun = true;

      // Refresh detection immediately so UI reflects "installed" even if start fails.
      runtimeDetect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
      if (!runtimeDetect.found) {
        throw new Error("Runtime installation completed, but no executable was detected.");
      }
    }

    flowStage = "start_runtime";
    runtimeInstallProgress = {
      stage: "start_runtime",
      message: "Starting local runtime...",
    };
    renderOllamaModelManager();
    const startResult = await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
    if (startResult.pending_start) {
      startRuntimeBackgroundPolling("manual");
      await refreshOllamaRuntimeState({ force: true });
      showToast({
        type: "warning",
        title: "Runtime starting in background",
        message: "Startup is taking longer than usual. Verification continues in background.",
        duration: 4200,
      });
      return;
    }

    flowStage = "verify_runtime";
    runtimeInstallProgress = {
      stage: "verify_runtime",
      message: "Verifying local runtime...",
    };
    renderOllamaModelManager();
    runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");

    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
    await maybePersistWizardState();

    showToast({
      type: "success",
      title: installedThisRun ? "Local runtime installed" : "Local runtime ready",
      message: installedThisRun
        ? "Install and startup completed. Local Ollama is ready."
        : "Local Ollama runtime is running and verified.",
      duration: 4000,
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    runtimeInstallError = {
      stage: flowStage,
      error: message,
    };

    if (installedThisRun && (flowStage === "start_runtime" || flowStage === "verify_runtime")) {
      showToast({
        type: "warning",
        title: "Install completed, start failed",
        message: `${message} Try "Start local runtime" again.`,
        duration: 7000,
      });
    } else {
      showToast({
        type: "error",
        title: "Runtime setup failed",
        message,
        duration: 7000,
      });
    }
  } finally {
    runtimeBusyAction = null;
    runtimeInstallProgress = null;
    renderOllamaModelManager();
  }
}

async function handleUseSystemRuntime(): Promise<void> {
  await runRuntimeAction("use-system", async () => {
    if (!settings) {
      throw new Error("Settings are not loaded yet.");
    }

    settings.providers.ollama.runtime_source = "system";
    settings.providers.ollama.runtime_path = "";
    await persistCurrentSettings();

    const detect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
    if (!detect.found || detect.source !== "system") {
      throw new Error("No system Ollama found in PATH.");
    }

    const startResult = await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
    if (startResult.pending_start) {
      startRuntimeBackgroundPolling("manual");
      await refreshOllamaRuntimeState({ force: true });
      showToast({
        type: "warning",
        title: "Using system Ollama",
        message: "System runtime detected and starting in background.",
        duration: 4200,
      });
    } else {
      runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");
      await refreshOllamaInstalledModels();
      await refreshOllamaRuntimeState();
      await maybePersistWizardState();

      showToast({
        type: "success",
        title: "Using system Ollama",
        message: `Detected ${detect.version || "installed version"} from PATH.`,
        duration: 3500,
      });
    }
  });
}

export async function useSystemOllamaRuntime(): Promise<void> {
  await handleUseSystemRuntime();
}

async function handleStartRuntime(): Promise<void> {
  await runRuntimeAction("start", async () => {
    const result = await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
    if (result.pending_start) {
      startRuntimeBackgroundPolling("manual");
      await refreshOllamaRuntimeState({ force: true });
      showToast({
        type: "warning",
        title: "Runtime starting in background",
        message: `${result.endpoint} is still warming up. Verification continues in background.`,
        duration: 4200,
      });
      return;
    }

    runtimeHealth = {
      ok: true,
      endpoint: result.endpoint,
      models_count: installedOllamaModels.length,
    };
    runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");
    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
    await maybePersistWizardState();

    showToast({
      type: "success",
      title: result.already_running ? "Runtime already running" : "Runtime started",
      message: `${result.endpoint} is reachable.`,
      duration: 3000,
    });
  });
}

export async function startOllamaRuntime(): Promise<void> {
  await handleStartRuntime();
}

async function handleVerifyRuntime(): Promise<void> {
  await runRuntimeAction("verify", async () => {
    runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");
    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
    await maybePersistWizardState();

    showToast({
      type: "success",
      title: "Runtime verified",
      message: `${runtimeVerify.models_count} model(s) available.`,
      duration: 3000,
    });
  });
}

export async function verifyOllamaRuntime(): Promise<void> {
  await handleVerifyRuntime();
}

async function handleImportModelFromFile(): Promise<void> {
  await runRuntimeAction("import", async () => {
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [{ name: "Model files", extensions: ["gguf", "modelfile", "txt"] }],
    });

    if (!selected || Array.isArray(selected)) {
      return;
    }

    const sourcePath = selected;
    const lower = sourcePath.toLowerCase();
    const mode = lower.endsWith(".gguf") ? "gguf" : "modelfile";

    const imported = await invoke<OllamaImportResult>("import_ollama_model_from_file", {
      path: sourcePath,
      mode,
    });

    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
    await maybePersistWizardState();

    showToast({
      type: "success",
      title: "Model imported",
      message: `${imported.model_name} is now available for local refinement.`,
      duration: 4000,
    });
  });
}

export async function importOllamaModelFromLocalFile(): Promise<void> {
  await handleImportModelFromFile();
}

async function handleSetActiveModel(modelName: string): Promise<void> {
  if (!settings) return;
  const normalized = normalizeModelTag(modelName);
  settings.ai_fallback.model = normalized;
  settings.postproc_llm_model = normalized;
  settings.providers.ollama.preferred_model = normalized;
  await persistCurrentSettings();
  showToast({
    type: "success",
    title: "Model selected",
    message: `${modelName} is now active for AI refinement.`,
    duration: 3000,
  });
  renderOllamaModelManager();
}

async function handleRefreshRuntimeAndModels(): Promise<void> {
  await runRuntimeAction("refresh", async () => {
    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
  });
}

export async function refreshOllamaRuntimeAndModels(): Promise<void> {
  await handleRefreshRuntimeAndModels();
}

export async function detectOllamaRuntime(): Promise<void> {
  await handleDetectRuntime();
}

function renderModelsSection(container: HTMLElement): void {
  const section = document.createElement("div");
  section.className = "ollama-models-section";

  const hint = document.createElement("p");
  hint.className = "field-hint";
  hint.textContent = "Download, activate, or remove local models for offline refinement.";
  section.appendChild(hint);

  const list = document.createElement("div");
  list.className = "model-list ollama-model-list";

  OLLAMA_RECOMMENDED_MODELS.forEach((spec) => {
    const installed = isModelInstalled(spec.name);
    const active = isModelActive(spec.name);
    const isPulling = ollamaPullProgress.has(spec.name) || activeOllamaPulls.has(spec.name);
    const progress = ollamaPullProgress.get(spec.name);

    const status = resolveCardStatus(spec.name);
    const statusText = isPulling
      ? "Downloading"
      : status === "active"
        ? "Active"
        : status === "downloaded"
          ? "Installed"
          : "Available";

    const card = document.createElement("article");
    card.className = `model-item ollama-model-item${
      status === "active" ? " selected" : ""
    }${status === "available" ? " model-item--available" : ""}${isPulling ? " is-loading" : ""}`;

    const sizeDisplay = installed
      ? formatBytesGb(getInstalledSize(spec.name))
      : `~${spec.size_gb.toFixed(1)} GB`;

    card.innerHTML = `
      <div class="model-header">
        <div class="model-name">${spec.label}</div>
        <div class="model-size">${sizeDisplay}</div>
      </div>
      <div class="model-meta">${spec.profile}</div>
      <div class="model-desc">${spec.description}</div>
      <div class="model-status ${isPulling ? "downloaded" : status}">${statusText}</div>
      ${
        isPulling && progress
          ? `
        <div class="model-progress ollama-model-progress">
          <div class="ollama-progress-bar">
            <div class="ollama-progress-fill" style="width: ${computeOllamaPercent(progress)}%"></div>
          </div>
          <span class="ollama-progress-text">${formatOllamaProgress(progress)}</span>
        </div>
      `
          : ""
      }
      <div class="model-actions ollama-model-actions"></div>
    `;

    const actionsEl = card.querySelector(".ollama-model-actions") as HTMLDivElement | null;
    if (actionsEl) {
      if (isPulling) {
        const note = document.createElement("span");
        note.className = "ollama-cancel-note";
        note.textContent = "Pull in progress...";
        actionsEl.appendChild(note);
      } else if (status === "available") {
        const pullBtn = document.createElement("button");
        pullBtn.className = "btn-sm btn-primary";
        pullBtn.textContent = "Download";
        pullBtn.title = `Pull ${spec.name} via Ollama`;
        applyHelpTooltip(pullBtn, "ollama_action_download");
        pullBtn.addEventListener("click", () => {
          void handleOllamaPull(spec.name);
        });
        actionsEl.appendChild(pullBtn);
      } else {
        if (!active) {
          const activateBtn = document.createElement("button");
          activateBtn.className = "btn-sm btn-primary";
          activateBtn.textContent = "Set active";
          activateBtn.title = `Use ${spec.name} for AI refinement`;
          applyHelpTooltip(activateBtn, "ollama_action_set_active");
          activateBtn.addEventListener("click", () => {
            void handleSetActiveModel(spec.name);
          });
          actionsEl.appendChild(activateBtn);
        }

        const deleteBtn = document.createElement("button");
        deleteBtn.className = "btn-sm btn-danger";
        deleteBtn.textContent = "Delete";
        deleteBtn.title = `Remove ${spec.name} from Ollama`;
        applyHelpTooltip(deleteBtn, "ollama_action_delete");
        deleteBtn.addEventListener("click", () => {
          void handleOllamaDelete(spec.name);
        });
        actionsEl.appendChild(deleteBtn);
      }
    }

    list.appendChild(card);
  });

  section.appendChild(list);
  container.appendChild(section);
}

function renderOllamaModelManagerNow(): void {
  const container = document.getElementById("ollama-model-manager");
  if (!container) return;

  const isOllama = isOllamaProvider();
  container.style.display = isOllama ? "" : "none";
  if (!isOllama) return;

  container.innerHTML = "";
  renderModelsSection(container);
}

export function renderOllamaModelManager(): void {
  scheduleRender();
}

export async function refreshOllamaInstalledModels(): Promise<void> {
  try {
    const result = await invoke<Array<{ name: string; size_bytes: number }>>(
      "fetch_ollama_models_with_size"
    );
    installedOllamaModels = result;
  } catch (error) {
    console.error("Failed to refresh Ollama models:", error);
    installedOllamaModels = [];
  }

  if (runtimeHealth) {
    runtimeHealth = {
      ...runtimeHealth,
      models_count: installedOllamaModels.length,
    };
  }
  if (runtimeVerify) {
    runtimeVerify = {
      ...runtimeVerify,
      models_count: installedOllamaModels.length,
    };
  }
}

export async function refreshOllamaRuntimeState(
  options: RuntimeStateRefreshOptions = {}
): Promise<void> {
  const verify = options.verify ?? false;
  const skipDetect = options.skipDetect ?? false;
  const force = options.force ?? false;
  const now = Date.now();

  if (!verify && !force && now - runtimeStateLastRefreshMs < PASSIVE_RUNTIME_REFRESH_TTL_MS) {
    renderOllamaModelManager();
    return;
  }

  if (runtimeStateRefreshInFlight) {
    await runtimeStateRefreshInFlight;
    if (!verify) return;
  }

  const refreshTask = (async () => {
    if (!skipDetect) {
      try {
        runtimeDetect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
      } catch {
        runtimeDetect = null;
      }
    }

    const endpoint = settings?.providers?.ollama?.endpoint || DEFAULT_LOCAL_ENDPOINT;

    if (verify) {
      try {
        runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");
        runtimeHealth = {
          ok: runtimeVerify.ok,
          endpoint: runtimeVerify.endpoint,
          models_count: runtimeVerify.models_count,
        };
      } catch {
        runtimeVerify = null;
        runtimeHealth = {
          ok: false,
          endpoint,
          models_count: 0,
        };
      }
    } else {
      const serving = Boolean(runtimeDetect?.is_serving);
      if (!serving) {
        runtimeVerify = null;
      }
      runtimeHealth = {
        ok: serving,
        endpoint,
        models_count: installedOllamaModels.length,
      };
    }

    try {
      await maybePersistWizardState();
    } catch (error) {
      console.warn("Failed to persist local AI wizard state:", error);
    }

    if (runtimeHealth?.ok) {
      runtimeBackgroundStartUntilMs = 0;
    }

    runtimeStateLastRefreshMs = Date.now();
    renderOllamaModelManager();
  })();

  runtimeStateRefreshInFlight = refreshTask;
  try {
    await refreshTask;
  } finally {
    if (runtimeStateRefreshInFlight === refreshTask) {
      runtimeStateRefreshInFlight = null;
    }
  }
}

async function handleOllamaPull(modelName: string): Promise<void> {
  activeOllamaPulls.add(modelName);
  renderOllamaModelManager();

  try {
    await invoke("pull_ollama_model", { model: modelName });
  } catch (error) {
    activeOllamaPulls.delete(modelName);
    showToast({
      type: "error",
      title: "Pull Failed",
      message: String(error),
    });
    renderOllamaModelManager();
  }
}

async function handleOllamaDelete(modelName: string): Promise<void> {
  try {
    await invoke("delete_ollama_model", { model: modelName });
    await refreshOllamaInstalledModels();

    if (settings && normalizeModelTag(settings.ai_fallback.model) === normalizeModelTag(modelName)) {
      const fallbackModel = installedOllamaModels[0]?.name ?? "";
      settings.ai_fallback.model = normalizeModelTag(fallbackModel);
      settings.providers.ollama.preferred_model = normalizeModelTag(fallbackModel);
      settings.postproc_llm_model = normalizeModelTag(fallbackModel);
      await persistCurrentSettings();
    }

    await maybePersistWizardState();
    showToast({
      type: "success",
      title: "Model Deleted",
      message: `${modelName} has been removed from Ollama.`,
    });
    renderOllamaModelManager();
  } catch (error) {
    showToast({
      type: "error",
      title: "Delete Failed",
      message: String(error),
    });
  }
}

function computeOllamaPercent(progress: OllamaPullProgressType): number {
  if (progress.total && progress.total > 0 && progress.completed) {
    return Math.min(100, Math.round((progress.completed / progress.total) * 100));
  }
  return 0;
}

function formatOllamaProgress(progress: OllamaPullProgressType): string {
  if (progress.total && progress.completed) {
    const pct = computeOllamaPercent(progress);
    const mbDone = Math.round(progress.completed / (1024 * 1024));
    const mbTotal = Math.round(progress.total / (1024 * 1024));
    return `${pct}% (${mbDone} / ${mbTotal} MB) - ${progress.status}`;
  }
  return progress.status;
}

function formatBytesGb(bytes: number): string {
  if (bytes === 0) return "Unknown size";
  const gb = bytes / (1024 * 1024 * 1024);
  return `${gb.toFixed(1)} GB`;
}

export function clearActiveOllamaPull(modelName: string): void {
  activeOllamaPulls.delete(modelName);
}

export function setOllamaRuntimeInstallProgress(progress: OllamaRuntimeInstallProgress): void {
  runtimeInstallProgress = progress;
  renderOllamaModelManager();
}

export function setOllamaRuntimeInstallComplete(_payload: OllamaRuntimeInstallComplete): void {
  runtimeInstallError = null;
  runtimeInstallProgress = null;
  renderOllamaModelManager();
}

export function setOllamaRuntimeInstallError(error: OllamaRuntimeInstallError): void {
  runtimeInstallError = error;
  runtimeInstallProgress = null;
  renderOllamaModelManager();
}

export function setOllamaRuntimeHealth(health: OllamaRuntimeHealth): void {
  runtimeHealth = health;
  renderOllamaModelManager();
}
