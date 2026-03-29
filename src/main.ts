// Main entry point - Bootstrap and backend event listeners

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import {
  installGlobalFrontendErrorLogging,
  traceFrontendError,
  traceFrontendInfo,
  traceFrontendWarn,
} from "./frontend-trace";
import { initWindowStatePersistence } from "./window-state";

type TranscriptionStatus = "idle" | "recording" | "transcribing";
type AudioCueType = "start" | "stop";

import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  DownloadComplete,
  DownloadError,
  QuantizeProgress,
  ErrorEvent,
  TranscribeBacklogStatus,
  OllamaPullProgress,
  OllamaPullComplete,
  OllamaPullError,
  OllamaRuntimeInstallProgress,
  OllamaRuntimeInstallComplete,
  OllamaRuntimeInstallError,
  OllamaRuntimeHealth,
  OverlayHealthEvent,
  RuntimeDiagnostics,
  TranscriptionRefinedEvent,
  TranscriptionRefinementFailedEvent,
  TranscriptionRefinementStartedEvent,
  TranscriptionRefinementActivityEvent,
  TranscriptionGpuActivityEvent,
  TranscriptionResultEvent,
  TranscriptionRawResultEvent,
  DependencyPreflightReport,
  StabilityDegradedEvent,
  StartupStatus,
  AssistantActionResultEvent,
  AssistantAwaitingConfirmationEvent,
  AssistantConfirmationExpiredEvent,
  AssistantIntentDetectedEvent,
  AssistantPlanReadyEvent,
  AssistantStateChangedEvent,
} from "./types";
import {
  settings,
  history,
  transcribeHistory,
  setSettings,
  setHistory,
  setTranscribeHistory,
  setDevices,
  setOutputDevices,
  models,
  setModels,
  setDynamicSustainThreshold,
  modelProgress,
  quantizeProgress,
  ollamaPullProgress,
  startupStatus,
  setRuntimeDiagnostics,
  setOverlayHealth,
  setStartupStatus,
  isRefinementEnabled,
} from "./state";
import * as dom from "./dom-refs";
import { renderAIFallbackSettingsUi, renderSettings } from "./settings";
import { renderDevices, renderOutputDevices } from "./devices";
import {
  renderHero,
  setCaptureStatus,
  setGpuActivity,
  setRefiningActive,
  setTranscribeStatus,
  updateThresholdMarkers,
} from "./ui-state";
import { scheduleHistoryRender, setHistoryTab, initHistoryDelegation } from "./history";
import { initPanelState, isPanelCollapsed, setPanelCollapsed } from "./panels";
import { renderModels, refreshModels, refreshModelsDir } from "./models";
import {
  wireEvents,
  initMainTab,
  cleanupWindowListeners,
  scheduleSettingsRender,
  reconcileMainTabVisibility,
} from "./event-listeners";
import { initUnifiedTooltips, cleanupUnifiedTooltips } from "./custom-tooltips";
import { dismissToast, showToast, showErrorToast } from "./toast";
import { playAudioCue } from "./audio-cues";
import { levelToDb, thresholdToPercent } from "./ui-helpers";
import { dumpHistoryToFile, initLiveDump } from "./live-dump";
import { initExportDialog } from "./export-dialog";
import { initArchiveBrowser } from "./archive-browser";
import { initExpertMode } from "./expert-mode";
import { initModulesHub, refreshModulesHub } from "./modules-hub";
import { initGddFlow } from "./gdd-flow";
import { initOnboardingWizard } from "./onboarding-wizard";
import { initPipelineStatus } from "./pipeline-status";
import {
  appendWorkflowAgentLog,
  handleAssistantActionResult as handleWorkflowAssistantActionResult,
  handleAssistantAwaitingConfirmation,
  handleAssistantConfirmationExpired,
  handleAssistantIntentDetected,
  handleAssistantStateChanged,
  handleWorkflowAgentRawResult,
  initWorkflowAgentConsole,
  syncWorkflowAgentConsoleState,
} from "./workflow-agent-console";
import { syncVoiceOutputConsoleState } from "./voice-output-console";
import {
  handleRefinementFailureForInspector,
  handleRefinementStartedForInspector,
  handleRefinementSuccessForInspector,
  handleTranscriptionResultForInspector,
  markAllPendingAsFailed,
  pruneOrphanedSnapshots,
  restoreRefinementInspector,
  renderRefinementInspector,
} from "./refinement-inspector";
import {
  handlePipelineRefined,
  handlePipelineRefinementFailed,
  reconcilePipelineRefinementIdle,
  handlePipelineRefinementReset,
  handlePipelineRefinementStarted,
  handlePipelineRefinementTimeout,
  handlePipelineTranscriptionResult,
} from "./refinement-pipeline-graph";
import {
  activateOllamaModel,
  autoStartLocalRuntimeIfNeeded,
  clearActiveOllamaPull,
  getOllamaRuntimeCardState,
  isRuntimeBackgroundPollingActive,
  renderOllamaModelManager,
  refreshOllamaInstalledModels,
  refreshOllamaRuntimeState,
  setOllamaRuntimeHealth,
  setOllamaRuntimeInstallComplete,
  setOllamaRuntimeInstallError,
  setOllamaRuntimeInstallProgress,
} from "./ollama-models";
import { OLLAMA_SETTINGS_CHANGED_POLICY } from "./ollama-refresh-policy";

// Track event listeners for cleanup to prevent memory leaks
let eventUnlisteners: Array<() => void> = [];
let backlogWarningToastId: string | null = null;
let overlayHealthToastId: string | null = null;
let ollamaRuntimeLoadingToastId: string | null = null;
let pasteQueue: Promise<void> = Promise.resolve();
let frontendHeartbeatTimer: number | null = null;
let frontendHeartbeatFailureCount = 0;
let frontendHeartbeatReloadIssued = false;
let lastOllamaStartupStarting = false;
let lastOllamaStartupReady = false;

const FRONTEND_HEARTBEAT_INTERVAL_MS = 2_500;
const FRONTEND_HEARTBEAT_RELOAD_THRESHOLD = 5;
const FRONTEND_HEARTBEAT_IPC_TIMEOUT_MS = 1_500;
const AUTO_RELOAD_WINDOW_MS = 10 * 60_000;
const AUTO_RELOAD_MAX_PER_WINDOW = 3;
const AUTO_RELOAD_LEDGER_KEY = "trispr_flow_auto_reload_ledger_v1";

function loadAutoReloadLedger(): number[] {
  try {
    const raw = window.sessionStorage.getItem(AUTO_RELOAD_LEDGER_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed?.timestamps_ms)) return [];
    return parsed.timestamps_ms
      .map((value: unknown) => Number(value))
      .filter((value: number) => Number.isFinite(value) && value > 0);
  } catch {
    return [];
  }
}

function saveAutoReloadLedger(timestamps: number[]): void {
  try {
    window.sessionStorage.setItem(AUTO_RELOAD_LEDGER_KEY, JSON.stringify({ timestamps_ms: timestamps }));
  } catch {
    // Ignore storage errors; watchdog still works without persistence.
  }
}

