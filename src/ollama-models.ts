import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  traceFrontendError,
  traceFrontendInfo,
  traceFrontendWarn,
} from "./frontend-trace";
import { settings, ollamaPullProgress, runtimeDiagnostics, startupStatus } from "./state";
import { showToast } from "./toast";
import { isExactModelTagMatch, normalizeModelTag } from "./ollama-tag-utils";
import {
  MODEL_CATALOG,
  catalogSourceLabel,
  isHuggingFaceModelRef,
  type ModelCatalogEntry,
  type ModelFunctionalProof,
} from "./ollama-model-catalog";
import { confirmDialog, type ConfirmDialogOptions } from "./ollama-confirm-dialog";
import {
  captureModelListFocus,
  renderModelList,
  restoreModelListFocus,
  type ModelListAction,
  type ModelListChip,
  type ModelListHint,
  type ModelListHooks,
  type ModelListInfo,
  type ModelListLink,
  type ModelListRow,
} from "./ollama-model-list";
import { PullProgressTracker } from "./ollama-pull-progress";
import {
  DEFAULT_RUNTIME_VERSION,
  compareVersions,
  formatDownloadSize,
  formatReleaseDate,
  parseVersionTriple,
  pickRuntimeVersionForRequirement,
  syncRuntimeTarget,
  type RuntimeVersionActionId,
} from "./ollama-runtime-versions";
// applyHelpTooltip removed — simplified model dropdown doesn't use tooltips
import {
  normalizePersistedRefinementPromptPresetId,
  normalizeUserRefinementPromptPresets,
} from "./refinement-prompts";
import type {
  OllamaImportResult,
  OllamaModelFinalizeResult,
  OllamaModelReplaceResult,
  OllamaModelRequirement,
  OllamaModelRollbackResult,
  OllamaPullComplete,
  OllamaPullError,
  OllamaPullProgress as OllamaPullProgressType,
  OllamaRuntimeDeleteResult,
  OllamaRuntimeDetectResult,
  OllamaRuntimeDownloadResult,
  OllamaRuntimeHealth,
  OllamaRuntimeInstallComplete,
  OllamaRuntimeInstallError,
  OllamaRuntimeInstallProgress,
  OllamaRuntimeInstallResult,
  OllamaRuntimeVersionInfo,
  OllamaRuntimeStartResult,
  OllamaRuntimeVerifyResult,
} from "./types";

const DEFAULT_LOCAL_ENDPOINT = "http://127.0.0.1:11434";
const AUTOSTART_WARNING_COOLDOWN_MS = 60_000;
const BACKGROUND_START_POLL_INTERVAL_MS = 2_000;
const BACKGROUND_START_POLL_MAX_MS = 30_000;
const RUNTIME_BUSY_START_STALE_MS = 45_000;
const RUNTIME_DETECT_TIMEOUT_MS = 2_500;
const RUNTIME_VERIFY_TIMEOUT_MS = 4_000;
const RUNTIME_MODEL_INFO_TIMEOUT_MS = 2_500;
/** Removing a few GB with retries for locked files. */
const RUNTIME_DELETE_TIMEOUT_MS = 120_000;

// Nothing may wait for a backend that never answers: every call a UI lock depends on has a limit.
/** The backend streams roughly four progress lines a second; its own read timeout is one hour. */
const PULL_STALL_MS = 10 * 60_000;
/** `pull_ollama_model` only spawns the pull, after a registry precheck. */
const PULL_START_TIMEOUT_MS = 2 * 60_000;
/** `/api/copy` shares the blobs, so this is fast. */
const BACKUP_TIMEOUT_MS = 2 * 60_000;
/** Functional check (the backend gives up after 90 s) plus unload, delete and copy. */
const FINALIZE_TIMEOUT_MS = 5 * 60_000;
const ONLINE_CATALOG_TIMEOUT_MS = 90_000;
const SAVE_SETTINGS_TIMEOUT_MS = 3_000;
/** Start waits for the server to come up (the backend returns `pending_start` itself after a while). */
const RUNTIME_START_TIMEOUT_MS = 2 * 60_000;
/** Detect, verify, model list: plain local HTTP checks. */
const RUNTIME_ACTION_TIMEOUT_MS = 30_000;
const DELETE_TIMEOUT_MS = 2 * 60_000;
/** A failed event registration is retried by renders at most this often (user actions retry at once). */
const PULL_TRACKING_RETRY_MS = 30_000;

type WizardStage =
  | "not_detected"
  | "download_runtime"
  | "install_runtime"
  | "start_runtime"
  | "verify_runtime"
  | "failed"
  | "select_model_source"
  | "ready";

type OllamaRecommendedModelSpec = ModelCatalogEntry;

type CuratedModelState = "active" | "installed" | "downloading" | "available" | "missing";

/** One row of `fetch_ollama_models_with_size`; the metadata fields are empty on older backends. */
export type InstalledOllamaModel = {
  name: string;
  size_bytes: number;
  digest?: string;
  modified_at?: string;
  family?: string;
  parameter_size?: string;
  quantization_level?: string;
};

export type OllamaModelUpdateStatus = "current" | "outdated" | "local" | "unknown";

/** One row of `check_ollama_model_updates`. */
export type OllamaModelUpdateInfo = {
  name: string;
  status: OllamaModelUpdateStatus;
  remote_digest?: string | null;
  remote_size_bytes?: number | null;
  detail?: string | null;
};

export type ModelUpdateChip = {
  status: "current" | "outdated" | "local";
  label: "Current" | "Outdated" | "Local";
  title: string;
};

export type InstalledOllamaInventoryItem = {
  name: string;
  size_bytes: number;
  size_label: string;
  is_active: boolean;
  is_curated: boolean;
  curated_spec?: OllamaRecommendedModelSpec;
  can_activate: boolean;
  can_uninstall: boolean;
  digest_short: string;
  modified_at: string;
  /** "c6eb396dbd59 · pulled 23 Jul 2026 · 8.0B · Q4_K_M" (unknown parts left out). */
  meta_label: string;
  update?: OllamaModelUpdateInfo;
  update_chip: ModelUpdateChip | null;
  update_hint: string | null;
  /** Outdated and nothing installed carries the same registry build: re-pull it ("Update"). */
  can_update: boolean;
  /** Outdated, but another installed model already is the current build: name to switch to. */
  replace_with: string | null;
  /** "Verified working" proof from the curated catalog (installed curated models only). */
  functional_chip: ModelFunctionalChip | null;
};

export type ModelFunctionalChip = { label: "Verified working"; title: string };

/** What a catalog card or an outdated model needs from the runtime. */
export type ModelRuntimeGate = {
  /** Minimum Ollama version; null when only a refused pull told us it is too old. */
  requires: string | null;
  running: string | null;
  compatible: boolean | null;
  /** True when the runtime is too old: pulling is off and the runtime has to change first. */
  blocked: boolean;
  chipLabel: string;
  chipTitle: string;
  /** Verified catalog runtime that fixes it (only when blocked). */
  target: OllamaRuntimeVersionInfo | null;
  /** Button label for `target`: "Update runtime to X" or, when already on disk, "Switch runtime to X". */
  targetLabel: string | null;
  /** Shown instead of the button when blocked and the catalog has no suitable runtime. */
  hint: string | null;
};

/**
 * Curated pull list (Ollama library + Hugging Face), fed from `model-catalog.json`.
 * Sizes are checked against the registries by `scripts/verify-model-catalog.mjs`.
 */
export const OLLAMA_RECOMMENDED_MODELS: OllamaRecommendedModelSpec[] = MODEL_CATALOG;

let installedOllamaModels: InstalledOllamaModel[] = [];
/** The last `fetch_ollama_models_with_size` failed: the list above is what was last known, not the truth. */
let installedModelsUnavailable = false;
/** The list has been read successfully at least once; until then "no models" only means "not read yet". */
let installedModelsRead = false;
let installedRefreshSeq = 0;
let installedAppliedSeq = 0;

// Registry update check per installed model: cached in memory, never blocks the inventory.
const MODEL_UPDATE_TTL_MS = 6 * 60 * 60_000;
const MODEL_UPDATE_FAILURE_TTL_MS = 5 * 60_000;
const MODEL_UPDATE_TIMEOUT_MS = 20_000;
/** The backend answers at most this many models per call. */
const MODEL_UPDATE_MAX_PER_CALL = 24;
/** Shorter local digests cannot be compared reliably against the registry manifest digest. */
const MIN_COMPARABLE_DIGEST_LENGTH = 12;
type ModelUpdateCacheEntry = { digest: string; checkedAtMs: number; info: OllamaModelUpdateInfo | null };
const modelUpdateCache = new Map<string, ModelUpdateCacheEntry>();
let modelUpdateInFlight: Promise<void> | null = null;
/** Pulls this view started and that have not finished yet (normalized names). */
const activeOllamaPulls = new Set<string>();
const activeOllamaDeletes = new Set<string>();

type ModelOperationPhase = "check" | "backup" | "download" | "verify" | "replace" | "rollback";
/** The one running safe update, replacement, re-check or roll back; their buttons are off meanwhile. */
type ModelOperation = {
  kind: "update" | "replace" | "recheck" | "rollback";
  model: string;
  phase: ModelOperationPhase;
  /** Replace only: the installed build that takes over. */
  target?: string;
};
let modelOperation: ModelOperation | null = null;
/** True while a confirmation for an update/replace/roll back is open (a second click must not open another). */
let confirmationOpen = false;

/**
 * An update whose functional check could not finish ("inconclusive"): the new build and the backup
 * both exist and the user decides. Kept in memory only, so it is lost when the app restarts.
 */
type PendingModelCheck = { backup: string; detail: string };
const pendingModelChecks = new Map<string, PendingModelCheck>();

let runtimeDetect: OllamaRuntimeDetectResult | null = null;
let runtimeVerify: OllamaRuntimeVerifyResult | null = null;
let runtimeInstallProgress: OllamaRuntimeInstallProgress | null = null;

export function getRuntimeInstallProgress(): OllamaRuntimeInstallProgress | null {
  return runtimeInstallProgress;
}
let runtimeInstallError: OllamaRuntimeInstallError | null = null;
let runtimeHealth: OllamaRuntimeHealth | null = null;
let runtimeVersionCatalog: OllamaRuntimeVersionInfo[] = [];
let activeModelRequiredRuntime: string | null = null;
let runtimeBusyAction: string | null = null;
/** Last sign of life of the busy action (start, stage change, install progress), not just its start. */
let runtimeBusyActivityMs = 0;
/** A runtime switch/install/managed change is between its first click and its busy flag. */
let runtimeChangeInFlight = false;
/** When the runtime was last switched or seen to change version; older backend verdicts are stale. */
let lastRuntimeChangeMs = 0;
/** The target version that was selected when `runtimeDetect` was requested (`target_installed` is about that). */
let runtimeDetectTarget = "";
let runtimeStateRefreshInFlight: Promise<void> | null = null;
let runtimeStateLastRefreshMs = 0;
let runtimeVersionCatalogLastRefreshMs = 0;
let lastAutostartWarningMs = 0;
let runtimeBackgroundStartUntilMs = 0;
let runtimeBackgroundStartPollInFlight: Promise<void> | null = null;
let runtimeStateDriftLogged = false;
let runtimeRefreshTraceSignature = "";

let renderFrame: number | null = null;
const PASSIVE_RUNTIME_REFRESH_TTL_MS = 1500;
const VERSION_CATALOG_REFRESH_TTL_MS = 5 * 60_000;

