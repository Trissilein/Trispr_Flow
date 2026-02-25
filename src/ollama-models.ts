import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { settings, ollamaPullProgress } from "./state";
import { showToast } from "./toast";
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
const DEFAULT_SETUP_MODEL = "qwen3:8b";
const DEFAULT_RUNTIME_VERSION = "0.17.0";

type WizardStage =
  | "not_detected"
  | "download_runtime"
  | "install_runtime"
  | "start_runtime"
  | "verify_runtime"
  | "select_model_source"
  | "ready";

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

function isOllamaProvider(): boolean {
  return settings?.ai_fallback?.provider === "ollama";
}

function strictLocalModeEnabled(): boolean {
  return Boolean(settings?.ai_fallback?.strict_local_mode ?? true);
}

function runtimeIsHealthy(): boolean {
  return Boolean(runtimeVerify?.ok || runtimeHealth?.ok);
}

function computeWizardStage(): WizardStage {
  const progressStage = runtimeInstallProgress?.stage;
  if (progressStage === "download_runtime") return "download_runtime";
  if (progressStage === "install_runtime") return "install_runtime";

  if (!runtimeDetect?.found) return "not_detected";
  if (!runtimeIsHealthy()) return "start_runtime";
  if (installedOllamaModels.length === 0) return "select_model_source";
  return "ready";
}

function stageTitle(stage: WizardStage): string {
  if (stage === "not_detected") return "Runtime not detected";
  if (stage === "download_runtime") return "Downloading runtime";
  if (stage === "install_runtime") return "Installing runtime";
  if (stage === "start_runtime") return "Start local runtime";
  if (stage === "verify_runtime") return "Verifying runtime";
  if (stage === "select_model_source") return "Runtime ready, no model installed";
  return "Ready";
}

function stageHint(stage: WizardStage): string {
  if (stage === "not_detected") {
    return "Install local Ollama runtime, use existing system Ollama, or import manually.";
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
  if (stage === "select_model_source") {
    return "Install a model with one click or import from local files for offline setup.";
  }
  return "Local AI refinement is configured and ready.";
}

function stageClass(stage: WizardStage): string {
  if (stage === "ready") return "is-ready";
  if (stage === "download_runtime" || stage === "install_runtime") return "is-busy";
  if (stage === "select_model_source") return "is-warning";
  return "is-pending";
}

function isModelInstalled(name: string): boolean {
  return installedOllamaModels.some(
    (m) => m.name === name || m.name.startsWith(`${name.split(":")[0]}:`)
  );
}

function getInstalledSize(name: string): number {
  const found = installedOllamaModels.find(
    (m) => m.name === name || m.name.startsWith(`${name.split(":")[0]}:`)
  );
  return found ? found.size_bytes : 0;
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

function renderRuntimeProgress(section: HTMLElement): void {
  if (!runtimeInstallProgress) return;

  const row = document.createElement("div");
  row.className = "ollama-runtime-progress";

  const hasTotals =
    typeof runtimeInstallProgress.downloaded === "number" &&
    typeof runtimeInstallProgress.total === "number" &&
    runtimeInstallProgress.total > 0;

  const percent = hasTotals
    ? Math.max(
        0,
        Math.min(
          100,
          Math.round(
            ((runtimeInstallProgress.downloaded as number) /
              (runtimeInstallProgress.total as number)) *
              100
          )
        )
      )
    : null;

  if (percent !== null) {
    const bar = document.createElement("div");
    bar.className = "ollama-progress-bar";
    const fill = document.createElement("div");
    fill.className = "ollama-progress-fill";
    fill.style.width = `${percent}%`;
    bar.appendChild(fill);
    row.appendChild(bar);
  }

  const text = document.createElement("span");
  text.className = "ollama-progress-text";
  text.textContent = runtimeInstallProgress.message;
  row.appendChild(text);

  section.appendChild(row);
}

function renderRuntimeMeta(section: HTMLElement): void {
  const meta = document.createElement("div");
  meta.className = "ollama-runtime-meta";

  const endpoint = settings?.providers?.ollama?.endpoint || DEFAULT_LOCAL_ENDPOINT;
  const strictLocal = strictLocalModeEnabled() ? "on" : "off";
  const source = runtimeDetect?.source || settings?.providers?.ollama?.runtime_source || "manual";
  const version = runtimeDetect?.version || settings?.providers?.ollama?.runtime_version || "unknown";

  const endpointRow = document.createElement("span");
  endpointRow.textContent = `Endpoint: ${endpoint}`;
  meta.appendChild(endpointRow);

  const strictRow = document.createElement("span");
  strictRow.textContent = `Strict local: ${strictLocal}`;
  meta.appendChild(strictRow);

  const sourceRow = document.createElement("span");
  sourceRow.textContent = `Runtime source: ${source}`;
  meta.appendChild(sourceRow);

  const versionRow = document.createElement("span");
  versionRow.textContent = `Runtime version: ${version}`;
  meta.appendChild(versionRow);

  if (runtimeInstallError) {
    const errorRow = document.createElement("span");
    errorRow.className = "ollama-runtime-error";
    errorRow.textContent = `Last error: ${runtimeInstallError.error}`;
    meta.appendChild(errorRow);
  }

  section.appendChild(meta);
}

function runtimeButton(label: string, onClick: () => Promise<void> | void, options?: {
  recommended?: boolean;
  disabled?: boolean;
}): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = options?.recommended ? "hotkey-record-btn is-recommended" : "hotkey-record-btn";
  button.textContent = label;
  button.disabled = Boolean(options?.disabled);
  button.addEventListener("click", () => {
    void onClick();
  });
  return button;
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
    runtimeDetect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
    await refreshOllamaRuntimeState();
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

async function handleInstallLocalRuntime(): Promise<void> {
  await runRuntimeAction("install", async () => {
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

    runtimeInstallProgress = {
      stage: "install_runtime",
      message: "Installing runtime files...",
      version: download.version,
    };
    renderOllamaModelManager();

    await invoke<OllamaRuntimeInstallResult>("install_ollama_runtime", {
      archivePath: download.archive_path,
    });

    await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
    runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");

    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState();
    await maybePersistWizardState();

    showToast({
      type: "success",
      title: "Local runtime installed",
      message: "Ollama runtime is installed and running locally.",
      duration: 4000,
    });
  });
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

    await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
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
  });
}