function tryAutoReloadWithBudget(context: string): boolean {
  const now = Date.now();
  const timestamps = loadAutoReloadLedger().filter((ts) => now - ts <= AUTO_RELOAD_WINDOW_MS);
  if (timestamps.length >= AUTO_RELOAD_MAX_PER_WINDOW) {
    traceFrontendError("frontend.watchdog", "auto-reload suppressed (budget exhausted)", {
      context,
      budget: AUTO_RELOAD_MAX_PER_WINDOW,
      window_ms: AUTO_RELOAD_WINDOW_MS,
    });
    showToast({
      type: "warning",
      title: "Auto-recovery paused",
      message: "Too many automatic reload attempts. Please reopen Trispr Flow manually.",
      duration: 9000,
    });
    return false;
  }
  timestamps.push(now);
  saveAutoReloadLedger(timestamps);
  window.location.reload();
  return true;
}

type PendingDeferredPasteJob = {
  rawText: string;
  timeoutHandle: number;
};

type DeferredPasteOutcome = "refined" | "failed" | "timed_out";

const pendingDeferredPasteJobs = new Map<string, PendingDeferredPasteJob>();
const deferredPasteOutcomes = new Map<string, DeferredPasteOutcome>();
const deferredRefinedTextByJobId = new Map<string, string>();
const trackedRefinementJobs = new Set<string>();
const MAX_DEFERRED_PASTE_OUTCOMES = 500;
let backendRefinementActiveCount = 0;

function clearPendingDeferredPasteJobs() {
  for (const pending of pendingDeferredPasteJobs.values()) {
    window.clearTimeout(pending.timeoutHandle);
  }
  pendingDeferredPasteJobs.clear();
}

function rememberDeferredPasteOutcome(
  jobId: string,
  outcome: DeferredPasteOutcome,
  refinedText?: string
): void {
  deferredPasteOutcomes.set(jobId, outcome);
  if (outcome === "refined" && typeof refinedText === "string") {
    deferredRefinedTextByJobId.set(jobId, refinedText);
  } else {
    deferredRefinedTextByJobId.delete(jobId);
  }
  if (deferredPasteOutcomes.size <= MAX_DEFERRED_PASTE_OUTCOMES) {
    return;
  }
  const first = deferredPasteOutcomes.keys().next();
  if (!first.done) {
    deferredRefinedTextByJobId.delete(first.value);
    deferredPasteOutcomes.delete(first.value);
  }
}

function reportRuntimeMetric(metric: string): void {
  void invoke("record_runtime_metric", { metric }).catch((error) => {
    console.warn("record_runtime_metric failed", metric, error);
  });
}

function syncRefiningIndicator(preferTrackedState = false): void {
  const trackedActive = trackedRefinementJobs.size > 0;
  const active = preferTrackedState
    ? trackedActive
    : trackedActive || backendRefinementActiveCount > 0;
  setRefiningActive(active);
}

function markRefinementJobStarted(jobId: string): void {
  const normalized = jobId.trim();
  if (!normalized) return;
  trackedRefinementJobs.add(normalized);
  syncRefiningIndicator();
}

function markRefinementJobFinished(jobId: string): void {
  const normalized = jobId.trim();
  if (!normalized) return;
  trackedRefinementJobs.delete(normalized);
  syncRefiningIndicator();
}

function resetTrackedRefinementJobs(): void {
  trackedRefinementJobs.clear();
  backendRefinementActiveCount = 0;
  syncRefiningIndicator(true);
}

function applyStartupStatus(status: StartupStatus | null): void {
  syncOllamaStartupToasts(status);
  setStartupStatus(status);
  applyStartupReadinessUi();
  renderHero();
  renderAIFallbackSettingsUi();
  renderOllamaModelManager();
}

function syncOllamaStartupToasts(status: StartupStatus | null): void {
  const localAiEnabled = isRefinementEnabled() && settings?.ai_fallback?.provider === "ollama";
  const starting = Boolean(status?.ollama_starting);
  const ready = Boolean(status?.ollama_ready);

  if (!localAiEnabled) {
    dismissToast(ollamaRuntimeLoadingToastId);
    ollamaRuntimeLoadingToastId = null;
    lastOllamaStartupStarting = starting;
    lastOllamaStartupReady = ready;
    return;
  }

  if (starting && !ready && !ollamaRuntimeLoadingToastId) {
    ollamaRuntimeLoadingToastId = showToast({
      type: "info",
      icon: "🔵",
      title: "Lokale AI startet",
      message: "Ollama wird geladen. Das kann etwas dauern, bis es verfügbar ist.",
      duration: 0,
    });
  }

  if ((!starting || ready) && ollamaRuntimeLoadingToastId) {
    dismissToast(ollamaRuntimeLoadingToastId);
    ollamaRuntimeLoadingToastId = null;
  }

  const transitionedToReady = ready && !lastOllamaStartupReady && (lastOllamaStartupStarting || starting);
  if (transitionedToReady) {
    showToast({
      type: "success",
      title: "Lokale AI bereit",
      message: "Ollama ist jetzt erreichbar und kann für AI Refinement verwendet werden.",
      duration: 4500,
    });
  }

  lastOllamaStartupStarting = starting;
  lastOllamaStartupReady = ready;
}

function applyStartupReadinessUi(): void {
  const ready = Boolean(startupStatus?.interactive && startupStatus?.transcription_ready);
  const controls = [
    dom.captureEnabledToggle,
    dom.productModeSelect,
    dom.modeSelect,
    dom.deviceSelect,
    dom.pttHotkey,
    dom.pttHotkeyRecord,
    dom.toggleHotkey,
    dom.toggleHotkeyRecord,
    dom.transcribeEnabledToggle,
    dom.transcribeHotkey,
    dom.transcribeHotkeyRecord,
    dom.transcribeDeviceSelect,
  ];
  for (const control of controls) {
    if (control) {
      control.disabled = !ready;
    }
  }
}

async function refreshStartupStatusFromBackend(): Promise<void> {
  try {
    const nextStatus = await invoke<StartupStatus>("get_startup_status");
    applyStartupStatus(nextStatus);
  } catch (error) {
    console.warn("get_startup_status failed", error);
  }
}

function queueTranscriptPaste(text: string, context: string): void {
  const trimmed = text.trim();
  if (!trimmed) return;

  pasteQueue = pasteQueue
    .catch(() => {})
    .then(async () => {
      await invoke("paste_transcript_text", { text });
    })
    .catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`paste_transcript_text failed (${context})`, message);
      if (dom.statusMessage) {
        dom.statusMessage.textContent = message;
      }
      showToast({
        type: "error",
        title: "Paste Failed",
        message,
        duration: 7000,
      });
    });
}