const RUNTIME_ACTION_LABELS: Record<string, string> = {
  detect: "Detecting runtime...",
  install: "Installing local runtime...",
  "ensure-runtime": "Preparing local runtime...",
  "delete-runtime": "Deleting runtime...",
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

async function invokeWithTimeout<T>(
  command: string,
  args: Record<string, unknown> = {},
  timeoutMs = 2_500
): Promise<T> {
  let timeoutHandle: number | null = null;
  try {
    return await Promise.race([
      invoke<T>(command, args),
      new Promise<T>((_, reject) => {
        timeoutHandle = window.setTimeout(() => {
          reject(new Error(`${command} timed out after ${timeoutMs}ms`));
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timeoutHandle !== null) {
      window.clearTimeout(timeoutHandle);
    }
  }
}

const startRuntimeCommand = () =>
  invokeWithTimeout<OllamaRuntimeStartResult>("start_ollama_runtime", {}, RUNTIME_START_TIMEOUT_MS);
const verifyRuntimeCommand = () =>
  invokeWithTimeout<OllamaRuntimeVerifyResult>("verify_ollama_runtime", {}, RUNTIME_ACTION_TIMEOUT_MS);

export type OllamaRuntimeCardState = {
  detected: boolean;
  healthy: boolean;
  source: string;
  version: string;
  managedPid: number | null;
  managedAlive: boolean;
  endpoint: string;
  busy: boolean;
  backgroundStarting: boolean;
  busyAction: string | null;
  stage: WizardStage;
  primaryAction: OllamaRuntimePrimaryAction;
  primaryLabel: string;
  primaryDisabled: boolean;
  detail: string;
  compatibilityWarning: string | null;
};

function compareSemverLoose(left: string, right: string): number {
  const l = left
    .trim()
    .split(".")
    .map((v) => Number.parseInt(v, 10) || 0);
  const r = right
    .trim()
    .split(".")
    .map((v) => Number.parseInt(v, 10) || 0);
  const len = Math.max(l.length, r.length);
  for (let i = 0; i < len; i += 1) {
    const a = l[i] ?? 0;
    const b = r[i] ?? 0;
    if (a !== b) return a > b ? 1 : -1;
  }
  return 0;
}

function isOllamaProvider(): boolean {
  return settings?.ai_fallback?.provider === "ollama";
}

function shouldAutoStartLocalRuntime(): boolean {
  if (!settings?.ai_fallback || !settings.providers?.ollama) return false;

  const aiFallback = settings.ai_fallback;
  if (!aiFallback.enabled) return false;
  return aiFallback.provider === "ollama";
}

function showAutostartWarning(trigger: "bootstrap" | "enable_toggle", message: string): void {
  if (trigger === "bootstrap") {
    return;
  }
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
  return Boolean(
    runtimeVerify?.ok
    || runtimeHealth?.ok
    || runtimeDiagnostics?.ollama?.reachable
    || runtimeDiagnostics?.ollama?.spawn_stage === "ready"
    || startupStatus?.ollama_ready
  );
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isRuntimeBackgroundStarting(): boolean {
  return Boolean(startupStatus?.ollama_starting)
    || (runtimeBackgroundStartUntilMs > Date.now() && !runtimeIsHealthy());
}

function traceRuntimeStateDriftIfNeeded(): void {
  const drift = runtimeIsHealthy() && Boolean(startupStatus?.ollama_starting);
  if (drift && !runtimeStateDriftLogged) {
    runtimeStateDriftLogged = true;
    traceFrontendWarn("ollama.state", "healthy runtime with stale startup starting flag", {
      startupStatus,
      runtimeDiagnostics: runtimeDiagnostics?.ollama ?? null,
    });
    return;
  }
  if (!drift) {
    runtimeStateDriftLogged = false;
  }
}

function formatOllamaDiagnosticDetail(): string | null {
  const diagnostics = runtimeDiagnostics?.ollama;
  if (!diagnostics) return null;
  const error = diagnostics.last_error?.trim();
  switch (diagnostics.spawn_stage) {
    case "spawn_failed":
      return error ? `Spawn failed: ${error}` : "Spawn failed.";
    case "job_assign_failed":
      return error || "Runtime started with degraded cleanup path.";
    case "health_timeout":
      return error || "Health timeout. Runtime is still warming up.";
    case "verify_failed":
      return error ? `Verify failed: ${error}` : "Verify failed.";
    case "runtime_not_found":
      return error || "No local runtime detected.";
    case "running_externally":
      return "Running externally.";
    case "ready":
      return "Runtime ready.";
    default:
      return error || null;
  }
}

function traceRuntimeRefreshStateIfChanged(reason: string): void {
  const stage = runtimeDiagnostics?.ollama?.spawn_stage ?? "unknown";
  const healthy = runtimeHealth?.ok ?? runtimeIsHealthy();
  const error = runtimeDiagnostics?.ollama?.last_error?.trim() || "";
  const signature = `${stage}|${healthy ? "1" : "0"}|${error}`;
  if (signature === runtimeRefreshTraceSignature) {
    return;
  }
  runtimeRefreshTraceSignature = signature;
  traceFrontendInfo("ollama.refresh", reason, {
    healthy,
    stage,
    error: error || null,
    detail: formatOllamaDiagnosticDetail(),
  });
}

function ollamaRuntimeFailureTitle(defaultAction: string): string {
  const stage = runtimeDiagnostics?.ollama?.spawn_stage;
  if (stage === "spawn_failed") return "Runtime start failed (spawn)";
  if (stage === "health_timeout") return "Runtime start delayed (health)";
  if (stage === "verify_failed") return "Runtime verify failed";
  if (stage === "runtime_not_found") return "Runtime not found";
  return defaultAction;
}

function startRuntimeBackgroundPolling(reason: "autostart" | "manual"): void {
  runtimeBackgroundStartUntilMs = Date.now() + BACKGROUND_START_POLL_MAX_MS;
  renderOllamaModelManager();
  traceFrontendInfo("ollama.background", "background polling started", { reason });

  if (runtimeBackgroundStartPollInFlight) {
    return;
  }

  runtimeBackgroundStartPollInFlight = (async () => {
    while (Date.now() < runtimeBackgroundStartUntilMs) {
      await refreshOllamaRuntimeState({ force: true, verify: true });
      if (runtimeIsHealthy()) {
        await refreshOllamaInstalledModels();
        runtimeBackgroundStartUntilMs = 0;
        traceFrontendInfo("ollama.background", "background polling reached healthy state", { reason });
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
    traceFrontendInfo("ollama.background", "background polling finished", { reason });
  });
}

function effectiveRuntimeBusyAction(): string | null {
  if (
    (runtimeBusyAction === "start" || runtimeBusyAction === "ensure-runtime")
    && runtimeBusyActivityMs > 0
    && Date.now() - runtimeBusyActivityMs > RUNTIME_BUSY_START_STALE_MS
  ) {
    console.warn(`Clearing stale runtime busy state: ${runtimeBusyAction}`);
    runtimeBusyAction = null;
    runtimeBusyActivityMs = 0;
  }
  if (!runtimeBusyAction) return null;
  if ((runtimeBusyAction === "start" || runtimeBusyAction === "ensure-runtime") && runtimeIsHealthy()) {
    return null;
  }
  return runtimeBusyAction;
}

function setRuntimeBusyAction(action: string | null): void {
  runtimeBusyAction = action;
  runtimeBusyActivityMs = action ? Date.now() : 0;
}

/**
 * Progress of the busy action: a 1.6 GB runtime download legitimately takes far longer than the
 * silence limit, and releasing the flag mid-download would let a second install start next to it.
 */
function noteRuntimeProgress(progress: OllamaRuntimeInstallProgress | null): void {
  runtimeInstallProgress = progress;
  if (runtimeBusyAction) runtimeBusyActivityMs = Date.now();
}

export async function autoStartLocalRuntimeIfNeeded(
  trigger: "bootstrap" | "enable_toggle"
): Promise<void> {
  traceFrontendInfo("ollama.autostart", "autostart requested", { trigger });
  if (!shouldAutoStartLocalRuntime()) {
    traceFrontendInfo("ollama.autostart", "autostart skipped: policy disabled", { trigger });
    return;
  }

  await refreshOllamaRuntimeState({ force: true });
  const card = getOllamaRuntimeCardState();
  traceFrontendInfo("ollama.autostart", "runtime state before autostart", {
    trigger,
    healthy: card.healthy,
    detected: card.detected,
    busy: card.busy,
    backgroundStarting: card.backgroundStarting,
    stage: card.stage,
    detail: card.detail,
  });
  if (card.busy || card.healthy || !card.detected) {
    return;
  }

  if (runtimeBusyAction) {
    return;
  }

  setRuntimeBusyAction("start");
  runtimeInstallError = null;
  renderOllamaModelManager();
  try {
    const startResult = await startRuntimeCommand();
    traceFrontendInfo("ollama.autostart", "start_ollama_runtime returned", startResult);
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
    traceFrontendError("ollama.autostart", "autostart failed", { trigger, message });
    showAutostartWarning(trigger, message);
  } finally {
    setRuntimeBusyAction(null);
    renderOllamaModelManager();
    traceFrontendInfo("ollama.autostart", "autostart finalized", { trigger });
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

/** The catalog says this runtime version already has a folder with ollama.exe on disk. */
function isRuntimeVersionOnDisk(version: string): boolean {
  // The detect result checks the file system for the selected target. The catalog also counts
  // a version recorded in settings as installed even when its folder is gone, which would
  // offer "Switch to X" for a runtime that cannot start.
  const target = currentTargetVersion();
  // `target_installed` is about the target that was selected when that detect ran; after a new
  // pick in the dropdown it says nothing about the new one until the next detect.
  if (runtimeDetect?.target_installed !== undefined && version === target && runtimeDetectTarget === target) {
    return Boolean(runtimeDetect.target_installed);
  }
  return runtimeVersionCatalog.some((entry) => entry.version === version && entry.installed);
}

/**
 * The selected target. An old default that was never a choice (0.20.2) reads as the recommended
 * one, and a target nobody picked that is older than the running runtime reads as the running one:
 * the UI must never offer a downgrade the user did not ask for. (Read on a copy; the settings
 * themselves are healed by `refreshOllamaRuntimeState` and by the backend on every save.)
 */
function currentTargetVersion(): string {
  const ollama = settings?.providers?.ollama;
  if (!ollama) return "";
  const view = { ...ollama };
  syncRuntimeTarget(view, runtimeDetect?.found ? runtimeDetect.version : null);
  return view.runtime_target_version;
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
  // If the user has selected a different target version, offer to install it, or to switch to it
  // when the catalog says it is already on disk (no download, no install).
  const targetVersion = currentTargetVersion();
  const detectedVersion = runtimeDetect?.version?.trim();
  if (targetVersion && detectedVersion && targetVersion !== detectedVersion) {
    return {
      action: "install",
      label: `${isRuntimeVersionOnDisk(targetVersion) ? "Switch to" : "Install"} ${targetVersion}`,
      disabled: false,
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

/**
 * Shows a modal dialog asking the user if they want to install Ollama
 * for AI Refinement. Returns a Promise that resolves to true if the user
 * clicks "Jetzt installieren", false if they click "Später".
 */
export function showOllamaRequiredModal(): Promise<boolean> {
  const dom = document.getElementById("ollama-required-modal") as HTMLDivElement | null;
  if (!dom) {
    console.error("ollama-required-modal element not found");
    return Promise.resolve(false);
  }

  return new Promise((resolve) => {
    const installBtn = dom.querySelector("#ollama-required-install") as HTMLButtonElement | null;
    const cancelBtn = dom.querySelector("#ollama-required-cancel") as HTMLButtonElement | null;
    const backdrop = dom.querySelector("#ollama-required-modal-backdrop") as HTMLDivElement | null;

    const cleanup = () => {
      dom.setAttribute("hidden", "");
      installBtn?.removeEventListener("click", onInstall);
      cancelBtn?.removeEventListener("click", onCancel);
      backdrop?.removeEventListener("click", onCancel);
    };

    const onInstall = () => {
      cleanup();
      resolve(true);
    };

    const onCancel = () => {
      cleanup();
      resolve(false);
    };

    // Show modal
    dom.removeAttribute("hidden");

    // Add listeners
    installBtn?.addEventListener("click", onInstall);
    cancelBtn?.addEventListener("click", onCancel);
    backdrop?.addEventListener("click", onCancel);
  });
}

export function getOllamaRuntimeCardState(): OllamaRuntimeCardState {
  traceRuntimeStateDriftIfNeeded();
  const source = runtimeDetect?.source || settings?.providers?.ollama?.runtime_source || "per_user_zip";
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
        : formatOllamaDiagnosticDetail() || stageHint(stage));
  const compatibilityWarning =
    activeModelRequiredRuntime &&
    version &&
    version !== "unknown" &&
    compareSemverLoose(version, activeModelRequiredRuntime) < 0
      ? `Active model requires Ollama >= ${activeModelRequiredRuntime}; current runtime is ${version}.`
      : null;

  return {
    detected: Boolean(runtimeDetect?.found),
    healthy: runtimeIsHealthy(),
    source,
    version: version || "unknown",
    managedPid: runtimeDetect?.managed_pid ?? null,
    managedAlive: Boolean(runtimeDetect?.managed_alive),
    endpoint,
    busy: Boolean(busyAction),
    backgroundStarting,
    busyAction,
    stage,
    primaryAction: primary.action,
    primaryLabel: primary.label,
    primaryDisabled: primary.disabled,
    detail,
    compatibilityWarning,
  };
}

/** What the dropdown shows while the backend has not told us anything: the target (or the default) as pinned. */
function fallbackRuntimeCatalog(): OllamaRuntimeVersionInfo[] {
  const version = currentTargetVersion() || DEFAULT_RUNTIME_VERSION;
  return [
    {
      version,
      source: "pinned",
      selected: true,
      installed: false,
      recommended: true,
      prerelease: /(?:-rc|-alpha|-beta)/i.test(version),
      installable: true,
      installable_reason: null,
    },
  ];
}

/** The backend answer is untrusted at this edge: only objects with a version string survive. */
function sanitizeRuntimeCatalog(raw: unknown): OllamaRuntimeVersionInfo[] {
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (entry): entry is OllamaRuntimeVersionInfo =>
      Boolean(entry) &&
      typeof entry === "object" &&
      typeof (entry as { version?: unknown }).version === "string" &&
      (entry as { version: string }).version.trim().length > 0
  );
}

export function getOllamaRuntimeVersionCatalog(): OllamaRuntimeVersionInfo[] {
  return runtimeVersionCatalog.length > 0 ? runtimeVersionCatalog : fallbackRuntimeCatalog();
}

export async function refreshOllamaRuntimeVersionCatalog(force = false): Promise<void> {
  const now = Date.now();
  if (!force && now - runtimeVersionCatalogLastRefreshMs < VERSION_CATALOG_REFRESH_TTL_MS) {
    return;
  }

  try {
    // No network call here: pinned + last cached online catalog + runtimes on disk.
    const versions = sanitizeRuntimeCatalog(await invokeWithTimeout<unknown>("list_ollama_runtime_versions", {}, RUNTIME_ACTION_TIMEOUT_MS));
    // Never replace a known catalog by nothing usable.
    if (versions.length > 0) {
      runtimeVersionCatalog = versions;
      runtimeVersionCatalogLastRefreshMs = now;
      autoFetchOnlineVersionCatalogOnce();
    }
  } catch (error) {
    console.warn("Failed to refresh Ollama runtime version catalog:", error);
  }
}

let onlineVersionFetchInProgress = false;

export const RUNTIME_CATALOG_UPDATED_EVENT = "ollama-runtime-catalog-updated";
let autoCatalogFetchStarted = false;

/**
 * Fetches the major-line version list from GitHub and merges it into the catalog.
 * `force` bypasses the backend's 6 h cache (the "Get versions" button); the automatic
 * once-per-session fetch uses the cache. Failures are shown, not swallowed.
 */
export async function fetchOnlineVersionCatalog(
  onStatus: (msg: string | null) => void,
  force = true
): Promise<void> {
  if (onlineVersionFetchInProgress) return;
  onlineVersionFetchInProgress = true;
  onStatus("Fetching version list from GitHub...");
  try {
    const versions = sanitizeRuntimeCatalog(
      await invokeWithTimeout<unknown>("fetch_ollama_online_versions", { force }, ONLINE_CATALOG_TIMEOUT_MS)
    );
    if (versions.length === 0) throw new Error("The answer contained no usable versions.");
    runtimeVersionCatalog = versions;
    runtimeVersionCatalogLastRefreshMs = Date.now();
    onStatus(null);
    window.dispatchEvent(new CustomEvent(RUNTIME_CATALOG_UPDATED_EVENT));
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    console.warn("Failed to fetch online Ollama versions:", msg);
    onStatus(`Could not fetch the version list: ${msg}`);
    // Keep whatever is cached on disk visible instead of an empty or stale list.
    try {
      const cached = sanitizeRuntimeCatalog(await invokeWithTimeout<unknown>("list_ollama_runtime_versions", {}, RUNTIME_ACTION_TIMEOUT_MS));
      if (cached.length > 0) runtimeVersionCatalog = cached;
    } catch {
      /* keep the previous catalog */
    }
  } finally {
    onlineVersionFetchInProgress = false;
  }
}

/** One cached catalog refresh per app session so the list is never "unchecked". */
function autoFetchOnlineVersionCatalogOnce(): void {
  if (autoCatalogFetchStarted) return;
  autoCatalogFetchStarted = true;
  void fetchOnlineVersionCatalog(() => {}, false);
}

export function isOnlineVersionFetchInProgress(): boolean {
  return onlineVersionFetchInProgress;
}

/**
 * Ollama keeps the case of the pulled ref (`hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M`) and
 * compares names case-insensitively (types/model/name.go, `Name.EqualFold`), so installed
 * models are matched case-insensitively against the catalog ref.
 */
export function findInstalledModel(
  installed: readonly InstalledOllamaModel[],
  name: string | null | undefined
): InstalledOllamaModel | undefined {
  const target = normalizeModelTag(name);
  if (!target) return undefined;
  return installed.find((m) => isExactModelTagMatch(m.name, target));
}

function isModelInstalled(name: string): boolean {
  return Boolean(findInstalledModel(installedOllamaModels, name));
}

function getInstalledSize(name: string): number {
  return findInstalledModel(installedOllamaModels, name)?.size_bytes ?? 0;
}

/** Backend text fields are untrusted: anything that is not a string counts as missing. */
function textOf(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

/** First 12 hex characters of a digest, like `ollama list` shows them. */
export function shortDigest(digest: unknown): string {
  return textOf(digest).replace(/^sha256:/i, "").slice(0, MIN_COMPARABLE_DIGEST_LENGTH);
}

/** Registry build id comparable across models (full digest, case-insensitive); null when too short to trust. */
function comparableBuild(digest: unknown): string | null {
  const value = textOf(digest).replace(/^sha256:/i, "").toLowerCase();
  return value.length >= MIN_COMPARABLE_DIGEST_LENGTH ? value : null;
}

/** Keeps the rows of `fetch_ollama_models_with_size` that name a model and coerces the metadata to plain types. */
export function sanitizeInstalledModels(raw: unknown): InstalledOllamaModel[] {
  if (!Array.isArray(raw)) return [];
  const rows: InstalledOllamaModel[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const row = item as Record<string, unknown>;
    if (!textOf(row.name)) continue;
    const size = row.size_bytes;
    rows.push({
      name: row.name as string,
      size_bytes: typeof size === "number" && Number.isFinite(size) ? size : 0,
      digest: textOf(row.digest) || undefined,
      modified_at: textOf(row.modified_at) || undefined,
      family: textOf(row.family) || undefined,
      parameter_size: textOf(row.parameter_size) || undefined,
      quantization_level: textOf(row.quantization_level) || undefined,
    });
  }
  return rows;
}

/** "Verified working" chip with the proof as tooltip; null without a (valid) proof. */
export function functionalProofChip(proof: ModelFunctionalProof | null | undefined): ModelFunctionalChip | null {
  if (!proof) return null;
  const checked = formatReleaseDate(`${proof.verified_at}T12:00:00Z`);
  const latency =
    proof.latency_p50_ms >= 1000 ? `${(proof.latency_p50_ms / 1000).toFixed(1)} s` : `${proof.latency_p50_ms} ms`;
  const parts = [
    `Passed a functional check on Ollama ${proof.runtime}${checked ? ` (${checked})` : ""}: typical response ${latency}.`,
  ];
  if (proof.note) parts.push(proof.note);
  return { label: "Verified working", title: parts.join(" ") };
}

/** "c6eb396dbd59 · pulled 23 Jul 2026 · 8.0B · Q4_K_M"; unknown parts are left out. */
export function formatInstalledModelMeta(model: InstalledOllamaModel): string {
  const parts: string[] = [];
  const digest = shortDigest(model.digest);
  if (digest) parts.push(digest);
  const pulled = formatReleaseDate(textOf(model.modified_at));
  if (pulled) parts.push(`pulled ${pulled}`);
  const parameters = textOf(model.parameter_size);
  if (parameters) parts.push(parameters);
  const quantization = textOf(model.quantization_level);
  if (quantization) parts.push(quantization);
  return parts.join(" · ");
}

/** "Ollama library · recommended · size checked 2 Nov 2026" for a catalog card. */
export function formatCatalogModelMeta(spec: ModelCatalogEntry): string {
  const parts = [catalogSourceLabel(spec.source)];
  if (spec.status === "recommended" || spec.status === "legacy") parts.push(spec.status);
  if (spec.verified_at) {
    // Date-only value: pin to noon UTC so no timezone shifts it to the previous day.
    const checked = formatReleaseDate(`${spec.verified_at}T12:00:00Z`);
    if (checked) parts.push(`size checked ${checked}`);
  }
  return parts.join(" · ");
}

export function modelUpdateChip(info: OllamaModelUpdateInfo | null | undefined): ModelUpdateChip | null {
  switch (info?.status) {
    case "current":
      return { status: "current", label: "Current", title: "Matches the build the registry serves for this tag." };
    case "outdated":
      return { status: "outdated", label: "Outdated", title: "The registry serves a different build for this tag." };
    case "local":
      return { status: "local", label: "Local", title: "Not available in a public registry (imported or custom model)." };
    default:
      return null;
  }
}

/** "newer build available (registry dc35e8d9c606, 6.6 GB)" for outdated models, otherwise null. */
export function modelUpdateHint(info: OllamaModelUpdateInfo | null | undefined): string | null {
  if (info?.status !== "outdated") return null;
  const details: string[] = [];
  const digest = shortDigest(info.remote_digest);
  if (digest) details.push(`registry ${digest}`);
  if (info.remote_size_bytes && info.remote_size_bytes > 0) details.push(formatBytesGb(info.remote_size_bytes));
  return details.length > 0 ? `newer build available (${details.join(", ")})` : "newer build available";
}

function isModelUpdateEntryFresh(
  entry: ModelUpdateCacheEntry | undefined,
  digest: string,
  nowMs: number
): boolean {
  if (!entry || entry.digest !== digest) return false;
  const ttl = entry.info ? MODEL_UPDATE_TTL_MS : MODEL_UPDATE_FAILURE_TTL_MS;
  return nowMs - entry.checkedAtMs < ttl;
}

/** Cached registry status of the installed models, keyed by normalized name. */
export function getOllamaModelUpdateMap(): Map<string, OllamaModelUpdateInfo> {
  const map = new Map<string, OllamaModelUpdateInfo>();
  for (const model of installedOllamaModels) {
    const key = normalizeModelTag(model.name);
    const entry = modelUpdateCache.get(key);
    if (entry?.info && entry.digest === textOf(model.digest)) {
      map.set(key, entry.info);
    }
  }
  return map;
}

const KNOWN_UPDATE_STATUSES: ReadonlySet<string> = new Set(["current", "outdated", "local"]);

/** Entry counts of the in-memory caches (diagnostics and tests): they must stay bounded. */
export function getOllamaCacheStats(): { updates: number; requirements: number; refusals: number; trackers: number } {
  return {
    updates: modelUpdateCache.size,
    requirements: requirementCache.size,
    refusals: runtimeRefusals.size,
    trackers: pullTrackers.size,
  };
}

/** Registry status of a model that is no longer installed is of no use any more. */
function pruneUpdateCache(): void {
  const installed = new Set(installedOllamaModels.map((model) => normalizeModelTag(model.name)));
  for (const key of [...modelUpdateCache.keys()]) {
    if (!installed.has(key)) modelUpdateCache.delete(key);
  }
}

/**
 * Asks the backend (registry manifests only) whether the installed builds are still what the
 * registries serve. Results are cached for 6 h per name+digest; failures (offline, strict local
 * mode, timeout) are cached for 5 min and show no chip. Never throws, never toasts.
 */
export function refreshOllamaModelUpdateStatuses(nowMs: number = Date.now()): Promise<void> {
  if (modelUpdateInFlight) return modelUpdateInFlight;

  const stale = installedOllamaModels
    .map((model) => ({ name: model.name, digest: textOf(model.digest) }))
    .filter(
      (model) =>
        shortDigest(model.digest).length >= MIN_COMPARABLE_DIGEST_LENGTH &&
        !isModelUpdateEntryFresh(modelUpdateCache.get(normalizeModelTag(model.name)), model.digest, nowMs)
    )
    .slice(0, MODEL_UPDATE_MAX_PER_CALL);
  if (stale.length === 0) return Promise.resolve();

  modelUpdateInFlight = (async () => {
    const results = new Map<string, OllamaModelUpdateInfo>();
    try {
      const response = await invokeWithTimeout<unknown>(
        "check_ollama_model_updates",
        { models: stale },
        MODEL_UPDATE_TIMEOUT_MS
      );
      if (Array.isArray(response)) {
        for (const row of response as OllamaModelUpdateInfo[]) {
          if (row && typeof row.name === "string") results.set(normalizeModelTag(row.name), row);
        }
      }
    } catch (error) {
      traceFrontendWarn("ollama.models", "model update check failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    }
    for (const model of stale) {
      const key = normalizeModelTag(model.name);
      const info = results.get(key);
      modelUpdateCache.set(key, {
        digest: model.digest,
        checkedAtMs: nowMs,
        info: info && KNOWN_UPDATE_STATUSES.has(info.status) ? info : null,
      });
    }
  })().finally(() => {
    modelUpdateInFlight = null;
    renderOllamaModelManager();
    // Newly known outdated models may need a newer runtime for their update.
    void refreshOllamaModelRequirements();
    // The list may have changed while the check ran (update, replace): check what is stale now.
    // Every answer, including a failure, is cached, so this settles after at most one more round.
    void refreshOllamaModelUpdateStatuses();
  });
  return modelUpdateInFlight;
}

// ---------------------------------------------------------------------------
// Runtime requirements ("Needs Ollama >= X") per model
// ---------------------------------------------------------------------------

// `requires` of a tag never changes, so the answer is cached for 6 h; whether the *running*
// runtime satisfies it is recomputed from the current version on every render (no network),
// so the chip disappears right after a runtime update. Failures stay silent and retry after 5 min.
const REQUIREMENT_TTL_MS = 6 * 60 * 60_000;
const REQUIREMENT_FAILURE_TTL_MS = 5 * 60_000;
const REQUIREMENT_TIMEOUT_MS = 45_000;
const REQUIREMENT_MAX_PER_CALL = 32;
type RequirementCacheEntry = { checkedAtMs: number; info: OllamaModelRequirement | null };
const requirementCache = new Map<string, RequirementCacheEntry>();
let requirementsInFlight: Promise<void> | null = null;

/** A pull the runtime refused as too old (HTTP 412), valid only while the same runtime runs. */
type RuntimeRefusal = { requires: string | null; runtime: string };
const runtimeRefusals = new Map<string, RuntimeRefusal>();

// Messages that say which version is needed. The 2nd is the real text of newer runtimes (seen on
// 0.40.7); the 1st is this app's own refusal before a pull starts.
const REQUIRED_VERSION_PATTERNS = [
  /needs ollama\s*(?:>=|≥)\s*v?(\d+\.\d+\.\d+)/i, // "Model 'x' needs Ollama >= 0.40.13 (running 0.40.7). ..."
  /requires ollama version v?(\d+\.\d+\.\d+)/i, // "model requires ollama version v0.40.13 or newer (current version is v0.40.7)"
];
// Older runtimes (seen on 0.30.4) only say that the model needs a newer one, without a number.
const TOO_OLD_WITHOUT_VERSION = [
  /requires a newer version of ollama/i, // "pull model manifest: 412: ... requires a newer version of Ollama"
  /(?:status(?: code)?:?\s*|:\s*)412\b/i,
];

/**
 * Recognises a pull refused because the runtime is too old. Returns `{ requires }` (leading "v"
 * removed; null when the text names no version, then the registry check has to say) or null for
 * every other error.
 */
export function parseRuntimeTooOldError(message: string | null | undefined): { requires: string | null } | null {
  const text = message ?? "";
  for (const pattern of REQUIRED_VERSION_PATTERNS) {
    const match = pattern.exec(text);
    if (match) return { requires: match[1] };
  }
  return TOO_OLD_WITHOUT_VERSION.some((pattern) => pattern.test(text)) ? { requires: null } : null;
}

function currentRuntimeVersion(): string | null {
  const version = runtimeDetect?.found ? runtimeDetect.version?.trim() : "";
  return version || null;
}

/** Next patch release above `version`: the floor when a refusal does not say what is needed. */
function versionAbove(version: string | null): string | null {
  const triple = parseVersionTriple(version);
  return triple ? `${triple[0]}.${triple[1]}.${triple[2] + 1}` : null;
}

export function buildModelRuntimeGate(
  info: OllamaModelRequirement | null | undefined,
  running: string | null,
  catalog: readonly OllamaRuntimeVersionInfo[],
  refusal?: RuntimeRefusal | null
): ModelRuntimeGate | null {
  // The row comes from the backend: wrong types count as "not said".
  const requires = textOf(info?.requires).replace(/^v/i, "") || refusal?.requires || null;
  if (!requires && !refusal) return null;

  const runningVersion = running || textOf(info?.runtime_version) || null;
  const verdict = typeof info?.compatible === "boolean" ? info.compatible : null;
  const byVersion = requires ? compareVersions(runningVersion, requires) : null;
  const compatible = byVersion !== null ? byVersion >= 0 : verdict;
  // Neither something that reads as a version ("0.5" still does) nor a verdict: nothing to tell the user.
  const readable = compareVersions(requires, requires) !== null || /^\d+(?:\.\d+)*$/.test(requires ?? "");
  if (!refusal && compatible === null && !readable) return null;
  const blocked = compatible === false || Boolean(refusal);

  let target: OllamaRuntimeVersionInfo | null = null;
  if (blocked) {
    const floor = requires ?? versionAbove(runningVersion);
    const pick = pickRuntimeVersionForRequirement(catalog, floor);
    // A refusal that contradicts the version numbers must not offer the runtime that is already running.
    if (pick && (compareVersions(pick.version, runningVersion) ?? 1) > 0) target = pick;
  }
  const targetLabel = target
    ? `${target.installed ? "Switch runtime to" : "Update runtime to"} ${target.version}`
    : null;

  const chipLabel = requires ? `Needs Ollama ≥ ${requires}` : "Needs a newer Ollama";
  const need = requires ? `This model needs Ollama ${requires} or newer` : "The runtime refused this model as too old";
  return {
    requires,
    running: runningVersion,
    compatible,
    blocked,
    chipLabel,
    chipTitle: runningVersion ? `${need}. Running: ${runningVersion}.` : `${need}.`,
    target,
    targetLabel,
    hint:
      blocked && !target
        ? `No verified runtime${requires ? ` ≥ ${requires}` : ""} in the catalog yet. Use "Get versions" in the runtime settings.`
        : null,
  };
}

/** Gate for a model name from the caches; null while nothing is known about it. */
export function getModelRuntimeGate(name: string): ModelRuntimeGate | null {
  const key = normalizeModelTag(name);
  const entry = requirementCache.get(key);
  const running = currentRuntimeVersion();
  // The runtime version and verdict stored with an answer describe the runtime of that moment. With
  // no local reading of the running version, an answer older than the last runtime change must
  // not decide (it would resurrect "too old" right after a successful update).
  const info =
    entry?.info && !running && entry.checkedAtMs <= lastRuntimeChangeMs
      ? { ...entry.info, runtime_version: null, compatible: null }
      : (entry?.info ?? null);
  const stored = runtimeRefusals.get(key);
  const refusal = stored && stored.runtime === (running ?? "") ? stored : null;
  return buildModelRuntimeGate(info, running, getOllamaRuntimeVersionCatalog(), refusal);
}

function isRequirementEntryFresh(entry: RequirementCacheEntry | undefined, nowMs: number): boolean {
  if (!entry) return false;
  return nowMs - entry.checkedAtMs < (entry.info ? REQUIREMENT_TTL_MS : REQUIREMENT_FAILURE_TTL_MS);
}

/** Library models worth a requirement check: the not-yet-installed catalog cards and outdated installed models. */
function requirementCandidates(): string[] {
  const names: string[] = [];
  const seen = new Set<string>();
  const add = (name: string) => {
    const key = normalizeModelTag(name);
    if (!key || seen.has(key) || key.startsWith("hf.co/")) return;
    seen.add(key);
    names.push(name);
  };
  for (const spec of OLLAMA_RECOMMENDED_MODELS) {
    if (spec.source === "ollama" && !isModelInstalled(spec.name)) add(spec.name);
  }
  const updates = getOllamaModelUpdateMap();
  for (const model of installedOllamaModels) {
    if (updates.get(normalizeModelTag(model.name))?.status === "outdated") add(model.name);
  }
  return names;
}

/**
 * Reads `requires` of the candidate models from the registry (manifests only, one backend call for
 * all stale names). `force` re-checks those names regardless of the cache. Never throws, never toasts.
 */
export async function refreshOllamaModelRequirements(
  nowMs: number = Date.now(),
  force: readonly string[] = []
): Promise<void> {
  while (requirementsInFlight) await requirementsInFlight;

  // Answers past their TTL are re-fetched when needed anyway: do not keep them for the life of the app.
  for (const [key, entry] of [...requirementCache]) {
    if (!isRequirementEntryFresh(entry, nowMs)) requirementCache.delete(key);
  }
  const forced = new Set(force.map((name) => normalizeModelTag(name)));
  const names = requirementCandidates();
  for (const name of force) {
    if (!names.some((known) => isExactModelTagMatch(known, name))) names.push(name);
  }
  const stale = names
    .filter((name) => {
      const key = normalizeModelTag(name);
      return forced.has(key) || !isRequirementEntryFresh(requirementCache.get(key), nowMs);
    })
    .slice(0, REQUIREMENT_MAX_PER_CALL);
  if (stale.length === 0) return;

  requirementsInFlight = (async () => {
    const results = new Map<string, OllamaModelRequirement>();
    try {
      const response = await invokeWithTimeout<unknown>(
        "check_ollama_model_requirements",
        { models: stale },
        REQUIREMENT_TIMEOUT_MS
      );
      if (Array.isArray(response)) {
        for (const row of response as OllamaModelRequirement[]) {
          if (row && typeof row.name === "string") results.set(normalizeModelTag(row.name), row);
        }
      }
    } catch (error) {
      traceFrontendWarn("ollama.models", "model requirement check failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    }
    for (const name of stale) {
      const key = normalizeModelTag(name);
      requirementCache.set(key, { checkedAtMs: nowMs, info: results.get(key) ?? null });
    }
  })().finally(() => {
    requirementsInFlight = null;
    renderOllamaModelManager();
  });
  await requirementsInFlight;
}

// ---------------------------------------------------------------------------
// Pull progress per layer + completion, from the same events main.ts listens to
// ---------------------------------------------------------------------------

type PullResult = { ok: boolean; error: string | null };

const pullTrackers = new Map<string, PullProgressTracker>();
const pullWaiters = new Map<string, Array<(result: PullResult) => void>>();
/** Per model: the timer that declares a pull dead when it stops reporting. Re-armed by every event. */
const pullWatchdogs = new Map<string, number>();
let pullTracking: Promise<boolean> | null = null;
let pullUnlisteners: Array<() => void> = [];
/** Bumped by dispose: a registration that finishes after it must undo itself. */
let pullTrackingGeneration = 0;
let pullTrackingFailedAtMs = 0;

/** Fired when this module drops a stalled pull on its own, so other views (download popup) redraw. */
export const OLLAMA_PULLS_CHANGED_EVENT = "ollama-pulls-changed";

/** Size the registry will send, to keep the overall bar from dropping when the next layer appears. */
function expectedPullBytes(model: string): number {
  if (modelOperation && isExactModelTagMatch(modelOperation.model, model)) {
    const remote = getOllamaModelUpdateMap().get(normalizeModelTag(model))?.remote_size_bytes;
    if (remote && remote > 0) return remote;
  }
  const spec = getCuratedModelSpec(model);
  return spec ? Math.round(spec.size_gb * 1e9) : 0;
}

/** A pull this view or the backend is known to be running (whatever the spelling of the name). */
function isModelPulling(model: string): boolean {
  const key = normalizeModelTag(model);
  if (!key) return false;
  return (
    activeOllamaPulls.has(key) ||
    pullTrackers.has(key) ||
    [...ollamaPullProgress.keys()].some((name) => normalizeModelTag(name) === key)
  );
}

/** Any download or model operation: restarting Ollama now would cut it off. */
function modelWorkRunning(): boolean {
  return (
    Boolean(modelOperation) ||
    activeOllamaPulls.size > 0 ||
    ollamaPullProgress.size > 0 ||
    pullTrackers.size > 0
  );
}

function armPullWatchdog(model: string): void {
  const key = normalizeModelTag(model);
  if (!key) return;
  const running = pullWatchdogs.get(key);
  if (running !== undefined) window.clearTimeout(running);
  pullWatchdogs.set(
    key,
    window.setTimeout(() => expireStalledPull(model), PULL_STALL_MS)
  );
}

function disarmPullWatchdog(model: string): void {
  const key = normalizeModelTag(model);
  const running = pullWatchdogs.get(key);
  if (running !== undefined) window.clearTimeout(running);
  pullWatchdogs.delete(key);
}

/** No event for a long time: whoever waits is told it failed, and the card is free again. */
function expireStalledPull(model: string): void {
  const key = normalizeModelTag(model);
  pullWatchdogs.delete(key);
  traceFrontendWarn("ollama.pull", "pull stopped reporting; giving up on it", { model });
  clearActiveOllamaPull(model);
  for (const name of [...ollamaPullProgress.keys()]) {
    if (normalizeModelTag(name) === key) ollamaPullProgress.delete(name);
  }
  settlePull(model, { ok: false, error: `No download progress for ${PULL_STALL_MS / 60_000} minutes.` });
  window.dispatchEvent(new CustomEvent(OLLAMA_PULLS_CHANGED_EVENT));
  renderOllamaModelManager();
}

function onPullProgressEvent(payload: OllamaPullProgressType): void {
  if (!payload?.model) return;
  const key = normalizeModelTag(payload.model);
  let tracker = pullTrackers.get(key);
  if (!tracker) {
    tracker = new PullProgressTracker(expectedPullBytes(payload.model));
    pullTrackers.set(key, tracker);
  }
  tracker.record(payload);
  armPullWatchdog(payload.model);
  renderOllamaModelManager();
}

function settlePull(model: string, result: PullResult): void {
  const key = normalizeModelTag(model);
  pullTrackers.delete(key);
  disarmPullWatchdog(model);
  const waiters = pullWaiters.get(key) ?? [];
  pullWaiters.delete(key);
  for (const resolve of waiters) resolve(result);
}

function rememberRuntimeRefusal(model: string, requires: string | null): void {
  const runtime = currentRuntimeVersion() ?? "";
  // A refusal only counts for the runtime that gave it: those of other runtimes are dead weight.
  for (const [key, refusal] of [...runtimeRefusals]) {
    if (refusal.runtime !== runtime) runtimeRefusals.delete(key);
  }
  runtimeRefusals.set(normalizeModelTag(model), { requires, runtime });
}

/** Remembers a "runtime too old" pull error for the card; true when `message` was one. */
function noteRuntimeRefusal(model: string, message: string | null | undefined): boolean {
  const tooOld = parseRuntimeTooOldError(message);
  if (!tooOld) return false;
  rememberRuntimeRefusal(model, tooOld.requires);
  // The text names no version: ask the registry which one the model needs, so the card can say it.
  if (!tooOld.requires) void refreshOllamaModelRequirements(Date.now(), [model]);
  return true;
}

function onPullFinishedEvent(model: string | undefined, error: string | null): void {
  if (!model) return;
  if (error) noteRuntimeRefusal(model, error);
  clearActiveOllamaPull(model);
  settlePull(model, { ok: !error, error });
  renderOllamaModelManager();
}

/**
 * Starts (once) listening to the pull events for per-layer progress and update waiting.
 * Resolves false when the event bridge is unavailable; callers that must wait for a pull
 * (update flow) then refuse to start. `force` is for user actions and tries again at once; the
 * renders that call this on every pass wait out a short back-off after a failure.
 */
export function ensureOllamaPullTracking(force = false): Promise<boolean> {
  if (!pullTracking) {
    if (!force && pullTrackingFailedAtMs > 0 && Date.now() - pullTrackingFailedAtMs < PULL_TRACKING_RETRY_MS) {
      return Promise.resolve(false);
    }
    const generation = pullTrackingGeneration;
    pullTracking = (async () => {
      const results = await Promise.allSettled([
        listen<OllamaPullProgressType>("ollama:pull-progress", (event) => onPullProgressEvent(event.payload)),
        listen<OllamaPullComplete>("ollama:pull-complete", (event) =>
          onPullFinishedEvent(event.payload?.model, null)
        ),
        listen<OllamaPullError>("ollama:pull-error", (event) =>
          onPullFinishedEvent(event.payload?.model, event.payload?.error ?? "Download failed")
        ),
      ]);
      const registered = results.flatMap((result) => (result.status === "fulfilled" ? [result.value] : []));
      const failure = results.find((result): result is PromiseRejectedResult => result.status === "rejected");
      const current = generation === pullTrackingGeneration;
      if (failure || !current) {
        // All or nothing: a half registration would double every event after the retry.
        for (const unlisten of registered) unlisten();
        if (current) {
          pullTracking = null;
          pullTrackingFailedAtMs = Date.now();
          traceFrontendWarn("ollama.pull", "pull event tracking unavailable", {
            error: failure?.reason instanceof Error ? failure.reason.message : String(failure?.reason),
          });
        }
        return false;
      }
      pullUnlisteners = registered;
      pullTrackingFailedAtMs = 0;
      return true;
    })();
  }
  return pullTracking;
}

export function disposeOllamaPullTracking(): void {
  pullTrackingGeneration += 1;
  for (const unlisten of pullUnlisteners) unlisten();
  pullUnlisteners = [];
  pullTracking = null;
  // Nobody will hear the end of these pulls any more: tell the waiters instead of leaving them hanging.
  for (const key of [...pullWaiters.keys()]) {
    settlePull(key, { ok: false, error: "Download tracking was stopped." });
  }
  for (const timer of pullWatchdogs.values()) window.clearTimeout(timer);
  pullWatchdogs.clear();
  pullTrackers.clear();
}

/** Overall progress of a running pull (all layers); null before the first line arrived. */
export function getOllamaPullOverall(model: string): ReturnType<PullProgressTracker["snapshot"]> | null {
  return pullTrackers.get(normalizeModelTag(model))?.snapshot() ?? null;
}

/** Resolves when the pull of `model` finishes; register BEFORE starting the pull so no event is missed. */
function waitForPull(model: string): Promise<PullResult> {
  return new Promise((resolve) => {
    const key = normalizeModelTag(model);
    pullWaiters.set(key, [...(pullWaiters.get(key) ?? []), resolve]);
  });
}

// ---------------------------------------------------------------------------
// "Download selected": one pull after the other
// ---------------------------------------------------------------------------

/** Models of "Download selected" that wait for their turn (the one being downloaded is not in here). */
const pullQueue: string[] = [];
let pullQueueRunning = false;

function isModelQueued(model: string): boolean {
  const key = normalizeModelTag(model);
  return pullQueue.some((name) => normalizeModelTag(name) === key);
}

/**
 * `handleOllamaPull` only STARTS a pull (the command returns at once), so awaiting it alone would
 * start all downloads together. Settled once the pull is over; also when it never started (the
 * command failed and said so itself).
 */
async function pullModelAndWait(model: string): Promise<void> {
  await handleOllamaPull(model);
  // No pull running now: it failed to start, or its end event already came in. Otherwise this runs
  // in the same tick as the check, so the end event cannot slip in between.
  if (!isModelPulling(model)) return;
  await waitForPull(model);
}

async function runPullQueue(): Promise<void> {
  if (pullQueueRunning) return;
  pullQueueRunning = true;
  try {
    while (pullQueue.length > 0) {
      if (runtimeActionRunning()) {
        // One note for the rest of the queue; the models can be ticked again afterwards.
        pullQueue.length = 0;
        showBusyToast("Downloads not started", "The local runtime is being changed. Select the models again when that is done.");
        break;
      }
      const model = pullQueue.shift() as string;
      try {
        await pullModelAndWait(model);
      } catch (error) {
        // A failed model must not stop the others.
        traceFrontendWarn("ollama.pull", "queued pull failed", { model, error: errorText(error) });
      }
    }
  } finally {
    pullQueueRunning = false;
    renderOllamaModelManager();
  }
}

/** Adds models to the download queue (those already downloading or queued are skipped) and starts it. */
export function queueModelPulls(models: readonly string[]): void {
  for (const model of models) {
    if (!normalizeModelTag(model) || isModelPulling(model) || isModelQueued(model)) continue;
    pullQueue.push(model);
  }
  renderOllamaModelManager();
  void runPullQueue();
}

function getCuratedModelSpec(name: string): OllamaRecommendedModelSpec | undefined {
  const target = normalizeModelTag(name);
  return OLLAMA_RECOMMENDED_MODELS.find((spec) => isExactModelTagMatch(spec.name, target));
}

export function resolveCuratedModelState(
  specName: string,
  activeModel: string | null | undefined,
  installed: boolean,
  isPulling: boolean
): CuratedModelState {
  const selected = isExactModelTagMatch(specName, activeModel);
  if (isPulling) return "downloading";
  if (selected && installed) return "active";
  if (selected) return "missing";
  if (installed) return "installed";
  return "available";
}

export function buildInstalledOllamaInventory(
  installedModels: InstalledOllamaModel[],
  activeModel: string | null | undefined,
  updates?: ReadonlyMap<string, OllamaModelUpdateInfo>
): InstalledOllamaInventoryItem[] {
  const deduped = new Map<string, InstalledOllamaModel>();
  for (const model of installedModels) {
    const normalized = normalizeModelTag(model.name);
    if (!normalized || deduped.has(normalized)) continue;
    deduped.set(normalized, model);
  }

  // Installed models whose registry status is "current", by the build the registry serves for them.
  const currentByBuild = new Map<string, string>();
  for (const [normalized, model] of deduped) {
    const info = updates?.get(normalized);
    const build = comparableBuild(info?.remote_digest);
    if (info?.status === "current" && build && !currentByBuild.has(build)) {
      currentByBuild.set(build, model.name);
    }
  }

  return Array.from(deduped.entries())
    .map(([normalized, model]): InstalledOllamaInventoryItem => {
      const curatedSpec = getCuratedModelSpec(model.name);
      const isActive = isExactModelTagMatch(model.name, activeModel);
      const update = updates?.get(normalized);
      const build = comparableBuild(update?.remote_digest);
      const outdated = update?.status === "outdated";
      const replaceWith = outdated && build ? (currentByBuild.get(build) ?? null) : null;
      return {
        name: model.name,
        size_bytes: model.size_bytes,
        size_label: formatBytesGb(model.size_bytes),
        is_active: isActive,
        is_curated: Boolean(curatedSpec),
        curated_spec: curatedSpec,
        can_activate: !isActive,
        can_uninstall: !isActive,
        digest_short: shortDigest(model.digest),
        modified_at: model.modified_at ?? "",
        meta_label: formatInstalledModelMeta(model),
        update,
        update_chip: modelUpdateChip(update),
        update_hint: modelUpdateHint(update),
        can_update: outdated && !replaceWith,
        replace_with: replaceWith,
        functional_chip: functionalProofChip(curatedSpec?.functional),
      };
    })
    .sort((left, right) => {
      if (left.is_active !== right.is_active) {
        return left.is_active ? -1 : 1;
      }
      if (left.is_curated !== right.is_curated) {
        return left.is_curated ? -1 : 1;
      }
      return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
    });
}

// resolveCardStatus removed — dropdown UI uses direct isModelActive/isModelInstalled checks

// Helper functions for custom model management removed — simplified dropdown uses curated list only


/**
 * Persist current settings to the backend — fire-and-forget.
 * Uses a 3 s timeout so that a slow/blocked save_settings command
 * can never freeze the Ollama model manager UI flow.
 */
async function persistCurrentSettings(): Promise<void> {
  if (!settings) return;
  const aiFallback = settings.ai_fallback;
  const settingsForSave = {
    ...settings,
    ai_fallback: aiFallback ? { ...aiFallback } : aiFallback,
  };
  if (settingsForSave.ai_fallback) {
    settingsForSave.ai_fallback.prompt_presets = normalizeUserRefinementPromptPresets(
      settingsForSave.ai_fallback.prompt_presets
    );
    settingsForSave.ai_fallback.active_prompt_preset_id = normalizePersistedRefinementPromptPresetId(
      settingsForSave.ai_fallback.active_prompt_preset_id,
      settingsForSave.ai_fallback.prompt_profile,
      settingsForSave.ai_fallback.prompt_presets
    );
  }
  try {
    await invokeWithTimeout("save_settings", { settings: settingsForSave }, SAVE_SETTINGS_TIMEOUT_MS);
  } catch (error) {
    console.error("save_settings failed:", error);
  }
}

async function maybePersistWizardState(): Promise<void> {
  if (!settings) return;
  // "No models" is only a fact once the list was read (start-up refreshes the runtime first, and an
  // unreachable Ollama reads nothing): saving "wizard pending" then would reopen the onboarding.
  if (!installedModelsRead || installedModelsUnavailable) return;

  settings.setup ??= {
    local_ai_wizard_completed: false,
    local_ai_wizard_pending: true,
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
  setRuntimeBusyAction(action);
  runtimeInstallError = null;
  renderOllamaModelManager();

  try {
    await task();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const detail = formatOllamaDiagnosticDetail() || message;
    runtimeInstallError = {
      stage: runtimeDiagnostics?.ollama?.spawn_stage || action,
      error: detail,
    };
    showToast({
      type: "error",
      title: ollamaRuntimeFailureTitle("Runtime action failed"),
      message: detail,
      duration: 6000,
    });
  } finally {
    setRuntimeBusyAction(null);
    renderOllamaModelManager();
  }
}

/** `detect_ollama_runtime`, remembering which target was selected when it ran (see `isRuntimeVersionOnDisk`). */
async function detectRuntime(timeoutMs = RUNTIME_ACTION_TIMEOUT_MS): Promise<OllamaRuntimeDetectResult> {
  const target = currentTargetVersion();
  const result = await invokeWithTimeout<OllamaRuntimeDetectResult>("detect_ollama_runtime", {}, timeoutMs);
  runtimeDetect = result;
  runtimeDetectTarget = target;
  return result;
}

function showSettingsUnavailableToast(): void {
  showToast({
    type: "error",
    title: "Settings unavailable",
    message: "Settings are not loaded yet.",
    duration: 5000,
  });
}

/**
 * Restarting Ollama (runtime switch, install, source change) would cut off a running download or
 * model update. Tells the user and returns true when one is running.
 */
function refuseRuntimeChangeDuringModelWork(): boolean {
  if (!modelWorkRunning()) return false;
  showToast({
    type: "info",
    title: "Runtime change postponed",
    message: "Restarting Ollama would interrupt the running download or model update. Try again when it is done.",
    duration: 5200,
  });
  return true;
}

export async function ensureLocalRuntimeReady(): Promise<void> {
  if (runtimeBusyAction) return;
  if (!settings?.providers?.ollama) {
    showSettingsUnavailableToast();
    return;
  }
  if (refuseRuntimeChangeDuringModelWork()) return;
  const targetVersion = currentTargetVersion() || DEFAULT_RUNTIME_VERSION;

  setRuntimeBusyAction("ensure-runtime");
  runtimeInstallError = null;
  renderOllamaModelManager();

  let installedThisRun = false;
  let switchedThisRun = false;
  let flowStage: "detect" | "download_runtime" | "install_runtime" | "start_runtime" | "verify_runtime" = "detect";

  try {
    noteRuntimeProgress({
      stage: "verify_runtime",
      message: "Checking for local runtime...",
    });
    renderOllamaModelManager();

    runtimeDetect = await detectRuntime();
    const installedVersion = runtimeDetect.version?.trim() || "";
    const needsTarget = !runtimeDetect.found || installedVersion !== targetVersion;
    // A target that already sits on disk is switched to, not downloaded and installed again.
    // (The catalog is refreshed first; this call needs no network.)
    let shouldInstall = needsTarget;
    if (needsTarget && runtimeDetect.found) {
      await refreshOllamaRuntimeVersionCatalog(true);
      if (isRuntimeVersionOnDisk(targetVersion)) {
        shouldInstall = false;
        switchedThisRun = true;
      }
    }
    if (shouldInstall) {
      flowStage = "download_runtime";
      noteRuntimeProgress({
        stage: "download_runtime",
        message: `Downloading runtime ${targetVersion}...`,
      });
      renderOllamaModelManager();

      const download = await invoke<OllamaRuntimeDownloadResult>("download_ollama_runtime", {
        version: targetVersion,
      });
      if (!download.sha256_ok) {
        throw new Error("Runtime checksum verification failed.");
      }

      flowStage = "install_runtime";
      noteRuntimeProgress({
        stage: "install_runtime",
        message: "Installing runtime files...",
        version: download.version,
      });
      renderOllamaModelManager();

      await invoke<OllamaRuntimeInstallResult>("install_ollama_runtime", {
        archivePath: download.archive_path,
      });
      installedThisRun = true;

      // Refresh detection immediately so UI reflects "installed" even if start fails.
      runtimeDetect = await detectRuntime();
      if (!runtimeDetect.found) {
        throw new Error("Runtime installation completed, but no executable was detected.");
      }
    }

    flowStage = "start_runtime";
    noteRuntimeProgress({
      stage: "start_runtime",
      message: "Starting local runtime...",
    });
    renderOllamaModelManager();
    const startResult = await startRuntimeCommand();
    if (installedThisRun || switchedThisRun) lastRuntimeChangeMs = Date.now();
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
    noteRuntimeProgress({
      stage: "verify_runtime",
      message: "Verifying local runtime...",
    });
    renderOllamaModelManager();
    runtimeVerify = await verifyRuntimeCommand();

    await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState({ force: true });
    await maybePersistWizardState();

    const runningVersion = runtimeDetect?.version?.trim() || "";
    if (switchedThisRun && runningVersion && runningVersion !== targetVersion) {
      // Do not report a switch that did not happen (the old server may have kept running).
      showToast({
        type: "warning",
        title: "Runtime not switched",
        message: `Ollama still reports ${runningVersion} instead of ${targetVersion}. Try "Start local runtime" or restart the app.`,
        duration: 8000,
      });
    } else {
      showToast({
        type: "success",
        title: installedThisRun
          ? "Local runtime installed"
          : switchedThisRun
            ? "Runtime switched"
            : "Local runtime ready",
        message: installedThisRun
          ? `Install and startup completed (v${targetVersion}). Local Ollama is ready.`
          : switchedThisRun
            ? `Now running Ollama ${targetVersion}.`
            : "Local Ollama runtime is running and verified.",
        duration: 4000,
      });
    }
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
    setRuntimeBusyAction(null);
    noteRuntimeProgress(null);
    renderOllamaModelManager();
  }
}

async function handleStartRuntime(): Promise<void> {
  await runRuntimeAction("start", async () => {
    const result = await startRuntimeCommand();
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
    runtimeVerify = await verifyRuntimeCommand();
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
    runtimeVerify = await verifyRuntimeCommand();
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

const activatingModels = new Set<string>();

export async function activateOllamaModel(modelName: string): Promise<void> {
  if (!settings) return;
  const normalized = normalizeModelTag(modelName);
  if (!normalized || activatingModels.has(normalized)) return;
  // A model that is being updated or replaced is loaded, unloaded and deleted by that operation.
  if (isOllamaModelOperationRunning(modelName)) {
    showToast({
      type: "info",
      title: "Model busy",
      message: `${modelName} is being updated. Activate it when that is done.`,
      duration: 4200,
    });
    return;
  }
  activatingModels.add(normalized);
  try {
    await activateModelNow(modelName, normalized);
  } finally {
    activatingModels.delete(normalized);
  }
}

async function activateModelNow(modelName: string, normalized: string): Promise<void> {
  if (!settings) return;
  const previousModel = settings.ai_fallback.model;

  settings.ai_fallback.model = normalized;
  settings.postproc_llm_model = normalized;
  settings.providers.ollama.preferred_model = normalized;
  await persistCurrentSettings();

  // Unload the previous model from VRAM to free up GPU memory
  if (previousModel && previousModel !== normalized) {
    await invokeWithTimeout("unload_ollama_model", { model: previousModel }, RUNTIME_ACTION_TIMEOUT_MS).catch(() => {
      // Silently ignore unload errors; it's not critical if unload fails
    });
  }

  showToast({
    type: "success",
    title: "Model activated",
    message: `${modelName} is now active for AI refinement.`,
    duration: 3000,
  });
  // Load it in the background with the refinement runner options so the first dictation does
  // not pay the one-time cold start (7-28 s measured right after a fresh pull).
  void invoke("warmup_ollama_model", { model: normalized }).catch(() => {
    // Not critical: the first refinement just loads the model itself.
  });
  await refreshOllamaInstalledModels();
  renderOllamaModelManager();
}

async function handleOllamaDelete(modelName: string): Promise<void> {
  if (activeOllamaDeletes.has(modelName)) return;

  const normalized = normalizeModelTag(modelName);
  if (!normalized) return;

  if (isExactModelTagMatch(normalized, settings?.ai_fallback?.model)) {
    showToast({
      type: "info",
      title: "Model in use",
      message: "Activate another refinement model before uninstalling this one.",
      duration: 4200,
    });
    return;
  }

  const sizeBytes = getInstalledSize(modelName);
  const sizeHint =
    sizeBytes > 0 ? `\n\nThis will remove about ${formatBytesGb(sizeBytes)} from local Ollama storage.` : "";
  const confirmed = window.confirm(
    `Delete local model "${modelName}" permanently?${sizeHint}\n\nThis removes the downloaded/imported model data from disk. You can install it again later.`
  );
  if (!confirmed) return;

  activeOllamaDeletes.add(modelName);
  renderOllamaModelManager();

  try {
    await invokeWithTimeout("delete_ollama_model", { model: modelName }, DELETE_TIMEOUT_MS);
    const refreshed = await refreshOllamaInstalledModels();
    await refreshOllamaRuntimeState({ force: true, verify: true });

    if (!refreshed) {
      showToast({
        type: "warning",
        title: "Model removed, refresh failed",
        message: `Deleted ${modelName}, but local inventory could not be refreshed automatically.`,
        duration: 5200,
      });
      return;
    }

    showToast({
      type: "success",
      title: "Model uninstalled",
      message:
        sizeBytes > 0
          ? `${modelName} was removed from local storage (${formatBytesGb(sizeBytes)} freed).`
          : `${modelName} was removed from local storage.`,
      duration: 4200,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Uninstall failed",
      message: String(error),
      duration: 5200,
    });
  } finally {
    activeOllamaDeletes.delete(modelName);
    renderOllamaModelManager();
  }
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

// No dedicated chip styles exist yet: borrow the look of existing badges, keep a hook class per status.
const UPDATE_BADGE_CLASS: Record<ModelUpdateChip["status"], string> = {
  current: "qwen-card-badge--installed qwen-card-badge--update-current",
  outdated: "qwen-card-badge--missing qwen-card-badge--update-outdated",
  local: "qwen-card-badge--curated qwen-card-badge--update-local",
};

const OPERATION_PHASE_LABEL: Record<ModelOperationPhase, string> = {
  check: "Checking requirements...",
  backup: "Backing up the current build...",
  download: "Downloading the new build...",
  verify: "Checking the new build...",
  replace: "Checking the current build, then replacing...",
  rollback: "Restoring the previous build...",
};

const OPERATION_BADGE: Record<ModelOperation["kind"], string> = {
  update: "Updating…",
  replace: "Replacing…",
  recheck: "Checking…",
  rollback: "Rolling back…",
};

// ---------------------------------------------------------------------------
// The model list: installed models and catalog entries as one zebra list
// ---------------------------------------------------------------------------

const DETAILS_TIMEOUT_MS = 20_000;

function chip(label: string, classes: string, title?: string): ModelListChip {
  return { label, className: `qwen-card-badge ${classes}`, ...(title ? { title } : {}) };
}

function functionalChip(proof: ModelFunctionalChip | null): ModelListChip | null {
  return proof ? chip(proof.label, "qwen-card-badge--installed qwen-card-badge--functional", proof.title) : null;
}

/** "Needs Ollama >= X": warning look when the running runtime is too old, neutral otherwise. */
function runtimeGateChip(gate: ModelRuntimeGate | null, onlyWhenBlocked: boolean): ModelListChip | null {
  if (!gate || (onlyWhenBlocked && !gate.blocked)) return null;
  const look = gate.blocked
    ? "qwen-card-badge--missing qwen-card-badge--needs-runtime"
    : "qwen-card-badge--curated qwen-card-badge--runtime-ok";
  return chip(gate.chipLabel, look, gate.chipTitle);
}

function runtimeGateAction(gate: ModelRuntimeGate | null): ModelListAction | null {
  if (!gate?.blocked || !gate.target || !gate.targetLabel) return null;
  return {
    action: "update-runtime",
    label: gate.targetLabel,
    className: "qwen-card-btn qwen-card-btn--activate",
    title: gate.target.installed
      ? "This runtime is already on disk. Ollama restarts with it."
      : "Downloads the runtime (checksum and signature are verified), installs it and restarts Ollama.",
    disabled: runtimeActionRunning() || modelWorkRunning(),
    data: { version: gate.target.version },
  };
}

function runtimeGateHint(gate: ModelRuntimeGate | null): ModelListHint | null {
  if (!gate?.blocked) return null;
  const text = gate.hint ?? `Your Ollama runtime${gate.running ? ` (${gate.running})` : ""} is too old for this model. Update it first.`;
  return { text, className: "qwen-card-hint--runtime" };
}

function pullProgress(model: string, line: OllamaPullProgressType | undefined): { percent: number | null; text: string } {
  return describePullProgress(getOllamaPullOverall(model), line);
}

const RECOMMENDED_CHIP_TITLE = "The stable build recommended for refinement.";

const MODEL_NAME_PART = /^[A-Za-z0-9._-]+$/;

/** "Open on Ollama" / "Open on Hugging Face" for a model name; none for local-only models. */
export function modelPageLinks(name: string, status?: OllamaModelUpdateStatus): ModelListLink[] {
  const trimmed = name.trim();
  const valid = (part: string) => MODEL_NAME_PART.test(part) && part !== "." && part !== "..";
  const hf = /^(?:hf\.co|huggingface\.co)\/(.+)$/i.exec(trimmed);
  if (hf) {
    const [repoPath] = hf[1].split(":");
    const parts = repoPath.split("/");
    if (parts.length !== 2 || !parts.every(valid)) return [];
    return [{ label: "Open on Hugging Face", url: `https://huggingface.co/${parts.join("/")}` }];
  }
  if (status === "local") return [];
  const rest = trimmed.replace(/^registry\.ollama\.ai\//i, "");
  const colon = rest.lastIndexOf(":");
  const path = colon === -1 ? rest : rest.slice(0, colon);
  const tag = colon === -1 ? "" : rest.slice(colon + 1);
  const parts = path.split("/");
  if (!parts.every(valid) || (tag !== "" && !valid(tag))) return [];
  let pagePath: string;
  if (parts.length === 1) pagePath = `library/${parts[0]}`;
  else if (parts.length === 2) pagePath = parts.join("/");
  else if (parts.length === 3 && parts[0] === "library") pagePath = parts.slice(1).join("/");
  else return [];
  return [{ label: "Open on Ollama", url: `https://ollama.com/${pagePath}${tag ? `:${tag}` : ""}` }];
}

function modelSourceLabel(name: string, spec: OllamaRecommendedModelSpec | undefined, status?: OllamaModelUpdateStatus): string {
  if (spec) return spec.source === "huggingface" ? "Hugging Face" : "Ollama";
  if (isHuggingFaceModelRef(name)) return "Hugging Face";
  return status === "local" ? "Local" : "Ollama";
}

function catalogInfoFacts(
  spec: OllamaRecommendedModelSpec | undefined
): Pick<ModelListInfo, "description" | "catalogNote" | "released" | "releasedFrom" | "license"> {
  if (!spec) return {};
  return {
    description: spec.description,
    catalogNote: spec.note,
    released: spec.released,
    releasedFrom: spec.released_from,
    license: spec.license,
  };
}

/** Update / "Replace with X" / runtime action of an installed model; empty when nothing applies. */
function installedMaintenanceActions(
  item: InstalledOllamaInventoryItem,
  gate: ModelRuntimeGate | null,
  operation: ModelOperation | null,
  pendingCheck: PendingModelCheck | null
): ModelListAction[] {
  if (operation) {
    return [{ action: "busy", label: OPERATION_BADGE[operation.kind], className: "qwen-card-btn qwen-card-btn--loading", disabled: true }];
  }
  const off = operationBlocked() || isModelPulling(item.name);
  if (pendingCheck) {
    // Both builds exist: the only sensible next steps are to finish the check or to undo the update.
    return [
      {
        action: "recheck",
        label: "Re-run check",
        className: "qwen-card-btn qwen-card-btn--download",
        title: "Runs the functional check on the new build again. Passing it removes the backup.",
        disabled: off,
      },
      {
        action: "rollback",
        label: "Roll back",
        className: "qwen-card-btn qwen-card-btn--delete",
        title: `Removes the new build and restores the previous one from "${pendingCheck.backup}".`,
        disabled: off,
      },
    ];
  }
  if (item.replace_with) {
    return [
      {
        action: "replace",
        label: `Replace with ${item.replace_with}`,
        className: "qwen-card-btn qwen-card-btn--download",
        title: `${item.replace_with} is the current registry build of this model and already installed. It is checked first; only then this older build is removed.`,
        disabled: off,
        data: { target: item.replace_with },
      },
    ];
  }
  if (item.can_update) {
    if (gate?.blocked) {
      const runtime = runtimeGateAction(gate);
      return runtime ? [runtime] : [];
    }
    return [
      {
        action: "update",
        label: "Update",
        className: "qwen-card-btn qwen-card-btn--download",
        title: "Downloads the new build, checks it, and only then replaces this one. The current build is kept as a backup until then.",
        disabled: off,
      },
    ];
  }
  return [];
}

function installedRowFor(item: InstalledOllamaInventoryItem): ModelListRow {
  const operation = modelOperation && isExactModelTagMatch(modelOperation.model, item.name) ? modelOperation : null;
  const pendingCheck = pendingModelChecks.get(normalizeModelTag(item.name)) ?? null;
  const gate = item.can_update && !pendingCheck ? getModelRuntimeGate(item.name) : null;
  const spec = item.curated_spec;
  const installed = findInstalledModel(installedOllamaModels, item.name);
  const recommended = spec?.status === "recommended";

  const chips: ModelListChip[] = [];
  if (operation) chips.push(chip(OPERATION_BADGE[operation.kind], "qwen-card-badge--loading"));
  else if (item.is_active) {
    chips.push(chip("Active", "qwen-card-badge--active", "In use for AI refinement. Activate another model to be able to delete this one."));
  }
  if (pendingCheck) {
    chips.push(chip("Check pending", "qwen-card-badge--missing qwen-card-badge--check-pending", pendingCheck.detail));
  }
  if (recommended) chips.push(chip("Recommended", "qwen-card-badge--curated", RECOMMENDED_CHIP_TITLE));
  if (item.update_chip) {
    chips.push(chip(item.update_chip.label, UPDATE_BADGE_CLASS[item.update_chip.status], item.update_chip.title));
  }
  for (const extra of [functionalChip(item.functional_chip), runtimeGateChip(gate, true)]) if (extra) chips.push(extra);

  const hints: ModelListHint[] = [];
  if (pendingCheck) hints.push({ text: pendingCheckHint(item.name, pendingCheck), className: "qwen-card-hint--check" });
  if (item.update_hint) {
    hints.push({
      text: `${item.update_hint}${item.replace_with ? ` — already installed as ${item.replace_with}` : ""}`,
      className: "qwen-card-hint--outdated",
    });
  }
  const gateHint = runtimeGateHint(gate);
  if (gateHint) hints.push(gateHint);
  if (operation && operation.phase !== "download") {
    hints.push({
      text:
        operation.phase === "replace" && operation.target
          ? `Checking ${operation.target}, then replacing this build...`
          : OPERATION_PHASE_LABEL[operation.phase],
      className: "",
    });
  }

  const lockedByOperation = Boolean(modelOperation);
  const deleting = activeOllamaDeletes.has(item.name);
  const actions: ModelListAction[] = [];
  if (!item.is_active) {
    actions.push({
      action: "activate",
      label: "Activate",
      className: "qwen-card-btn qwen-card-btn--activate",
      title: "Use this model for AI refinement.",
      disabled: lockedByOperation,
    });
  }
  actions.push(...installedMaintenanceActions(item, gate, operation, pendingCheck));
  if (!item.is_active) {
    actions.push({
      action: "delete",
      label: deleting ? "Deleting…" : "Delete",
      className: "qwen-card-btn qwen-card-btn--delete",
      title: "Remove this model from local storage. It can be downloaded again later.",
      disabled: deleting || lockedByOperation,
    });
  }

  const info: ModelListInfo = {
    installed: true,
    ...catalogInfoFacts(spec),
    downloadedAt: item.modified_at || undefined,
    digest: installed?.digest,
    sizeText: item.size_bytes > 0 ? item.size_label : undefined,
    parameterSize: installed?.parameter_size,
    quantization: installed?.quantization_level,
    family: installed?.family,
    requiresText: gate?.requires ? gate.chipTitle : undefined,
    verifiedText: item.functional_chip?.title,
    links: modelPageLinks(item.name, item.update?.status),
    detailSignature: `${installed?.digest ?? "-"}|installed`,
  };

  return {
    key: normalizeModelTag(item.name),
    name: item.name,
    title: spec?.label ?? item.name,
    state: item.is_active ? "active" : "installed",
    sourceLabel: modelSourceLabel(item.name, spec, item.update?.status),
    sizeLabel: item.size_label,
    chips,
    actions,
    hints,
    progress: operation?.phase === "download" ? pullProgress(item.name, ollamaPullProgress.get(item.name)) : undefined,
    info,
  };
}

function catalogRowFor(spec: OllamaRecommendedModelSpec, state: CuratedModelState): ModelListRow {
  const progress = ollamaPullProgress.get(spec.name);
  const gate = getModelRuntimeGate(spec.name);
  const blocked = Boolean(gate?.blocked);
  const sizeLabel = `${spec.size_gb?.toFixed(1) ?? "?"} GB`;
  const isPulling = state === "downloading";

  const queued = !isPulling && isModelQueued(spec.name);

  // One status chip per row (what the row is doing, or why it cannot): the runtime chip shows up
  // only for a real conflict, never as a permanent label.
  const chips: ModelListChip[] = [];
  if (isPulling) chips.push(chip("Downloading…", "qwen-card-badge--loading"));
  else if (queued) chips.push(chip("Queued", "qwen-card-badge--loading", "Waits for the downloads before it."));
  else if (blocked) {
    const conflict = runtimeGateChip(gate, true);
    if (conflict) chips.push(conflict);
  } else if (state === "missing") {
    chips.push(chip("Missing", "qwen-card-badge--missing", "This is the selected model, but it is not installed."));
  }
  if (spec.status === "recommended") chips.push(chip("Recommended", "qwen-card-badge--curated", RECOMMENDED_CHIP_TITLE));
  const proof = functionalChip(functionalProofChip(spec.functional));
  if (proof) chips.push(proof);

  const actions: ModelListAction[] = [];
  if (isPulling) {
    const pct = progress ? computeOllamaPercent(progress) : 0;
    actions.push({
      action: "busy",
      label: `Downloading… ${pct}%`,
      className: "qwen-card-btn qwen-card-btn--loading",
      disabled: true,
    });
  } else if (queued) {
    actions.push({ action: "busy", label: "Queued", className: "qwen-card-btn qwen-card-btn--loading", disabled: true });
  }
  // No per-row download button: rows are ticked and fetched with "Download selected". The pull ref
  // handed to the backend is the row's `name` verbatim (case preserved, incl. hf.co/... refs): the
  // backend keys its progress events by this exact string.
  const select =
    isPulling || queued
      ? undefined
      : {
          sizeGb: spec.size_gb,
          blocked,
          title: blocked ? `${gate?.chipLabel ?? "Runtime too old"}: update the runtime first.` : undefined,
        };
  const runtimeAction = runtimeGateAction(gate);
  if (runtimeAction) actions.push(runtimeAction);

  const hints: ModelListHint[] = [];
  const gateHint = runtimeGateHint(gate);
  if (gateHint) hints.push(gateHint);

  const info: ModelListInfo = {
    installed: false,
    ...catalogInfoFacts(spec),
    sizeText: sizeLabel,
    memoryText: spec.vram_gb != null ? `about ${spec.vram_gb.toFixed(1)} GB of GPU memory` : undefined,
    family: spec.family || undefined,
    requiresText: gate?.requires ? gate.chipTitle : undefined,
    verifiedText: functionalProofChip(spec.functional)?.title,
    links: modelPageLinks(spec.name),
    detailSignature: "-|catalog",
  };

  return {
    key: normalizeModelTag(spec.name),
    name: spec.name,
    title: spec.label,
    state,
    sourceLabel: spec.source === "huggingface" ? "Hugging Face" : "Ollama",
    sizeLabel,
    chips,
    actions,
    hints,
    progress: isPulling && (progress || getOllamaPullOverall(spec.name)) ? pullProgress(spec.name, progress) : undefined,
    select,
    info,
  };
}

/** One list: the active model, then installed ones, then downloadable catalog entries; recommended first within each. */
export function buildModelListRows(): ModelListRow[] {
  const activeModel = settings?.ai_fallback?.model ? normalizeModelTag(settings.ai_fallback.model) : "gemma4:e4b";
  const inventory = buildInstalledOllamaInventory(
    installedOllamaModels,
    settings?.ai_fallback?.model,
    getOllamaModelUpdateMap()
  );
  const entries: Array<{ row: ModelListRow; group: number; recommended: boolean }> = [];
  for (const item of inventory) {
    entries.push({
      row: installedRowFor(item),
      group: item.is_active ? 0 : 1,
      recommended: item.curated_spec?.status === "recommended",
    });
  }
  for (const spec of OLLAMA_RECOMMENDED_MODELS) {
    const installed = isModelInstalled(spec.name);
    const state = resolveCuratedModelState(spec.name, activeModel, installed, isModelPulling(spec.name));
    // Installed and active ones are the rows above (also while their own update downloads).
    if (state === "active" || state === "installed") continue;
    if (installed && isOllamaModelOperationRunning(spec.name)) continue;
    entries.push({
      row: catalogRowFor(spec, state),
      // The selected model that is not installed belongs on top: it is the one the app wants to run.
      group: state === "missing" ? 0 : 2,
      recommended: spec.status === "recommended",
    });
  }
  return entries
    .sort((a, b) => {
      if (a.group !== b.group) return a.group - b.group;
      if (a.recommended !== b.recommended) return a.recommended ? -1 : 1;
      return a.row.title.localeCompare(b.row.title, undefined, { sensitivity: "base" });
    })
    .map((entry) => entry.row);
}

function handleModelListAction(action: string, row: ModelListRow, data: DOMStringMap): void {
  const model = row.name;
  switch (action) {
    case "activate":
      void activateOllamaModel(model);
      break;
    case "delete":
      void handleOllamaDelete(model);
      break;
    case "update":
      void handleOllamaUpdate(model);
      break;
    case "replace":
      void handleOllamaReplace(model, data.target ?? "");
      break;
    case "recheck":
      void handleOllamaRecheck(model);
      break;
    case "rollback":
      void handleOllamaRollback(model);
      break;
    case "update-runtime":
      void updateOllamaRuntimeTo(data.version ?? "");
      break;
  }
}

function openModelPage(url: string): void {
  invoke("open_model_page", { url }).catch((error) => {
    showToast({
      type: "error",
      title: "Could not open the page",
      message: sentence(errorText(error)),
      duration: 5000,
    });
  });
}

const MODEL_LIST_HOOKS: ModelListHooks = {
  onAction: handleModelListAction,
  onDownloadSelected: (rows) => queueModelPulls(rows.map((row) => row.name)),
  downloadBlockedReason: () => (runtimeActionRunning() ? "The runtime is being changed. Wait until that is done." : null),
  loadDetails: (name) => invokeWithTimeout("get_ollama_model_details", { model: name }, DETAILS_TIMEOUT_MS),
  openLink: openModelPage,
};

function renderModelsList(container: HTMLElement): void {
  if (installedModelsUnavailable) {
    const stale = document.createElement("div");
    stale.className = "qwen-card-hint ollama-installed-stale";
    stale.textContent =
      installedOllamaModels.length === 0
        ? "Could not read the installed models: Ollama is not reachable."
        : "Ollama is not reachable right now, so this list may be out of date.";
    container.appendChild(stale);
  } else if (installedModelsRead && installedOllamaModels.length === 0) {
    const empty = document.createElement("div");
    empty.className = "qwen-card-hint ollama-installed-empty";
    empty.textContent = "No local refinement models installed yet. Download one from the list.";
    container.appendChild(empty);
  }
  renderModelList(container, buildModelListRows(), MODEL_LIST_HOOKS);
}

function renderOllamaModelManagerNow(): void {
  const container = document.getElementById("ollama-model-manager");
  if (!container) return;

  const isOllama = isOllamaProvider();
  container.style.display = isOllama ? "" : "none";
  if (!isOllama) return;

  // Idempotent: per-layer progress needs the pull events even for pulls this view did not start.
  void ensureOllamaPullTracking();
  const focus = captureModelListFocus(container);
  container.innerHTML = "";
  renderModelsList(container);
  restoreModelListFocus(container, focus);
}

export function renderOllamaModelManager(): void {
  scheduleRender();
}

/**
 * Reads the installed models. Overlapping calls are common (update end, pull-complete event, delete):
 * only the newest answer may overwrite the list. A failed read keeps the last known list and flags it
 * as possibly stale, instead of claiming that nothing is installed.
 */
export async function refreshOllamaInstalledModels(): Promise<boolean> {
  const seq = (installedRefreshSeq += 1);
  let rows: InstalledOllamaModel[];
  try {
    rows = sanitizeInstalledModels(await invokeWithTimeout<unknown>("fetch_ollama_models_with_size", {}, RUNTIME_ACTION_TIMEOUT_MS));
  } catch (error) {
    console.error("Failed to refresh Ollama models:", error);
    if (seq > installedAppliedSeq) {
      installedAppliedSeq = seq;
      installedModelsUnavailable = true;
      renderOllamaModelManager();
    }
    return false;
  }
  if (seq < installedAppliedSeq) return true; // a newer answer is already in
  installedAppliedSeq = seq;
  installedModelsUnavailable = false;
  installedModelsRead = true;
  installedOllamaModels = rows;
  pruneUpdateCache();
  const installedKeys = new Set(rows.map((row) => normalizeModelTag(row.name)));
  for (const key of [...pendingModelChecks.keys()]) {
    if (!installedKeys.has(key)) pendingModelChecks.delete(key);
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
  // Registry status chips arrive later and re-render on their own; never blocks the inventory.
  void refreshOllamaModelUpdateStatuses();
  void refreshOllamaModelRequirements();
  return true;
}

export async function refreshOllamaRuntimeState(
  options: RuntimeStateRefreshOptions = {}
): Promise<void> {
  const verify = options.verify ?? false;
  const skipDetect = options.skipDetect ?? false;
  const force = options.force ?? false;
  const now = Date.now();
  traceFrontendInfo("ollama.refresh", "refresh requested", {
    force,
    verify,
    skipDetect,
    backgroundPolling: Boolean(runtimeBackgroundStartPollInFlight),
    busyAction: runtimeBusyAction,
    inFlight: Boolean(runtimeStateRefreshInFlight),
  });

  if (runtimeBackgroundStartPollInFlight && force && !verify) {
    traceFrontendInfo("ollama.refresh", "refresh skipped: background start poll in flight", {
      force,
      verify,
    });
    renderOllamaModelManager();
    return;
  }

  if (!verify && !force && now - runtimeStateLastRefreshMs < PASSIVE_RUNTIME_REFRESH_TTL_MS) {
    traceFrontendInfo("ollama.refresh", "refresh skipped: passive ttl", {
      age_ms: now - runtimeStateLastRefreshMs,
    });
    renderOllamaModelManager();
    return;
  }

  if (runtimeStateRefreshInFlight) {
    if (!verify && !force) {
      traceFrontendInfo("ollama.refresh", "refresh skipped: refresh already in flight", {
        verify,
        force,
      });
      return;
    }
    // A forced or verifying call wants fresh state: what the older run read may predate its trigger
    // (a runtime switch, an install), so it runs after that one instead of adopting its answer.
    await runtimeStateRefreshInFlight;
  }

  const refreshTask = (async () => {
    await refreshOllamaRuntimeVersionCatalog(force);

    if (!skipDetect) {
      const before = currentRuntimeVersion();
      try {
        await detectRuntime(RUNTIME_DETECT_TIMEOUT_MS);
        const after = currentRuntimeVersion();
        if (before && after && before !== after) lastRuntimeChangeMs = Date.now();
        // Legacy default / stale target: the target follows a newer running runtime (a pick stays).
        if (settings?.providers?.ollama) syncRuntimeTarget(settings.providers.ollama, after);
      } catch (error) {
        runtimeDetect = null;
        traceFrontendWarn("ollama.refresh", "detect_ollama_runtime failed", {
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    const endpoint = settings?.providers?.ollama?.endpoint || DEFAULT_LOCAL_ENDPOINT;

    if (verify) {
      try {
        runtimeVerify = await invokeWithTimeout<OllamaRuntimeVerifyResult>(
          "verify_ollama_runtime",
          {},
          RUNTIME_VERIFY_TIMEOUT_MS
        );
        runtimeHealth = {
          ok: runtimeVerify.ok,
          endpoint: runtimeVerify.endpoint,
          models_count: runtimeVerify.models_count,
        };
      } catch (error) {
        runtimeVerify = null;
        traceFrontendWarn("ollama.refresh", "verify_ollama_runtime failed", {
          error: error instanceof Error ? error.message : String(error),
        });
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

    activeModelRequiredRuntime = null;
    const activeModel = settings?.ai_fallback?.model?.trim();
    // /api/show needs a serving runtime; skip it (and its timeout) while the runtime is down.
    if (activeModel && runtimeHealth?.ok) {
      try {
        const modelInfo = await invokeWithTimeout<Record<string, unknown>>(
          "get_ollama_model_info",
          { model: activeModel },
          RUNTIME_MODEL_INFO_TIMEOUT_MS
        );
        const requiresRaw = modelInfo?.requires;
        if (typeof requiresRaw === "string" && requiresRaw.trim()) {
          activeModelRequiredRuntime = requiresRaw.trim().replace(/^v/i, "");
        }
      } catch (error) {
        activeModelRequiredRuntime = null;
        traceFrontendWarn("ollama.refresh", "get_ollama_model_info failed", {
          error: error instanceof Error ? error.message : String(error),
        });
      }
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
    traceFrontendInfo("ollama.refresh", "refresh completed", {
      force,
      verify,
      skipDetect,
      healthy: runtimeHealth?.ok ?? false,
      detected: runtimeDetect?.found ?? null,
      serving: runtimeDetect?.is_serving ?? null,
      models: runtimeHealth?.models_count ?? 0,
      activeModelRequiredRuntime,
    });
    renderOllamaModelManager();
    traceRuntimeRefreshStateIfChanged("runtime state changed");
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

export function isRuntimeBackgroundPollingActive(): boolean {
  return Boolean(runtimeBackgroundStartPollInFlight) || runtimeBackgroundStartUntilMs > Date.now();
}

// ---------------------------------------------------------------------------
// Runtime change from a model card, safe model update, duplicate replacement
// ---------------------------------------------------------------------------

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * A runtime action is in progress, whether or not the old runtime still answers: a switch or install
 * runs while it does, and the card's `busy` (which ignores a healthy runtime) must not decide this.
 */
function runtimeActionRunning(): boolean {
  effectiveRuntimeBusyAction(); // applies the silence limit
  return runtimeBusyAction !== null;
}

function operationBlocked(): boolean {
  return Boolean(modelOperation) || runtimeActionRunning();
}

/** Why nothing may start for `model` right now; null when it may. */
function busyReason(model: string): string | null {
  if (modelOperation) return "Another model update or replacement is still running.";
  if (runtimeActionRunning()) return "The local runtime is busy. Try again in a moment.";
  if (isModelPulling(model)) return `${model} is being downloaded right now.`;
  return null;
}

function showBusyToast(title: string, reason: string): void {
  showToast({ type: "info", title, message: reason, duration: 5200 });
}

/** One confirmation at a time: a second click on a button must not stack another dialog. */
async function confirmOnce(options: ConfirmDialogOptions): Promise<boolean> {
  confirmationOpen = true;
  try {
    return await confirmDialog(options);
  } finally {
    confirmationOpen = false;
  }
}

/** Ends a message part with a full stop (backend texts come with and without one). */
function sentence(text: string): string {
  const trimmed = text.trim();
  return /[.!?]$/.test(trimmed) ? trimmed : `${trimmed}.`;
}

/**
 * True while a safe update or replacement runs (for `model` only when given). Lets the generic
 * "Model downloaded" toast stay quiet for the pull that belongs to an update.
 */
export function isOllamaModelOperationRunning(model?: string): boolean {
  if (!modelOperation) return false;
  return model === undefined || isExactModelTagMatch(modelOperation.model, model);
}

function setOperationPhase(phase: ModelOperationPhase): void {
  if (!modelOperation) return;
  modelOperation = { ...modelOperation, phase };
  renderOllamaModelManager();
}

function showRuntimeTooOldToast(model: string, gate: ModelRuntimeGate | null, detail?: string): void {
  const target = gate?.target ?? null;
  showToast({
    type: "warning",
    title: "Runtime too old for this model",
    message:
      detail ||
      `${model}: ${gate?.chipLabel ?? "needs a newer Ollama"}.${gate && !target && gate.hint ? ` ${gate.hint}` : ""}`,
    duration: 8000,
    ...(target && gate?.targetLabel
      ? { actionLabel: gate.targetLabel, onAction: () => updateOllamaRuntimeTo(target.version) }
      : {}),
  });
}

/**
 * Makes `version` the target runtime and runs the normal flow: switch when it is already on disk,
 * otherwise download (checksum + signature are verified by the backend), install and restart.
 */
export async function updateOllamaRuntimeTo(
  version: string,
  options: { explicit?: boolean } = {}
): Promise<void> {
  const target = version.trim();
  // The flag also covers the awaited save below: a second click must not start a second change.
  if (!target || runtimeBusyAction || runtimeChangeInFlight) return;
  if (!settings?.providers?.ollama) {
    showSettingsUnavailableToast();
    return;
  }
  if (refuseRuntimeChangeDuringModelWork()) return;
  runtimeChangeInFlight = true;
  try {
    settings.providers.ollama.runtime_target_version = target;
    // Only a pick in the version list may keep an older target; the "update the runtime for this
    // model" button moves forward and stays an ordinary, healable target.
    settings.providers.ollama.runtime_target_explicit = options.explicit === true;
    await persistCurrentSettings();
    await ensureLocalRuntimeReady();
  } finally {
    runtimeChangeInFlight = false;
  }
  window.dispatchEvent(new CustomEvent(RUNTIME_CATALOG_UPDATED_EVENT));
  renderOllamaModelManager();
}

/**
 * Deletes an installed runtime version after a confirmation. The backend refuses (and says why)
 * when the version is running, recorded as the runtime in use or the selected target.
 */
export async function deleteOllamaRuntimeVersion(version: string): Promise<void> {
  const target = version.trim();
  if (!target || runtimeBusyAction || runtimeChangeInFlight || confirmationOpen) return;
  const entry = runtimeVersionCatalog.find((candidate) => candidate.version === target);
  const size = formatDownloadSize(entry?.size_bytes);
  const confirmed = await confirmOnce({
    title: `Delete runtime ${target}?`,
    body: [
      `This removes the installed Ollama runtime ${target}${size ? ` (download size ${size})` : ""} from this computer.`,
      "It is not running and not selected. You can install it again later from this list.",
    ],
    confirmLabel: "Delete",
    careful: true,
  });
  if (!confirmed) return;
  await runRuntimeAction("delete-runtime", async () => {
    const result = await invokeWithTimeout<OllamaRuntimeDeleteResult>(
      "delete_ollama_runtime_version",
      { version: target },
      RUNTIME_DELETE_TIMEOUT_MS
    );
    await refreshOllamaRuntimeVersionCatalog(true);
    window.dispatchEvent(new CustomEvent(RUNTIME_CATALOG_UPDATED_EVENT));
    const freed = formatDownloadSize(result.freed_bytes);
    if (result.deleted) {
      showToast({
        type: result.detail ? "warning" : "success",
        title: `Runtime ${target} deleted`,
        message: result.detail ? sentence(result.detail) : freed ? `Freed ${freed}.` : "Done.",
        duration: result.detail ? 7000 : 4000,
      });
    } else {
      showToast({
        type: "warning",
        title: `Runtime ${target} kept`,
        message: sentence(result.detail || "The runtime could not be deleted."),
        duration: 7000,
      });
    }
  });
}

/** What a button of the runtime version list does (Install and Switch to share the normal flow). */
export async function runRuntimeVersionAction(
  version: string,
  action: RuntimeVersionActionId
): Promise<void> {
  if (action === "delete") {
    await deleteOllamaRuntimeVersion(version);
    return;
  }
  // A pick in the list is a choice: it may also be older than the running runtime.
  await updateOllamaRuntimeTo(version, { explicit: true });
}

/** Starts the pull and resolves when its completion/error event arrives (or the pull stops reporting). */
async function pullAndWait(model: string): Promise<PullResult> {
  const waiting = waitForPull(model);
  // Events that never come must not hold the update forever: every event re-arms this.
  armPullWatchdog(model);
  try {
    await invokeWithTimeout("pull_ollama_model", { model }, PULL_START_TIMEOUT_MS);
  } catch (error) {
    const message = errorText(error);
    noteRuntimeRefusal(model, message);
    settlePull(model, { ok: false, error: message });
  }
  return waiting;
}

/** The backend still counts the pull as running for a moment after its completion event. */
async function finalizeUpdate(model: string, backup: string): Promise<unknown> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await invokeWithTimeout<unknown>(
        "finalize_ollama_model_update",
        { model, backup },
        FINALIZE_TIMEOUT_MS
      );
    } catch (error) {
      if (attempt >= 5 || !/still running/i.test(errorText(error))) throw error;
      await sleep(400);
    }
  }
}

type FinalizeOutcome = { verified: boolean; restored: boolean; inconclusive: boolean; detail: string };

/** The answer of `finalize_ollama_model_update`; an unreadable one is treated as "could not tell". */
function readFinalizeResult(raw: unknown): FinalizeOutcome {
  if (!raw || typeof raw !== "object") {
    return { verified: false, restored: false, inconclusive: true, detail: "The backend gave no usable answer." };
  }
  const result = raw as Partial<OllamaModelFinalizeResult>;
  return {
    verified: result.verified === true,
    restored: result.restored === true,
    inconclusive: result.inconclusive === true,
    detail: textOf(result.detail),
  };
}

function pendingCheckHint(model: string, pending: PendingModelCheck): string {
  const reason = pending.detail ? ` (${pending.detail})` : "";
  return (
    `The new build of ${model} could not be checked${reason}. Both builds are kept: the new one under this name, ` +
    `the previous one as "${pending.backup}". Re-run the check or roll back. ` +
    `This notice is lost when the app restarts; the backup copy then stays in this list under that name.`
  );
}

/**
 * Runs the final check of an update (or its re-run) and tells the user what happened. A check that
 * could not finish keeps both builds and leaves a notice on the card; only a clear verdict ends it.
 */
async function finalizeAndReport(modelName: string, backup: string, pull: PullResult | null): Promise<void> {
  const key = normalizeModelTag(modelName);
  let result: FinalizeOutcome;
  try {
    // Also after a failed download: it drops the backup (or restores it if the tag was touched).
    result = readFinalizeResult(await finalizeUpdate(modelName, backup));
  } catch (error) {
    const reason = errorText(error);
    pendingModelChecks.set(key, { backup, detail: reason });
    showToast({
      type: "error",
      title: "Update could not be finished",
      message: `${sentence(reason)} The backup copy "${backup}" was kept.`,
      duration: 9000,
    });
    return;
  }

  const detail = result.detail ? ` ${result.detail}` : "";
  if (result.inconclusive) {
    pendingModelChecks.set(key, { backup, detail: result.detail || "The check did not finish." });
    const download = pull && !pull.ok ? ` The download failed: ${sentence(pull.error ?? "Download failed.")}` : "";
    showToast({
      type: "warning",
      title: "Update check inconclusive",
      message: `The new build of ${modelName} could not be checked, so nothing was removed or restored.${download}${detail} Use "Re-run check" or "Roll back" on its card.`,
      duration: 12000,
    });
    return;
  }

  pendingModelChecks.delete(key);
  if (pull && !pull.ok) {
    showToast({
      type: "error",
      title: "Update failed",
      message: `${sentence(pull.error ?? "Download failed.")} ${modelName} ${result.restored ? "was restored" : "is unchanged"}.`,
      duration: 8000,
    });
  } else if (result.verified) {
    showToast({
      type: "success",
      title: "Model updated",
      message: `${modelName} was updated and passed the functional check. The backup was removed.${detail}`,
      duration: 6000,
    });
  } else if (result.restored) {
    showToast({
      type: "warning",
      title: "Update rolled back",
      message: `The new build of ${modelName} failed the functional check, so the previous build was restored.${detail}`,
      duration: 9000,
    });
  } else {
    showToast({
      type: "error",
      title: "Update check failed",
      message: `The new build of ${modelName} failed the functional check and the previous build could not be restored automatically. Backup copy: "${backup}".${detail}`,
      duration: 12000,
    });
  }
}

/**
 * Update an outdated model without ever being left without a working one:
 * backup (copy, no extra disk) -> pull -> functional check -> only then drop the backup;
 * a failed check restores the old build.
 */
async function handleOllamaUpdate(modelName: string): Promise<void> {
  const key = normalizeModelTag(modelName);
  if (!key || confirmationOpen || pendingModelChecks.has(key)) return;
  const busyBefore = busyReason(modelName);
  if (busyBefore) {
    showBusyToast("Update not started", busyBefore);
    return;
  }

  const knownGate = getModelRuntimeGate(modelName);
  if (knownGate?.blocked) {
    showRuntimeTooOldToast(modelName, knownGate);
    return;
  }

  const remoteBytes = getOllamaModelUpdateMap().get(key)?.remote_size_bytes ?? 0;
  const confirmed = await confirmOnce({
    title: `Update ${modelName}?`,
    body: [
      `Downloads the current registry build${remoteBytes > 0 ? ` (${formatBytesGb(remoteBytes)})` : ""}. Parts you already have are skipped.`,
      "Your installed build stays as a backup and is only replaced after the new build passed a functional check. If the check fails, the old build is restored.",
    ],
    confirmLabel: "Update model",
  });
  if (!confirmed) return;
  const busyAfter = busyReason(modelName);
  if (busyAfter) {
    // Something started while the dialog was open.
    showBusyToast("Update not started", busyAfter);
    return;
  }

  modelOperation = { kind: "update", model: modelName, phase: "check" };
  renderOllamaModelManager();
  try {
    if (!requirementCache.has(key)) await refreshOllamaModelRequirements(Date.now(), [modelName]);
    const gate = getModelRuntimeGate(modelName);
    if (gate?.blocked) {
      showRuntimeTooOldToast(modelName, gate);
      return;
    }
    if (!(await ensureOllamaPullTracking(true))) {
      showToast({
        type: "error",
        title: "Update not started",
        message: "Download events are unavailable, so the update cannot be followed safely. Nothing was changed.",
        duration: 6500,
      });
      return;
    }

    setOperationPhase("backup");
    let backup: string;
    try {
      const made = await invokeWithTimeout<unknown>("backup_ollama_model", { model: modelName }, BACKUP_TIMEOUT_MS);
      if (typeof made !== "string" || !made.trim()) throw new Error("The backend returned no backup name.");
      backup = made.trim();
    } catch (error) {
      showToast({
        type: "error",
        title: "Update not started",
        message: `Could not back up ${modelName}: ${sentence(errorText(error))} Nothing was changed.`,
        duration: 7000,
      });
      return;
    }

    setOperationPhase("download");
    const pull = await pullAndWait(modelName);

    setOperationPhase("verify");
    await finalizeAndReport(modelName, backup, pull);
  } finally {
    modelOperation = null;
    await refreshOllamaInstalledModels();
    renderOllamaModelManager();
  }
}

/** "Re-run check" on a card whose last check was inconclusive. */
async function handleOllamaRecheck(modelName: string): Promise<void> {
  const key = normalizeModelTag(modelName);
  const pending = pendingModelChecks.get(key);
  if (!key || !pending || confirmationOpen) return;
  const busy = busyReason(modelName);
  if (busy) {
    showBusyToast("Check not started", busy);
    return;
  }

  modelOperation = { kind: "recheck", model: modelName, phase: "verify" };
  renderOllamaModelManager();
  try {
    await finalizeAndReport(modelName, pending.backup, null);
  } finally {
    modelOperation = null;
    await refreshOllamaInstalledModels();
    renderOllamaModelManager();
  }
}

/** "Roll back" on a card whose last check was inconclusive: the previous build comes back from the backup. */
async function handleOllamaRollback(modelName: string): Promise<void> {
  const key = normalizeModelTag(modelName);
  const pending = pendingModelChecks.get(key);
  if (!key || !pending || confirmationOpen) return;
  const busy = busyReason(modelName);
  if (busy) {
    showBusyToast("Roll back not started", busy);
    return;
  }

  const confirmed = await confirmOnce({
    title: `Roll back ${modelName}?`,
    body: [
      `The new build of ${modelName} is removed and the previous build is restored from the backup copy "${pending.backup}".`,
      "The new build was not checked successfully, so it is not known whether it works.",
    ],
    confirmLabel: "Roll back",
    careful: true,
  });
  if (!confirmed) return;
  const busyAfter = busyReason(modelName);
  if (busyAfter) {
    showBusyToast("Roll back not started", busyAfter);
    return;
  }

  modelOperation = { kind: "rollback", model: modelName, phase: "rollback" };
  renderOllamaModelManager();
  try {
    const raw = await invokeWithTimeout<unknown>(
      "rollback_ollama_model_update",
      { model: modelName, backup: pending.backup },
      FINALIZE_TIMEOUT_MS
    );
    const result: Partial<OllamaModelRollbackResult> = raw && typeof raw === "object" ? raw : {};
    const detail = textOf(result.detail) ? ` ${textOf(result.detail)}` : "";
    if (result.restored === true) {
      pendingModelChecks.delete(key);
      showToast({
        type: "success",
        title: "Rolled back",
        message: `${modelName} is the previous build again. The backup copy was removed.${detail}`,
        duration: 6000,
      });
    } else {
      showToast({
        type: "error",
        title: "Roll back failed",
        message: `${modelName} could not be rolled back.${detail} The backup copy "${pending.backup}" is still there.`,
        duration: 12000,
      });
    }
  } catch (error) {
    showToast({
      type: "error",
      title: "Roll back failed",
      message: `${sentence(errorText(error))} The backup copy "${pending.backup}" is still there.`,
      duration: 12000,
    });
  } finally {
    modelOperation = null;
    await refreshOllamaInstalledModels();
    renderOllamaModelManager();
  }
}

type ReplaceOutcome = { verified: boolean; switchedActive: boolean; deletedOld: boolean; latencyMs: number; detail: string };

function readReplaceResult(raw: unknown): ReplaceOutcome {
  const result = (raw && typeof raw === "object" ? raw : {}) as Partial<OllamaModelReplaceResult>;
  return {
    verified: result.verified === true,
    switchedActive: result.switched_active === true,
    deletedOld: result.deleted_old === true,
    latencyMs: typeof result.latency_ms === "number" && Number.isFinite(result.latency_ms) ? result.latency_ms : 0,
    detail: textOf(result.detail),
  };
}

/**
 * Two installed builds of the same model: check the current one first, then remove the old one
 * (the backend switches the active model if it was the old one).
 */
async function handleOllamaReplace(oldName: string, newName: string): Promise<void> {
  if (!normalizeModelTag(oldName) || !normalizeModelTag(newName) || confirmationOpen) return;
  const busyBefore = busyReason(oldName);
  if (busyBefore) {
    showBusyToast("Replace not started", busyBefore);
    return;
  }

  const oldBytes = getInstalledSize(oldName);
  const wasActive = isExactModelTagMatch(oldName, settings?.ai_fallback?.model);
  const confirmed = await confirmOnce({
    title: `Replace ${oldName}?`,
    body: [
      `${newName} is already installed and is the current registry build of the same model; ${oldName} is an older build.`,
      `${newName} is checked first with a short functional test. Only if it passes, ${oldName} is removed${oldBytes > 0 ? ` (frees up to ${formatBytesGb(oldBytes)})` : ""}${wasActive ? ` and the active model switches to ${newName}` : ""}. If the check fails, nothing is changed.`,
    ],
    confirmLabel: `Replace with ${newName}`,
    careful: true,
  });
  if (!confirmed) return;
  const busyAfter = busyReason(oldName);
  if (busyAfter) {
    showBusyToast("Replace not started", busyAfter);
    return;
  }

  modelOperation = { kind: "replace", model: oldName, phase: "replace", target: newName };
  renderOllamaModelManager();
  try {
    const result = readReplaceResult(
      await invokeWithTimeout<unknown>(
        "replace_ollama_model",
        { oldModel: oldName, newModel: newName },
        FINALIZE_TIMEOUT_MS
      )
    );
    const detail = result.detail ? ` ${result.detail}` : "";
    if (!result.verified) {
      showToast({
        type: "warning",
        title: "Nothing replaced",
        message: `${newName} did not pass the functional check, so ${oldName} was kept and nothing was changed.${detail}`,
        duration: 9000,
      });
    } else if (!result.deletedOld) {
      showToast({
        type: "warning",
        title: "Replaced, old build still installed",
        message: `${newName} passed the check${result.switchedActive ? " and is now active" : ""}, but ${oldName} could not be removed.${detail}`,
        duration: 9000,
      });
    } else {
      showToast({
        type: "success",
        title: "Model replaced",
        message: `${newName} passed the functional check (${result.latencyMs} ms). ${oldName} was removed${result.switchedActive ? " and the active model switched to the new build" : ""}.`,
        duration: 6000,
      });
    }
    if (result.verified && result.switchedActive && settings) {
      // The backend saved and announced the change; mirror it in case that event is still on its way.
      const next = newName;
      if (isExactModelTagMatch(settings.ai_fallback.model, oldName)) settings.ai_fallback.model = next;
      if (isExactModelTagMatch(settings.postproc_llm_model, oldName)) settings.postproc_llm_model = next;
      if (isExactModelTagMatch(settings.providers.ollama.preferred_model, oldName)) {
        settings.providers.ollama.preferred_model = next;
      }
    }
  } catch (error) {
    showToast({
      type: "error",
      title: "Replace failed",
      message: `${sentence(errorText(error))} ${oldName} was not removed.`,
      duration: 8000,
    });
  } finally {
    modelOperation = null;
    await refreshOllamaInstalledModels();
    renderOllamaModelManager();
  }
}

async function handleOllamaPull(modelName: string): Promise<void> {
  const key = normalizeModelTag(modelName);
  // Double click, or a pull that is already running (its card may not have caught up yet).
  if (!key || isModelPulling(modelName)) return;
  if (runtimeActionRunning()) {
    showBusyToast("Download not started", "The local runtime is being changed. Try again when that is done.");
    return;
  }
  traceFrontendInfo("ollama.pull", "pull requested", { model: modelName });
  // Stays until the completion/error event (or the watchdog): the command only starts the pull.
  activeOllamaPulls.add(key);
  armPullWatchdog(modelName);
  renderOllamaModelManager();

  try {
    // Listen before the first progress line can arrive (per-layer bar, "runtime too old" mapping).
    await ensureOllamaPullTracking(true);
    await invokeWithTimeout("pull_ollama_model", { model: modelName }, PULL_START_TIMEOUT_MS);
    traceFrontendInfo("ollama.pull", "pull command accepted", { model: modelName });
  } catch (error) {
    // The command failed, so no pull is running and no event will ever clear the card.
    clearActiveOllamaPull(modelName);
    disarmPullWatchdog(modelName);
    const message = errorText(error);
    traceFrontendWarn("ollama.pull", "pull command failed", { model: modelName, error: message });
    if (noteRuntimeRefusal(modelName, message)) {
      // Same hint as the card's chip: what is needed and the one-click fix.
      showRuntimeTooOldToast(modelName, getModelRuntimeGate(modelName), message);
    } else {
      showToast({
        type: "error",
        title: "Download failed",
        message,
      });
    }
  }
  renderOllamaModelManager();
}

// handleOllamaDelete removed — simplified dropdown UI; delete via ollama CLI if needed

/** Overall percent over all layers when the pull is tracked, otherwise the newest line's own percent. */
export function computeOllamaPercent(progress: OllamaPullProgressType): number {
  const overall = progress.model ? getOllamaPullOverall(progress.model) : null;
  if (overall && overall.percent !== null) return overall.percent;
  if (progress.total && progress.total > 0 && progress.completed) {
    return Math.min(100, Math.round((progress.completed / progress.total) * 100));
  }
  return 0;
}

/** Bar value (null = no size known yet) and caption for a pull, for the whole model rather than one layer. */
export function describePullProgress(
  overall: ReturnType<PullProgressTracker["snapshot"]> | null,
  line: OllamaPullProgressType | undefined
): { percent: number | null; text: string } {
  if (overall) {
    if (overall.percent === null) return { percent: null, text: overall.status || "Starting..." };
    const sizes = `${formatDownloadSize(overall.completed) || "0 MB"} / ${formatDownloadSize(overall.total)}`;
    return { percent: overall.percent, text: `${overall.percent}% (${sizes})${overall.status ? ` - ${overall.status}` : ""}` };
  }
  if (line) {
    return { percent: line.total && line.completed ? computeOllamaPercent(line) : null, text: line.status || "Starting..." };
  }
  return { percent: null, text: "Starting..." };
}

/** Decimal GB (10^9), like the registries and `ollama list` show sizes - not GiB. */
export function formatBytesGb(bytes: number): string {
  if (!(bytes > 0)) return "Unknown size";
  return `${(bytes / 1e9).toFixed(1)} GB`;
}

export function clearActiveOllamaPull(modelName: string): void {
  activeOllamaPulls.delete(normalizeModelTag(modelName));
}

export function setOllamaRuntimeInstallProgress(progress: OllamaRuntimeInstallProgress): void {
  noteRuntimeProgress(progress);
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