async function handleStartRuntime(): Promise<void> {
  await runRuntimeAction("start", async () => {
    const result = await invoke<OllamaRuntimeStartResult>("start_ollama_runtime");
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

async function handleImportModelFromFile(): Promise<void> {
  await runRuntimeAction("import", async () => {
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [
        { name: "Model files", extensions: ["gguf", "modelfile", "txt"] },
      ],
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

async function handlePullDefaultModel(): Promise<void> {
  await handleOllamaPull(DEFAULT_SETUP_MODEL);
}

function renderRuntimeWizard(container: HTMLElement): void {
  const section = document.createElement("div");
  section.className = "ai-refine-section";

  const titleRow = document.createElement("div");
  titleRow.className = "ollama-runtime-title-row";

  const title = document.createElement("h3");
  title.className = "ai-refine-section-title";
  title.textContent = "Local AI Runtime";
  titleRow.appendChild(title);

  const stage = computeWizardStage();
  const badge = document.createElement("span");
  badge.className = `ollama-runtime-badge ${stageClass(stage)}`;
  badge.textContent = stageTitle(stage);
  titleRow.appendChild(badge);

  section.appendChild(titleRow);

  const hint = document.createElement("p");
  hint.className = "field-hint";
  hint.textContent = stageHint(stage);
  section.appendChild(hint);

  if (runtimeBusyAction) {
    const busy = document.createElement("p");
    busy.className = "ollama-runtime-busy";
    busy.textContent = `Action in progress: ${runtimeBusyAction}`;
    section.appendChild(busy);
  }

  renderRuntimeProgress(section);
  renderRuntimeMeta(section);

  const actions = document.createElement("div");
  actions.className = "ollama-runtime-actions";
  const busy = Boolean(runtimeBusyAction);

  actions.appendChild(
    runtimeButton("Install local Ollama runtime (recommended)", handleInstallLocalRuntime, {
      recommended: true,
      disabled: busy,
    })
  );
  actions.appendChild(
    runtimeButton("Use existing system Ollama", handleUseSystemRuntime, {
      disabled: busy,
    })
  );
  actions.appendChild(runtimeButton("Detect runtime", handleDetectRuntime, { disabled: busy }));
  actions.appendChild(runtimeButton("Start runtime", handleStartRuntime, { disabled: busy }));
  actions.appendChild(runtimeButton("Verify runtime", handleVerifyRuntime, { disabled: busy }));
  actions.appendChild(
    runtimeButton("Import model from file", handleImportModelFromFile, { disabled: busy })
  );

  if (!isModelInstalled(DEFAULT_SETUP_MODEL)) {
    actions.appendChild(
      runtimeButton(`Pull ${DEFAULT_SETUP_MODEL}`, handlePullDefaultModel, { disabled: busy })
    );
  }

  section.appendChild(actions);
  container.appendChild(section);
}

function renderRecommendedModels(container: HTMLElement): void {
  const header = document.createElement("div");
  header.className = "ai-refine-section";
  header.innerHTML = `
    <h3 class="ai-refine-section-title">Recommended Models</h3>
    <p class="field-hint">These models are optimized for transcript refinement.</p>
  `;
  container.appendChild(header);

  OLLAMA_RECOMMENDED_MODELS.forEach((spec) => {
    const installed = isModelInstalled(spec.name);
    const isPulling = ollamaPullProgress.has(spec.name) || activeOllamaPulls.has(spec.name);
    const progress = ollamaPullProgress.get(spec.name);

    const card = document.createElement("div");
    card.className = `ollama-model-card${installed ? " is-installed" : ""}`;

    const sizeDisplay = installed
      ? formatBytesGb(getInstalledSize(spec.name))
      : `~${spec.size_gb.toFixed(1)} GB`;

    card.innerHTML = `
      <div class="ollama-model-header">
        <div class="ollama-model-name">${spec.label}</div>
        <div class="ollama-model-size">${sizeDisplay}</div>
      </div>
      <div class="ollama-model-profile">${spec.profile}</div>
      <div class="ollama-model-desc">${spec.description}</div>
      <div class="ollama-model-status ${installed ? "installed" : "available"}">
        ${installed ? "Installed" : isPulling ? "Downloading..." : "Not installed"}
      </div>
      ${
        isPulling && progress
          ? `
        <div class="ollama-model-progress">
          <div class="ollama-progress-bar">
            <div class="ollama-progress-fill" style="width: ${computeOllamaPercent(progress)}%"></div>
          </div>
          <span class="ollama-progress-text">${formatOllamaProgress(progress)}</span>
        </div>
      `
          : ""
      }
      <div class="ollama-model-actions"></div>
    `;

    container.appendChild(card);

    const actionsEl = card.querySelector(".ollama-model-actions");
    if (!actionsEl) return;

    if (installed) {
      const deleteBtn = document.createElement("button");
      deleteBtn.className = "btn-sm btn-danger";
      deleteBtn.textContent = "Delete";
      deleteBtn.title = `Remove ${spec.name} from Ollama`;
      deleteBtn.addEventListener("click", () => {
        void handleOllamaDelete(spec.name);
      });
      actionsEl.appendChild(deleteBtn);
    } else if (isPulling) {
      const note = document.createElement("span");
      note.className = "ollama-cancel-note";
      note.textContent = "Pull in progress...";
      actionsEl.appendChild(note);
    } else {
      const pullBtn = document.createElement("button");
      pullBtn.className = "btn-sm btn-primary";
      pullBtn.textContent = "Download";
      pullBtn.title = `Pull ${spec.name} via Ollama`;
      pullBtn.addEventListener("click", () => {
        void handleOllamaPull(spec.name);
      });
      actionsEl.appendChild(pullBtn);
    }
  });
}

function renderRefreshRow(container: HTMLElement): void {
  const refreshRow = document.createElement("div");
  refreshRow.className = "ollama-refresh-row";

  const refreshBtn = document.createElement("button");
  refreshBtn.className = "ghost-btn";
  refreshBtn.textContent = "Refresh Runtime + Models";
  refreshBtn.addEventListener("click", () => {
    void (async () => {
      await refreshOllamaInstalledModels();
      await refreshOllamaRuntimeState();
      renderOllamaModelManager();
    })();
  });

  refreshRow.appendChild(refreshBtn);
  container.appendChild(refreshRow);
}

export function renderOllamaModelManager(): void {
  const container = document.getElementById("ollama-model-manager");
  if (!container) return;

  const isOllama = isOllamaProvider();
  container.style.display = isOllama ? "" : "none";
  if (!isOllama) return;

  container.innerHTML = "";
  renderRuntimeWizard(container);
  renderRecommendedModels(container);
  renderRefreshRow(container);
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
}

export async function refreshOllamaRuntimeState(): Promise<void> {
  if (!isOllamaProvider()) return;

  try {
    runtimeDetect = await invoke<OllamaRuntimeDetectResult>("detect_ollama_runtime");
  } catch {
    runtimeDetect = null;
  }

  try {
    runtimeVerify = await invoke<OllamaRuntimeVerifyResult>("verify_ollama_runtime");
    runtimeHealth = {
      ok: runtimeVerify.ok,
      endpoint: runtimeVerify.endpoint,
      models_count: runtimeVerify.models_count,
    };
  } catch {
    runtimeVerify = null;
  }

  try {
    await maybePersistWizardState();
  } catch (error) {
    console.warn("Failed to persist local AI wizard state:", error);
  }

  renderOllamaModelManager();
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
  if (progress.stage === "verify_runtime") {
    runtimeVerify = { ok: true, endpoint: DEFAULT_LOCAL_ENDPOINT, models_count: installedOllamaModels.length };
  }
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