function settleDeferredPasteJob(jobId: string): PendingDeferredPasteJob | null {
  const pending = pendingDeferredPasteJobs.get(jobId);
  if (!pending) {
    return null;
  }
  window.clearTimeout(pending.timeoutHandle);
  pendingDeferredPasteJobs.delete(jobId);
  return pending;
}

function handleDeferredPasteTimeout(jobId: string): void {
  const pending = pendingDeferredPasteJobs.get(jobId);
  if (!pending) {
    return;
  }

  pendingDeferredPasteJobs.delete(jobId);
  markRefinementJobFinished(jobId);
  rememberDeferredPasteOutcome(jobId, "timed_out");
  handlePipelineRefinementTimeout(jobId);
  reportRuntimeMetric("refinement_timeout");
  queueTranscriptPaste(pending.rawText, `timeout:${jobId}`);
}

async function sendFrontendHeartbeat(source: "startup" | "interval"): Promise<void> {
  try {
    await new Promise<void>((resolve, reject) => {
      const timeoutHandle = window.setTimeout(() => {
        reject(new Error(`frontend_heartbeat timed out after ${FRONTEND_HEARTBEAT_IPC_TIMEOUT_MS}ms`));
      }, FRONTEND_HEARTBEAT_IPC_TIMEOUT_MS);
      void invoke("frontend_heartbeat")
        .then(() => resolve())
        .catch((error) => reject(error))
        .finally(() => {
          window.clearTimeout(timeoutHandle);
        });
    });
    if (frontendHeartbeatFailureCount > 0) {
      traceFrontendInfo("frontend.watchdog", "frontend heartbeat recovered", {
        source,
        previousFailures: frontendHeartbeatFailureCount,
      });
    }
    frontendHeartbeatFailureCount = 0;
    frontendHeartbeatReloadIssued = false;
  } catch (error) {
    frontendHeartbeatFailureCount += 1;
    if (frontendHeartbeatFailureCount === 1 || frontendHeartbeatFailureCount % 5 === 0) {
      traceFrontendWarn("frontend.watchdog", "frontend heartbeat failed", {
        source,
        failures: frontendHeartbeatFailureCount,
        error: error instanceof Error ? error.message : String(error),
      });
    }
    if (
      frontendHeartbeatFailureCount >= FRONTEND_HEARTBEAT_RELOAD_THRESHOLD
      && !frontendHeartbeatReloadIssued
    ) {
      frontendHeartbeatReloadIssued = true;
      traceFrontendError("frontend.watchdog", "backend IPC heartbeat unavailable — reloading window", {
        failures: frontendHeartbeatFailureCount,
      });
      tryAutoReloadWithBudget("heartbeat_ipc_unavailable");
    }
  }
}

function stopFrontendHeartbeatWatchdog(): void {
  if (frontendHeartbeatTimer !== null) {
    window.clearInterval(frontendHeartbeatTimer);
    frontendHeartbeatTimer = null;
  }
}

function startFrontendHeartbeatWatchdog(): void {
  stopFrontendHeartbeatWatchdog();
  frontendHeartbeatFailureCount = 0;
  frontendHeartbeatReloadIssued = false;
  void sendFrontendHeartbeat("startup");
  frontendHeartbeatTimer = window.setInterval(() => {
    void sendFrontendHeartbeat("interval");
  }, FRONTEND_HEARTBEAT_INTERVAL_MS);
}

function cleanupEventListeners() {
  stopFrontendHeartbeatWatchdog();
  clearPendingDeferredPasteJobs();
  resetTrackedRefinementJobs();
  cancelPendingRenderFrames();
  cleanupUnifiedTooltips();
  cleanupWindowListeners();
  eventUnlisteners.forEach((unlisten) => unlisten());
  eventUnlisteners = [];
  dismissToast(backlogWarningToastId);
  backlogWarningToastId = null;
  dismissToast(overlayHealthToastId);
  overlayHealthToastId = null;
}

function initConversationView() {
  const params = new URLSearchParams(window.location.search);
  const isConversationOnly = params.get("view") === "conversation";
  const applyConversationOnly = () => {
    document.body.classList.add("conversation-only");
    setHistoryTab("conversation");
  };
  if (isConversationOnly) {
    applyConversationOnly();
  }

  if ((window as unknown as { __TRISPR_VIEW__?: string }).__TRISPR_VIEW__ === "conversation") {
    applyConversationOnly();
  }

  const onTrisprView = (event: Event) => {
    const detail = (event as CustomEvent<string>).detail;
    if (detail === "conversation") applyConversationOnly();
  };
  window.addEventListener("trispr:view", onTrisprView);
  eventUnlisteners.push(() => window.removeEventListener("trispr:view", onTrisprView));

  const stored = Number(localStorage.getItem("conversationFontSize") ?? "16");
  const size = Number.isFinite(stored) ? stored : 16;
  document.documentElement.style.setProperty("--conversation-font-size", `${size}px`);
  if (dom.conversationFontSize) {
    dom.conversationFontSize.value = size.toString();
  }
  if (dom.conversationFontSizeValue) {
    dom.conversationFontSizeValue.textContent = `${size}px`;
  }
}

// ── RAF-guarded render helpers ──────────────────────────────────────────────
// High-frequency Tauri events (audio meters, download progress) can fire
// 30-60× per second.  Instead of touching the DOM on every event, we buffer
// the latest value and flush once per animation frame.

let _pendingAudioLevel: number | null = null;
let _pendingTranscribeLevel: number | null = null;
let _pendingTranscribeDb: number | null = null;
let _meterRafId: number | null = null;

function flushMeterUpdates(): void {
  _meterRafId = null;

  if (_pendingAudioLevel !== null) {
    const level = _pendingAudioLevel;
    _pendingAudioLevel = null;
    if (dom.vadMeterFill) {
      dom.vadMeterFill.style.width = `${thresholdToPercent(level)}%`;
    }
    if (dom.vadLevelDbm) {
      const db = levelToDb(level);
      dom.vadLevelDbm.textContent = db <= -60 ? "-∞ dB" : `${db.toFixed(0)} dB`;
    }
  }

  if (_pendingTranscribeLevel !== null) {
    const level = _pendingTranscribeLevel;
    _pendingTranscribeLevel = null;
    if (dom.transcribeMeterFill) {
      dom.transcribeMeterFill.style.width = `${Math.round(level * 100)}%`;
    }
  }

  if (_pendingTranscribeDb !== null) {
    const value = _pendingTranscribeDb;
    _pendingTranscribeDb = null;
    if (dom.transcribeMeterDb) {
      dom.transcribeMeterDb.textContent = `${value.toFixed(1)} dB`;
    }
  }
}

function scheduleMeterFlush(): void {
  if (_meterRafId === null) {
    _meterRafId = requestAnimationFrame(flushMeterUpdates);
  }
}

let _ollamaRenderFrame: number | null = null;

function scheduleOllamaRender(): void {
  if (_ollamaRenderFrame !== null) return;
  _ollamaRenderFrame = requestAnimationFrame(() => {
    _ollamaRenderFrame = null;
    renderAIFallbackSettingsUi();
    renderOllamaModelManager();
  });
}

let _modelRenderFrame: number | null = null;

function scheduleModelRender(): void {
  if (_modelRenderFrame !== null) return;
  _modelRenderFrame = requestAnimationFrame(() => {
    _modelRenderFrame = null;
    renderModels();
  });
}

function cancelPendingRenderFrames(): void {
  if (_meterRafId !== null) { cancelAnimationFrame(_meterRafId); _meterRafId = null; }
  if (_ollamaRenderFrame !== null) { cancelAnimationFrame(_ollamaRenderFrame); _ollamaRenderFrame = null; }
  if (_modelRenderFrame !== null) { cancelAnimationFrame(_modelRenderFrame); _modelRenderFrame = null; }
  _pendingAudioLevel = _pendingTranscribeLevel = _pendingTranscribeDb = null;
}

async function bootstrap() {
  traceFrontendInfo("bootstrap", "bootstrap start");
  // Clean up old listeners if re-bootstrapping to prevent memory leaks
  cleanupEventListeners();
  startFrontendHeartbeatWatchdog();
  // Reset the paste queue to prevent accumulation across re-bootstrap cycles
  pasteQueue = Promise.resolve();

  if (dom.bootstrapLabel) dom.bootstrapLabel.textContent = "Loading configuration…";
  traceFrontendInfo("bootstrap", "loading initial configuration");

  // Bootstrap watchdog: if the backend doesn't respond within 18 s, reload the window.
  // This handles the rare startup deadlock where a blocking IPC call never resolves.
  const BOOTSTRAP_TIMEOUT_MS = 18_000;
  let bootstrapWatchdogCleared = false;
  const bootstrapWatchdog = setTimeout(() => {
    if (bootstrapWatchdogCleared) return;
    traceFrontendError("bootstrap", "startup timed out after 18 s — reloading window");
    const reloaded = tryAutoReloadWithBudget("bootstrap_timeout");
    if (!reloaded) {
      if (dom.bootstrapLabel) {
        dom.bootstrapLabel.textContent = "Startup recovery paused. Please reopen Trispr Flow.";
      }
      dom.bootstrapOverlay?.setAttribute("hidden", "");
    }
  }, BOOTSTRAP_TIMEOUT_MS);

  // Phase 1: Load data from backend in parallel
  // Audio device enumeration is non-fatal: Bluetooth/driver issues can cause hangs or errors
  const [
    fetchedSettings,
    fetchedDevices,
    fetchedOutputDevices,
    fetchedVersion,
    fetchedStartupStatus,
    fetchedRuntimeDiagnostics,
  ] = await Promise.all([
    invoke<Settings>("get_settings"),
    invoke<AudioDevice[]>("list_audio_devices").catch((): AudioDevice[] => []),
    invoke<AudioDevice[]>("list_output_devices").catch((): AudioDevice[] => []),
    getVersion().catch(() => null),
    invoke<StartupStatus>("get_startup_status"),
    invoke<RuntimeDiagnostics>("get_runtime_diagnostics").catch(() => null),
  ]);

  bootstrapWatchdogCleared = true;
  clearTimeout(bootstrapWatchdog);
  traceFrontendInfo("bootstrap", "initial configuration loaded", {
    audioDevices: fetchedDevices.length,
    outputDevices: fetchedOutputDevices.length,
    startupInteractive: fetchedStartupStatus?.interactive ?? null,
    ollamaStarting: fetchedStartupStatus?.ollama_starting ?? null,
  });
  setSettings(fetchedSettings);
  setDevices(fetchedDevices);
  setOutputDevices(fetchedOutputDevices);
  setHistory([]);
  setTranscribeHistory([]);
  setModels([]);
  setStartupStatus(fetchedStartupStatus);
  setRuntimeDiagnostics(fetchedRuntimeDiagnostics);
  if (dom.appVersion && fetchedVersion) {
    dom.appVersion.textContent = `v${fetchedVersion}`;
  }

  if (dom.bootstrapLabel) dom.bootstrapLabel.textContent = "Wiring events…";
  traceFrontendInfo("bootstrap", "wiring events");

  // Phase 2: Wire event handlers FIRST so UI is always interactive
  wireEvents();
  initMainTab();
  initPanelState();
  initConversationView();
  initUnifiedTooltips();
  initHistoryDelegation();
  initExportDialog();
  initArchiveBrowser();
  initExpertMode();
  initModulesHub();
  initGddFlow();
  initWorkflowAgentConsole();
  initOnboardingWizard();
  initPipelineStatus();
  syncVoiceOutputConsoleState();

  if (dom.bootstrapLabel) dom.bootstrapLabel.textContent = "Rendering interface…";
  traceFrontendInfo("bootstrap", "rendering primary interface");

  // Phase 3: Render UI synchronously — UI becomes interactive here
  try {
    renderDevices();
    renderOutputDevices();
    renderSettings();
    renderHero();
    applyStartupReadinessUi();
    setCaptureStatus("idle");
    setTranscribeStatus("idle");
    setRefiningActive(false);
    renderRefinementInspector();
    renderModels();
    refreshModulesHub();
  } catch (renderError) {
    console.error("Non-fatal render error during bootstrap:", renderError);
    traceFrontendError("bootstrap.render", "non-fatal render error", {
      error: renderError instanceof Error ? renderError.message : String(renderError),
    });
  }

  // Remove loading overlay — UI is now ready for interaction
  dom.bootstrapOverlay?.setAttribute("hidden", "");
  traceFrontendInfo("bootstrap", "bootstrap overlay hidden");

  // Defer history render to next animation frame so the overlay removal is painted first
  scheduleHistoryRender();

  // Phase 3b: Heavy background checks — run async without blocking the UI
  void (async () => {
    try {
      traceFrontendInfo("bootstrap.background", "background init start");
      const [fetchedHistory, fetchedTranscribeHistory, fetchedModels] = await Promise.all([
        invoke<HistoryEntry[]>("get_history").catch((): HistoryEntry[] => []),
        invoke<HistoryEntry[]>("get_transcribe_history").catch((): HistoryEntry[] => []),
        invoke<ModelInfo[]>("list_models").catch((): ModelInfo[] => []),
      ]);
      traceFrontendInfo("bootstrap.background", "heavy data loaded", {
        history: fetchedHistory.length,
        transcribeHistory: fetchedTranscribeHistory.length,
        models: fetchedModels.length,
      });
      setHistory(fetchedHistory);
      setTranscribeHistory(fetchedTranscribeHistory);
      restoreRefinementInspector(fetchedHistory.concat(fetchedTranscribeHistory));
      setModels(fetchedModels);
      scheduleHistoryRender();
      renderModels();

      await refreshModelsDir();
      traceFrontendInfo("bootstrap.background", "models dir refreshed");
      renderAIFallbackSettingsUi();
      renderOllamaModelManager();

      // Ollama initialization is EVENT-DRIVEN: we wait for the Rust backend
      // to signal `ollama:runtime-health { ok: true }` before refreshing
      // models or persisting settings.  A 30 s safety-net timer fires the
      // init anyway in case the event never arrives (Ollama disabled, etc.).
      // This replaces the old "ping immediately at startup" pattern that
      // caused timeout-storms and IPC freezes.
      if (isRefinementEnabled() && settings?.ai_fallback?.provider === "ollama") {
        const OLLAMA_FALLBACK_MS = 30_000;
        let ollamaInitDone = false;

        const runDeferredOllamaInit = async (trigger: string) => {
          if (ollamaInitDone) return;
          ollamaInitDone = true;
          try {
            traceFrontendInfo("bootstrap.deferred-ollama", `init triggered by: ${trigger}`);
            await refreshOllamaRuntimeState({ force: true });
            if (getOllamaRuntimeCardState().healthy) {
              traceFrontendInfo("bootstrap.deferred-ollama", "ollama healthy; refreshing models");
              await refreshOllamaInstalledModels();
            }
            renderAIFallbackSettingsUi();
            renderOllamaModelManager();
            await autoStartLocalRuntimeIfNeeded("bootstrap");
          } catch (err) {
            console.error("Deferred Ollama init failed (non-fatal):", err);
          } finally {
            traceFrontendInfo("bootstrap.deferred-ollama", "deferred ollama init finished");
            renderAIFallbackSettingsUi();
            renderOllamaModelManager();
          }
        };

        // Primary trigger: Rust signals Ollama is reachable
        const unlistenHealth = await listen<OllamaRuntimeHealth>("ollama:runtime-health", (event) => {
          if (event.payload?.ok && !ollamaInitDone) {
            void runDeferredOllamaInit("ollama:runtime-health (ok=true)");
          }
        });

        // Safety net: if the event never fires, init after 30 s anyway
        const fallbackTimer = setTimeout(() => {
          if (!ollamaInitDone) {
            void runDeferredOllamaInit(`fallback timer (${OLLAMA_FALLBACK_MS}ms)`);
          }
        }, OLLAMA_FALLBACK_MS);

        // Also kick off autostart which will eventually emit the health event
        void autoStartLocalRuntimeIfNeeded("bootstrap").catch(() => {});

        traceFrontendInfo("bootstrap.background", "ollama init deferred — waiting for runtime-health event");

        // Cleanup when no longer needed (both paths converge here)
        void (async () => {
          // Wait until init completes (either path)
          await new Promise<void>((resolve) => {
            const check = setInterval(() => {
              if (ollamaInitDone) { clearInterval(check); resolve(); }
            }, 500);
          });
          clearTimeout(fallbackTimer);
          unlistenHealth();
        })();
      }
    } catch (bgError) {
      console.error("Non-fatal background init error:", bgError);
      traceFrontendError("bootstrap.background", "background init failed", {
        error: bgError instanceof Error ? bgError.message : String(bgError),
      });
    }
  })();

  // Hoist before Promise.all so it's accessible to history listener callbacks
  function makeHistoryUpdateHandler(setter: (entries: HistoryEntry[]) => void) {
    return async (event: { payload: HistoryEntry[] }) => {
      setter(event.payload ?? []);
      scheduleHistoryRender();
      // Prune orphaned refinement snapshots from localStorage
      const allIds = new Set([...history, ...transcribeHistory].map((e) => e.id));
      pruneOrphanedSnapshots(allIds);
      // Live dump to file for crash recovery
      dumpHistoryToFile().catch(() => {});
    };
  }

  // Register all event listeners in parallel — avoids 10-20s sequential IPC overhead
  const _newListeners = await Promise.all([
    listen<Settings>("settings-changed", (event) => {
      setSettings(event.payload ?? null);
      const settingsSnapshot = settings;
      reconcileMainTabVisibility();
      if (
        !isRefinementEnabled()
        || settingsSnapshot?.ai_fallback?.provider !== "ollama"
        || settingsSnapshot?.ai_fallback?.execution_mode !== "local_primary"
      ) {
        resetTrackedRefinementJobs();
      }
      scheduleSettingsRender();
      renderHero();
      renderModels();
      refreshModulesHub();
      syncWorkflowAgentConsoleState();
      void refreshModelsDir().catch((e) => console.error("refreshModelsDir failed:", e));
      if (isRefinementEnabled() && settings?.ai_fallback?.provider === "ollama") {
        if (OLLAMA_SETTINGS_CHANGED_POLICY.refreshInstalledModels) {
          void refreshOllamaInstalledModels();
        }
        if (OLLAMA_SETTINGS_CHANGED_POLICY.refreshRuntimeState) {
          if (!isRuntimeBackgroundPollingActive()) {
            void refreshOllamaRuntimeState();
          }
        }
      }
      if (OLLAMA_SETTINGS_CHANGED_POLICY.renderManager) {
        renderOllamaModelManager();
      }
    }),
    listen<string>("capture:state", (event) => {
      const state = event.payload as TranscriptionStatus;
      setCaptureStatus(state ?? "idle");
    }),
    listen<string>("transcribe:state", (event) => {
      const state = event.payload as TranscriptionStatus;
      setTranscribeStatus(state ?? "idle");
    }),
    listen<TranscriptionGpuActivityEvent>("transcription:gpu-activity", (event) => {
      setGpuActivity(event.payload);
    }),
    listen<number>("transcribe:level", (event) => {
      _pendingTranscribeLevel = Math.max(0, Math.min(1, event.payload ?? 0));
      scheduleMeterFlush();
    }),
    listen<number>("transcribe:db", (event) => {
      _pendingTranscribeDb = Math.max(-60, Math.min(0, event.payload ?? -60));
      scheduleMeterFlush();
    }),
    listen<HistoryEntry[]>("history:updated", makeHistoryUpdateHandler(setHistory)),
    listen<HistoryEntry[]>("transcribe:history-updated", makeHistoryUpdateHandler(setTranscribeHistory)),
    listen("module:state-changed", () => {
      reconcileMainTabVisibility();
      refreshModulesHub();
    }),
    listen<{
      provider?: string;
      preferred_provider?: string;
      fallback_provider?: string;
      context?: string;
      error: string;
    }>("tts:speech-error", (event) => {
      const preferred = event.payload.preferred_provider || event.payload.provider || "unknown";
      const fallback = event.payload.fallback_provider || "none";
      showToast({
        type: "error",
        title: "Voice output failed",
        message: `${event.payload.error} (preferred: ${preferred}, fallback: ${fallback})`,
      });
    }),
    listen<{
      provider_used: string;
      preferred_provider?: string;
      fallback_provider?: string;
      used_fallback?: boolean;
      primary_error?: string;
      context?: string;
    }>("tts:speech-finished", (event) => {
      if (!event.payload.used_fallback || event.payload.context !== "manual_test") return;
      const preferred = event.payload.preferred_provider || "unknown";
      const used = event.payload.provider_used || "unknown";
      const primaryError = event.payload.primary_error ? ` (${event.payload.primary_error})` : "";
      showToast({
        type: "warning",
        title: "Voice output fallback used",
        message: `${preferred} failed${primaryError}. Switched to ${used}.`,
      });
    }),
    // Re-check Ollama health when a timeout or connection error occurs during
    // refinement — avoids requiring a full app restart to recover the status.
    listen("ai_fallback:health_degraded", async () => {
      await refreshOllamaRuntimeState({ force: true });
      renderAIFallbackSettingsUi();
    }),
    listen<TranscriptionResultEvent>("transcription:result", (event) => {
      const payload = event.payload;
      handlePipelineTranscriptionResult(payload);
      handleTranscriptionResultForInspector(payload);
      scheduleHistoryRender();
      if (dom.statusMessage) dom.statusMessage.textContent = "";

      const jobId = typeof payload?.job_id === "string" ? payload.job_id.trim() : "";
      const pasteDeferred = Boolean(payload?.paste_deferred && jobId);
      if (!pasteDeferred) {
        queueTranscriptPaste(payload.text, `raw:${jobId || "unknown"}`);
        return;
      }

      const completedOutcome = deferredPasteOutcomes.get(jobId);
      if (completedOutcome === "refined") {
        const refinedText = deferredRefinedTextByJobId.get(jobId);
        queueTranscriptPaste(refinedText && refinedText.trim() ? refinedText : payload.text, `late_result_refined:${jobId}`);
        return;
      }
      if (completedOutcome === "failed" || completedOutcome === "timed_out") {
        queueTranscriptPaste(payload.text, `late_result_fallback:${jobId}`);
        return;
      }

      const timeoutMs = Math.max(1, Number(payload.paste_timeout_ms ?? 10_000));
      const existing = pendingDeferredPasteJobs.get(jobId);
      if (existing) {
        window.clearTimeout(existing.timeoutHandle);
      }
      const timeoutHandle = window.setTimeout(() => {
        handleDeferredPasteTimeout(jobId);
      }, timeoutMs);
      pendingDeferredPasteJobs.set(jobId, {
        rawText: payload.text,
        timeoutHandle,
      });
    }),
    listen<TranscriptionRawResultEvent>("transcription:raw-result", (event) => {
      void handleWorkflowAgentRawResult(event.payload);
    }),
    ...(["agent:command-detected", "agent:plan-ready", "agent:execution-progress",
      "agent:execution-finished", "agent:execution-failed"] as const).map((name) =>
      listen(name, (event) => {
        appendWorkflowAgentLog(`Event ${name} -> ${JSON.stringify(event.payload)}`);
      })
    ),
    listen<AssistantStateChangedEvent>("assistant:state-changed", (event) => {
      if (!event.payload) return;
      handleAssistantStateChanged(event.payload);
      appendWorkflowAgentLog(
        `Event assistant:state-changed -> state=${event.payload.state}, reason=${event.payload.reason}`
      );
    }),
    listen<AssistantPlanReadyEvent>("assistant:plan-ready", (event) => {
      if (!event.payload) return;
      appendWorkflowAgentLog(
        `Event assistant:plan-ready -> intent=${event.payload.plan.intent}, session=${event.payload.plan.session_id}`
      );
    }),
    listen<AssistantIntentDetectedEvent>("assistant:intent-detected", (event) => {
      if (!event.payload) return;
      handleAssistantIntentDetected(event.payload);
      appendWorkflowAgentLog(
        `Event assistant:intent-detected -> intent=${event.payload.parse.intent}, confidence=${event.payload.parse.confidence.toFixed(2)}`
      );
    }),
    listen<AssistantAwaitingConfirmationEvent>("assistant:awaiting-confirmation", (event) => {
      if (!event.payload) return;
      handleAssistantAwaitingConfirmation(event.payload);
      appendWorkflowAgentLog(
        `Event assistant:awaiting-confirmation -> timeout=${event.payload.confirm_timeout_sec}s`
      );
    }),
    listen<AssistantConfirmationExpiredEvent>("assistant:confirmation-expired", (event) => {
      if (!event.payload) return;
      handleAssistantConfirmationExpired(event.payload);
      appendWorkflowAgentLog("Event assistant:confirmation-expired");
    }),
    listen<AssistantActionResultEvent>("assistant:action-result", (event) => {
      if (!event.payload) return;
      handleWorkflowAssistantActionResult(event.payload);
      appendWorkflowAgentLog(
        `Event assistant:action-result -> status=${event.payload.result.status}, reason=${event.payload.reason}`
      );
    }),
    listen<TranscriptionRefinementStartedEvent>("transcription:refinement-started", (event) => {
      markRefinementJobStarted(event.payload?.job_id || "");
      handlePipelineRefinementStarted(event.payload);
      handleRefinementStartedForInspector(event.payload);
      scheduleHistoryRender();
    }),
    // AI Fallback: refined transcript available — log silently (original already shown).
    listen<TranscriptionRefinedEvent>("transcription:refined", (event) => {
      handleRefinementSuccessForInspector(event.payload);
      scheduleHistoryRender();
      const { refined, model, execution_time_ms, job_id: jobId } = event.payload;
      markRefinementJobFinished(jobId);
      const priorOutcome = deferredPasteOutcomes.get(jobId);
      const pending = settleDeferredPasteJob(jobId);
      if (pending) {
        rememberDeferredPasteOutcome(jobId, "refined", refined);
        handlePipelineRefined(event.payload);
        queueTranscriptPaste(refined, `refined:${jobId}`);
      } else if (priorOutcome === "timed_out") {
        rememberDeferredPasteOutcome(jobId, "refined", refined);
        handlePipelineRefinementTimeout(jobId);
        console.debug(`[AI] Late refinement received after timeout (${jobId}); history updated only.`);
      } else {
        rememberDeferredPasteOutcome(jobId, "refined", refined);
        handlePipelineRefined(event.payload);
      }
      console.debug(`[AI] Refinement done (${model}, ${execution_time_ms}ms):`, refined);
    }),
    // AI Fallback: refinement failed — log, no disruption to user workflow.
    listen<TranscriptionRefinementFailedEvent>("transcription:refinement-failed", (event) => {
      const payload = event.payload;
      markRefinementJobFinished(payload.job_id);
      const priorOutcome = deferredPasteOutcomes.get(payload.job_id);
      if (priorOutcome === "timed_out") {
        handlePipelineRefinementTimeout(payload.job_id);
      } else {
        handlePipelineRefinementFailed(payload);
      }
      handleRefinementFailureForInspector(payload);
      scheduleHistoryRender();
      const pending = settleDeferredPasteJob(payload.job_id);
      if (pending) {
        rememberDeferredPasteOutcome(payload.job_id, "failed");
        queueTranscriptPaste(pending.rawText, `fallback_failed:${payload.job_id}`);
      } else {
        rememberDeferredPasteOutcome(payload.job_id, "failed");
      }
      console.warn(
        `[AI] Refinement failed (${payload.source}, ${payload.reason || "unknown"}):`,
        payload.error
      );
    }),
    listen<TranscriptionRefinementActivityEvent>("transcription:refinement-activity", (event) => {
      const payload = event.payload;
      backendRefinementActiveCount = Math.max(0, Number(payload?.active_count ?? 0));
      if (backendRefinementActiveCount === 0) {
        if (trackedRefinementJobs.size > 0) {
          reconcilePipelineRefinementIdle(payload?.reason || "activity_zero");
        }
        trackedRefinementJobs.clear();
      }
      syncRefiningIndicator();
      if (payload?.reason === "watchdog_reset" || payload?.reason === "forced_reset") {
        resetTrackedRefinementJobs();
        handlePipelineRefinementReset(payload.reason);
        markAllPendingAsFailed(payload.reason);
        scheduleHistoryRender();
        showToast({
          type: "warning",
          title: "Refinement reset",
          message: "A stuck refinement job was reset automatically.",
          duration: 4200,
        });
      }
    }),
    listen<DownloadProgress>("model:download-progress", (event) => {
      modelProgress.set(event.payload.id, event.payload);
      const updatedModels = models.map((model) =>
        model.id === event.payload.id ? { ...model, downloading: true } : model
      );
      setModels(updatedModels);
      scheduleModelRender();
    }),
    listen<DownloadComplete>("model:download-complete", async (event) => {
      modelProgress.delete(event.payload.id);
      await refreshModels();
      await refreshStartupStatusFromBackend();
    }),
    listen<DownloadError>("model:download-error", async (event) => {
      console.error("model download error", event.payload.error);
      modelProgress.delete(event.payload.id);
      await refreshModels();
    }),
    listen<QuantizeProgress>("model:quantize-progress", (event) => {
      const payload = event.payload;
      if (!payload?.file_name) return;
      quantizeProgress.set(payload.file_name, payload);
      scheduleModelRender();
    }),
    // Ollama pull progress events
    listen<OllamaPullProgress>("ollama:pull-progress", (event) => {
      ollamaPullProgress.set(event.payload.model, event.payload);
      scheduleOllamaRender();
    }),
    listen<OllamaPullComplete>("ollama:pull-complete", async (event) => {
      clearActiveOllamaPull(event.payload.model);
      ollamaPullProgress.delete(event.payload.model);
      showToast({
        type: "success",
        title: "Model downloaded",
        message: `${event.payload.model} is installed locally.`,
        actionLabel: "Activate now",
        onAction: async () => {
          await activateOllamaModel(event.payload.model);
          renderAIFallbackSettingsUi();
        },
      });
      await refreshOllamaRuntimeState({ force: true });
      if (getOllamaRuntimeCardState().healthy) {
        await refreshOllamaInstalledModels();
      }
      renderOllamaModelManager();
      renderAIFallbackSettingsUi();
    }),
    listen<OllamaPullError>("ollama:pull-error", (event) => {
      clearActiveOllamaPull(event.payload.model);
      ollamaPullProgress.delete(event.payload.model);
      showToast({
        type: "error",
        title: "Download Failed",
        message: `${event.payload.model}: ${event.payload.error}`,
      });
      renderOllamaModelManager();
      renderAIFallbackSettingsUi();
    }),
    listen<OllamaRuntimeInstallProgress>("ollama:runtime-install-progress", (event) => {
      setOllamaRuntimeInstallProgress(event.payload);
      scheduleOllamaRender();
    }),
    listen<OllamaRuntimeInstallComplete>("ollama:runtime-install-complete", async (event) => {
      setOllamaRuntimeInstallComplete(event.payload);
      await refreshOllamaRuntimeState({ force: true });
      renderAIFallbackSettingsUi();
    }),
    listen<OllamaRuntimeInstallError>("ollama:runtime-install-error", (event) => {
      setOllamaRuntimeInstallError(event.payload);
      renderAIFallbackSettingsUi();
    }),
    listen<OllamaRuntimeHealth>("ollama:runtime-health", (event) => {
      setOllamaRuntimeHealth(event.payload);
      renderAIFallbackSettingsUi();
    }),
    listen<RuntimeDiagnostics>("runtime:diagnostics", (event) => {
      setRuntimeDiagnostics(event.payload ?? null);
      renderHero();
      renderAIFallbackSettingsUi();
    }),
    listen<boolean>("app:instance-activated", () => {
      traceFrontendInfo("app.single_instance", "existing instance activated from second launch");
      showToast({
        type: "info",
        title: "Already running",
        message: "Trispr Flow was already running and has been brought to the foreground.",
        duration: 3500,
      });
    }),
    listen<StabilityDegradedEvent>("app:stability-degraded", (event) => {
      const payload = event.payload;
      if (!payload) return;
      const title = payload.restart_blocked ? "Stability degraded" : "Stability recovery";
      showToast({
        type: payload.restart_blocked ? "warning" : "info",
        title,
        message: payload.reason,
        duration: 7000,
      });
      traceFrontendWarn("frontend.watchdog", "stability event", payload);
    }),
    listen<OverlayHealthEvent>("overlay:health", (event) => {
      const payload = event.payload ?? null;
      if (!payload) {
        setOverlayHealth(null);
        renderSettings();
        return;
      }
      if (payload.status === "recovering") {
        setOverlayHealth(payload);
        renderSettings();
        return;
      }
      if (payload.status === "recovered") {
        setOverlayHealth(null);
        renderSettings();
        dismissToast(overlayHealthToastId);
        overlayHealthToastId = null;
        return;
      }
      setOverlayHealth(payload);
      renderSettings();
      dismissToast(overlayHealthToastId);
      overlayHealthToastId = showToast({
        type: "warning",
        title: "Overlay degraded",
        message: payload.reason,
        duration: 5200,
      });
    }),
    listen<StartupStatus>("startup:status", (event) => {
      applyStartupStatus(event.payload ?? null);
    }),
    listen<string>("transcription:error", (event) => {
      console.error("transcription error", event.payload);
      resetTrackedRefinementJobs();
      handlePipelineRefinementReset("transcription_error");
      setCaptureStatus("idle");
      if (dom.statusMessage) dom.statusMessage.textContent = event.payload;

      // Show toast for transcription errors
      showToast({
        type: "error",
        title: "Transcription Failed",
        message: event.payload,
        duration: 7000,
      });
    }),
    listen<TranscribeBacklogStatus>("transcribe:backlog-expanded", (event) => {
      const payload = event.payload;
      if (!payload) return;
      dismissToast(backlogWarningToastId);
      backlogWarningToastId = null;
      showToast({
        type: "success",
        title: "Output Backlog Expanded",
        message: `New capacity: ${payload.capacity_chunks} chunks (${payload.percent_used}% used).`,
        duration: 5000,
      });
    }),
    listen<TranscribeBacklogStatus>("transcribe:backlog-warning", (event) => {
      const payload = event.payload;
      if (!payload) return;

      dismissToast(backlogWarningToastId);

      const droppedSuffix = payload.dropped_chunks > 0 ? ` Dropped chunks: ${payload.dropped_chunks}.` : "";
      backlogWarningToastId = showToast({
        type: "warning",
        title: "Output Backlog Near Capacity",
        message: `Queue at ${payload.percent_used}% (${payload.queued_chunks}/${payload.capacity_chunks} chunks). Auto-expand is scheduled.${droppedSuffix}`,
        duration: 0,
        actionLabel: "Expand now",
        actionDismiss: false,
        onAction: async () => {
          try {
            await invoke<TranscribeBacklogStatus>("expand_transcribe_backlog");
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            showToast({
              type: "error",
              title: "Backlog Expansion Failed",
              message,
              duration: 7000,
            });
          }
        },
      });
    }),
    // Listen for app-wide errors from backend
    listen<ErrorEvent>("app:error", (event) => {
      showErrorToast(event.payload.error, event.payload.context);
    }),
    // Listen for audio cues (beep on recording start/stop)
    listen<string>("audio:cue", (event) => {
      const type = event.payload as AudioCueType;
      if (settings?.audio_cues) {
        playAudioCue(type);
      }
    }),
    listen<number>("audio:level", (event) => {
      _pendingAudioLevel = Math.max(0, Math.min(1, event.payload ?? 0));
      scheduleMeterFlush();
    }),
    // Listen for dynamic sustain threshold updates from backend
    listen<number>("vad:dynamic-threshold", (event) => {
      setDynamicSustainThreshold(event.payload ?? 0.01);
      updateThresholdMarkers();
    }),
  ]);
  eventUnlisteners.push(..._newListeners);

  // Initialize live transcript dump for crash recovery
  initLiveDump();
}

async function checkModelOnStartup() {
  try {
    // Use the already-loaded settings from bootstrap to avoid redundant backend call
    if (!settings) {
      console.warn("Settings not available during model check");
      return;
    }
    const modelAvailable = await invoke<boolean>("check_model_available", {
      modelId: settings.model,
    });

    if (!modelAvailable) {
      showToast({
        type: "error",
        title: "Speech Model Missing",
        message: `The selected model "${settings.model}" is not installed. Please download it from the Model Manager panel to enable speech recognition.`,
        duration: 15000, // 15 seconds
      });

      // Scroll to model manager panel and expand it after a short delay
      setTimeout(() => {
        if (dom.modelPanel) {
          // Expand the panel if it's collapsed
          const collapseButton = dom.modelPanel.querySelector('[data-panel-collapse="model"]') as HTMLButtonElement;
          if (collapseButton && isPanelCollapsed("model")) {
            setPanelCollapsed("model", false);
          }

          // Scroll to the panel
          dom.modelPanel.scrollIntoView({ behavior: "smooth", block: "start" });

          // Highlight the panel briefly
          dom.modelPanel.classList.add('panel-highlight');
          setTimeout(() => {
            dom.modelPanel!.classList.remove('panel-highlight');
          }, 2000);
        }
      }, 1000);
    }
  } catch (error) {
    console.error("Failed to check model availability:", error);
  }
}

async function checkDependencyPreflightOnStartup() {
  try {
    const report = await invoke<DependencyPreflightReport>("get_dependency_preflight_status");
    if (!report || report.overall_status === "ok") {
      return;
    }

    if (report.blocking_count > 0) {
      const blocking = report.items.filter((item) => item.status === "error");
      const first = blocking[0];
      showToast({
        type: "error",
        title: "Missing Runtime Dependencies",
        message: first
          ? `${first.message}${first.hint ? ` ${first.hint}` : ""}`
          : `${report.blocking_count} blocking dependency issue(s) detected.`,
        duration: 12000,
      });
    }

    if (report.warning_count > 0) {
      const runtimeCard = getOllamaRuntimeCardState();
      const warning = report.items.find((item) => {
        if (item.status !== "warning") return false;
        // Startup race: managed Ollama can still be warming up while preflight runs.
        // Avoid noisy warning toasts in that short window.
        if (
          item.id === "ollama_runtime" &&
          isRefinementEnabled() &&
          settings?.ai_fallback?.provider === "ollama" &&
          (runtimeCard.busy || runtimeCard.backgroundStarting)
        ) {
          return false;
        }
        return true;
      });
      if (!warning) {
        return;
      }

      if (
        warning.id === "ollama_runtime" &&
        isRefinementEnabled() &&
        settings?.ai_fallback?.provider === "ollama"
      ) {
        const runtimeLoading =
          runtimeCard.busy
          || runtimeCard.backgroundStarting
          || Boolean(startupStatus?.ollama_starting);
        const runtimeReady = runtimeCard.healthy || Boolean(startupStatus?.ollama_ready);

        if (runtimeReady) {
          return;
        }

        if (runtimeLoading) {
          showToast({
            type: "info",
            icon: "🔵",
            title: "Lokale AI startet",
            message: "Ollama lädt im Hintergrund. Das kann etwas dauern, bis der Dienst verfügbar ist.",
            duration: 9000,
          });
          return;
        }
      }

      showToast({
        type: "warning",
        title: "Dependency Warnings",
        message: warning
          ? `${warning.message}${warning.hint ? ` ${warning.hint}` : ""}`
          : `${report.warning_count} dependency warning(s) detected.`,
        duration: 9000,
      });
    }
  } catch (error) {
    console.error("Failed to run dependency preflight check:", error);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  installGlobalFrontendErrorLogging();
  traceFrontendInfo("bootstrap", "DOMContentLoaded");
  bootstrap()
    .then(() => {
      traceFrontendInfo("bootstrap", "bootstrap resolved");
      initWindowStatePersistence();
      // Start GPU VRAM monitoring (update every 2 seconds)
      startGpuVramMonitoring();
      return Promise.all([checkModelOnStartup(), checkDependencyPreflightOnStartup()]);
    })
    .catch((error) => {
      console.error("bootstrap failed", error);
      traceFrontendError("bootstrap", "bootstrap failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      if (dom.bootstrapLabel) {
        dom.bootstrapLabel.textContent = "Startup failed. Open DevTools/logs and retry.";
      }
      dom.bootstrapOverlay?.setAttribute("hidden", "");
      showToast({
        type: "error",
        title: "Startup failed",
        message: error instanceof Error ? error.message : String(error),
        duration: 9000,
      });
    });
});

// GPU VRAM monitoring and updates
function startGpuVramMonitoring() {
  const updateVramDisplay = async () => {
    try {
      const vramUsage = await invoke<string>("get_gpu_vram_usage");
      if (dom.gpuVramLabel && vramUsage) {
        dom.gpuVramLabel.textContent = vramUsage;
      }
    } catch (error) {
      // Silently ignore nvidia-smi errors (GPU might not be present)
    }
  };

  // Initial update
  void updateVramDisplay();

  // Then update every 2 seconds
  setInterval(() => {
    void updateVramDisplay();
  }, 2000);
}

// Cleanup event listeners on window unload to prevent memory leaks
window.addEventListener("beforeunload", () => {
  cleanupEventListeners();
});
