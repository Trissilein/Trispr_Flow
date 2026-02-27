// Main entry point - Bootstrap and backend event listeners

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { initWindowStatePersistence } from "./window-state";

import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  DownloadComplete,
  DownloadError,
  ErrorEvent,
  TranscribeBacklogStatus,
  OllamaPullProgress,
  OllamaPullComplete,
  OllamaPullError,
  OllamaRuntimeInstallProgress,
  OllamaRuntimeInstallComplete,
  OllamaRuntimeInstallError,
  OllamaRuntimeHealth,
  TranscriptionRefinedEvent,
  TranscriptionRefinementFailedEvent,
  TranscriptionRefinementStartedEvent,
  TranscriptionRefinementActivityEvent,
  TranscriptionGpuActivityEvent,
  TranscriptionResultEvent,
} from "./types";
import {
  settings,
  setSettings,
  setHistory,
  setTranscribeHistory,
  setDevices,
  setOutputDevices,
  models,
  setModels,
  setDynamicSustainThreshold,
  modelProgress,
  ollamaPullProgress,
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
import { renderHistory, setHistoryTab } from "./history";
import { initPanelState, isPanelCollapsed, setPanelCollapsed } from "./panels";
import { renderModels, refreshModels, refreshModelsDir } from "./models";
import { wireEvents, initMainTab } from "./event-listeners";
import { initUnifiedTooltips } from "./custom-tooltips";
import { dismissToast, showToast, showErrorToast } from "./toast";
import { playAudioCue } from "./audio-cues";
import { levelToDb, thresholdToPercent } from "./ui-helpers";
import { dumpHistoryToFile, initLiveDump } from "./live-dump";
import { initChaptersUI, refreshChapters } from "./chapters";
import {
  handleRefinementFailureForInspector,
  handleRefinementStartedForInspector,
  handleRefinementSuccessForInspector,
  handleTranscriptionResultForInspector,
  markAllPendingAsFailed,
  restoreRefinementInspector,
  renderRefinementInspector,
} from "./refinement-inspector";
import {
  handlePipelineRefined,
  handlePipelineRefinementFailed,
  handlePipelineRefinementReset,
  handlePipelineRefinementStarted,
  handlePipelineRefinementTimeout,
  handlePipelineTranscriptionResult,
} from "./refinement-pipeline-graph";
import {
  autoStartLocalRuntimeIfNeeded,
  clearActiveOllamaPull,
  getOllamaRuntimeCardState,
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
let pasteQueue: Promise<void> = Promise.resolve();

type PendingDeferredPasteJob = {
  rawText: string;
  timeoutHandle: number;
};

type DeferredPasteOutcome = "refined" | "failed" | "timed_out";

const pendingDeferredPasteJobs = new Map<string, PendingDeferredPasteJob>();
const deferredPasteOutcomes = new Map<string, DeferredPasteOutcome>();
const deferredRefinedTextByJobId = new Map<string, string>();
const MAX_DEFERRED_PASTE_OUTCOMES = 500;

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
  rememberDeferredPasteOutcome(jobId, "timed_out");
  handlePipelineRefinementTimeout(jobId);
  reportRuntimeMetric("refinement_timeout");
  queueTranscriptPaste(pending.rawText, `timeout:${jobId}`);
}

function cleanupEventListeners() {
  clearPendingDeferredPasteJobs();
  eventUnlisteners.forEach((unlisten) => unlisten());
  eventUnlisteners = [];
  dismissToast(backlogWarningToastId);
  backlogWarningToastId = null;
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

  window.addEventListener("trispr:view", (event) => {
    const detail = (event as CustomEvent<string>).detail;
    if (detail === "conversation") {
      applyConversationOnly();
    }
  });

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

async function bootstrap() {
  // Clean up old listeners if re-bootstrapping to prevent memory leaks
  cleanupEventListeners();

  // Phase 1: Load data from backend — any failure here is fatal
  const fetchedSettings = await invoke<Settings>("get_settings");
  setSettings(fetchedSettings);

  const fetchedDevices = await invoke<AudioDevice[]>("list_audio_devices");
  setDevices(fetchedDevices);

  const fetchedOutputDevices = await invoke<AudioDevice[]>("list_output_devices");
  setOutputDevices(fetchedOutputDevices);

  const fetchedHistory = await invoke<HistoryEntry[]>("get_history");
  setHistory(fetchedHistory);

  const fetchedTranscribeHistory = await invoke<HistoryEntry[]>("get_transcribe_history");
  setTranscribeHistory(fetchedTranscribeHistory);
  restoreRefinementInspector([...fetchedHistory, ...fetchedTranscribeHistory]);

  const fetchedModels = await invoke<ModelInfo[]>("list_models");
  setModels(fetchedModels);

  // Phase 2: Wire event handlers FIRST so UI is always interactive
  wireEvents();
  initMainTab();
  initPanelState();
  initConversationView();
  initChaptersUI();
  initUnifiedTooltips();

  // Phase 3: Render UI — failures here should not block interaction
  try {
    renderDevices();
    renderOutputDevices();
    renderSettings();
    renderHero();
    setCaptureStatus("idle");
    setTranscribeStatus("idle");
    setRefiningActive(false);
    renderHistory();
    renderRefinementInspector();
    renderModels();
    await refreshModelsDir();
    // Initialize Ollama model manager if provider is Ollama
    if (settings?.ai_fallback?.provider === "ollama") {
      await refreshOllamaRuntimeState({ force: true });
      if (getOllamaRuntimeCardState().healthy) {
        await refreshOllamaInstalledModels();
      }
    }
    void autoStartLocalRuntimeIfNeeded("bootstrap").finally(() => {
      renderAIFallbackSettingsUi();
      renderOllamaModelManager();
    });
    renderOllamaModelManager();
  } catch (renderError) {
    console.error("Non-fatal render error during bootstrap:", renderError);
  }

  // Display app version
  if (dom.appVersion) {
    try {
      const version = await getVersion();
      dom.appVersion.textContent = `v${version}`;
    } catch (error) {
      console.warn("Failed to get app version:", error);
    }
  }

  eventUnlisteners.push(await listen<Settings>("settings-changed", (event) => {
    setSettings(event.payload ?? null);
    if (!settings?.ai_fallback?.enabled) setRefiningActive(false);
    renderSettings();
    renderHero();
    renderModels();
    refreshModelsDir();
    if (settings?.ai_fallback?.provider === "ollama") {
      if (OLLAMA_SETTINGS_CHANGED_POLICY.refreshInstalledModels) {
        void refreshOllamaInstalledModels();
      }
      if (OLLAMA_SETTINGS_CHANGED_POLICY.refreshRuntimeState) {
        void refreshOllamaRuntimeState();
      }
    }
    if (OLLAMA_SETTINGS_CHANGED_POLICY.renderManager) {
      renderOllamaModelManager();
    }
  }));

  eventUnlisteners.push(await listen<string>("capture:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setCaptureStatus(state ?? "idle");
  }));

  eventUnlisteners.push(await listen<string>("transcribe:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setTranscribeStatus(state ?? "idle");
  }));

  eventUnlisteners.push(
    await listen<TranscriptionGpuActivityEvent>("transcription:gpu-activity", (event) => {
      setGpuActivity(event.payload);
    })
  );

  eventUnlisteners.push(await listen<number>("transcribe:level", (event) => {
    if (!dom.transcribeMeterFill) return;
    const level = Math.max(0, Math.min(1, event.payload ?? 0));
    dom.transcribeMeterFill.style.width = `${Math.round(level * 100)}%`;
  }));

  eventUnlisteners.push(await listen<number>("transcribe:db", (event) => {
    if (!dom.transcribeMeterDb) return;
    const value = event.payload ?? -60;
    const clamped = Math.max(-60, Math.min(0, value));
    dom.transcribeMeterDb.textContent = `${clamped.toFixed(1)} dB`;
  }));

  eventUnlisteners.push(await listen<HistoryEntry[]>("history:updated", async (event) => {
    setHistory(event.payload ?? []);
    renderHistory();
    refreshChapters();
    // Live dump to file for crash recovery
    dumpHistoryToFile().catch(() => {});
  }));

  eventUnlisteners.push(await listen<HistoryEntry[]>("transcribe:history-updated", async (event) => {
    setTranscribeHistory(event.payload ?? []);
    renderHistory();
    refreshChapters();
    // Live dump to file for crash recovery
    dumpHistoryToFile().catch(() => {});
  }));

  eventUnlisteners.push(await listen<TranscriptionResultEvent>("transcription:result", (event) => {
    const payload = event.payload;
    handlePipelineTranscriptionResult(payload);
    handleTranscriptionResultForInspector(payload);
    renderHistory();
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
  }));

  eventUnlisteners.push(
    await listen<TranscriptionRefinementStartedEvent>("transcription:refinement-started", (event) => {
      handlePipelineRefinementStarted(event.payload);
      handleRefinementStartedForInspector(event.payload);
      renderHistory();
    })
  );

  // AI Fallback: refined transcript available — log silently (original already shown).
  eventUnlisteners.push(await listen<TranscriptionRefinedEvent>("transcription:refined", (event) => {
    handleRefinementSuccessForInspector(event.payload);
    renderHistory();
    const { refined, model, execution_time_ms, job_id: jobId } = event.payload;
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
  }));

  // AI Fallback: refinement failed — log, no disruption to user workflow.
  eventUnlisteners.push(await listen<TranscriptionRefinementFailedEvent>("transcription:refinement-failed", (event) => {
    const payload = event.payload;
    const priorOutcome = deferredPasteOutcomes.get(payload.job_id);
    if (priorOutcome === "timed_out") {
      handlePipelineRefinementTimeout(payload.job_id);
    } else {
      handlePipelineRefinementFailed(payload);
    }
    handleRefinementFailureForInspector(payload);
    renderHistory();
    const pending = settleDeferredPasteJob(payload.job_id);
    if (pending) {
      rememberDeferredPasteOutcome(payload.job_id, "failed");
      queueTranscriptPaste(pending.rawText, `fallback_failed:${payload.job_id}`);
    } else {
      rememberDeferredPasteOutcome(payload.job_id, "failed");
    }
    console.warn(`[AI] Refinement failed (${payload.source}):`, payload.error);
  }));

  eventUnlisteners.push(
    await listen<TranscriptionRefinementActivityEvent>("transcription:refinement-activity", (event) => {
      const payload = event.payload;
      const activeCount = Number(payload?.active_count ?? 0);
      setRefiningActive(activeCount > 0);
      if (payload?.reason === "watchdog_reset" || payload?.reason === "forced_reset") {
        handlePipelineRefinementReset(payload.reason);
        markAllPendingAsFailed(payload.reason);
        renderHistory();
        showToast({
          type: "warning",
          title: "Refinement reset",
          message: "A stuck refinement job was reset automatically.",
          duration: 4200,
        });
      }
    })
  );

  eventUnlisteners.push(await listen<DownloadProgress>("model:download-progress", (event) => {
    modelProgress.set(event.payload.id, event.payload);
    const updatedModels = models.map((model) =>
      model.id === event.payload.id ? { ...model, downloading: true } : model
    );
    setModels(updatedModels);
    renderModels();
  }));

  eventUnlisteners.push(await listen<DownloadComplete>("model:download-complete", async (event) => {
    modelProgress.delete(event.payload.id);
    await refreshModels();
  }));

  eventUnlisteners.push(await listen<DownloadError>("model:download-error", async (event) => {
    console.error("model download error", event.payload.error);
    modelProgress.delete(event.payload.id);
    await refreshModels();
  }));

  // Ollama pull progress events
  eventUnlisteners.push(await listen<OllamaPullProgress>("ollama:pull-progress", (event) => {
    ollamaPullProgress.set(event.payload.model, event.payload);
    renderOllamaModelManager();
    renderAIFallbackSettingsUi();
  }));

  eventUnlisteners.push(await listen<OllamaPullComplete>("ollama:pull-complete", async (event) => {
    clearActiveOllamaPull(event.payload.model);
    ollamaPullProgress.delete(event.payload.model);
    showToast({
      type: "success",
      title: "Model Downloaded",
      message: `${event.payload.model} is ready to use.`,
    });
    await refreshOllamaRuntimeState({ force: true });
    if (getOllamaRuntimeCardState().healthy) {
      await refreshOllamaInstalledModels();
    }
    renderOllamaModelManager();
    renderAIFallbackSettingsUi();
  }));

  eventUnlisteners.push(await listen<OllamaPullError>("ollama:pull-error", (event) => {
    clearActiveOllamaPull(event.payload.model);
    ollamaPullProgress.delete(event.payload.model);
    showToast({
      type: "error",
      title: "Download Failed",
      message: `${event.payload.model}: ${event.payload.error}`,
    });
    renderOllamaModelManager();
    renderAIFallbackSettingsUi();
  }));

  eventUnlisteners.push(
    await listen<OllamaRuntimeInstallProgress>("ollama:runtime-install-progress", (event) => {
      setOllamaRuntimeInstallProgress(event.payload);
      renderAIFallbackSettingsUi();
    })
  );

  eventUnlisteners.push(
    await listen<OllamaRuntimeInstallComplete>("ollama:runtime-install-complete", async (event) => {
      setOllamaRuntimeInstallComplete(event.payload);
      await refreshOllamaRuntimeState({ force: true });
      renderAIFallbackSettingsUi();
    })
  );

  eventUnlisteners.push(
    await listen<OllamaRuntimeInstallError>("ollama:runtime-install-error", (event) => {
      setOllamaRuntimeInstallError(event.payload);
      renderAIFallbackSettingsUi();
    })
  );

  eventUnlisteners.push(await listen<OllamaRuntimeHealth>("ollama:runtime-health", (event) => {
    setOllamaRuntimeHealth(event.payload);
    renderAIFallbackSettingsUi();
  }));

  eventUnlisteners.push(await listen<string>("transcription:error", (event) => {
    console.error("transcription error", event.payload);
    setCaptureStatus("idle");
    if (dom.statusMessage) dom.statusMessage.textContent = event.payload;

    // Show toast for transcription errors
    showToast({
      type: "error",
      title: "Transcription Failed",
      message: event.payload,
      duration: 7000,
    });
  }));

  eventUnlisteners.push(await listen<TranscribeBacklogStatus>("transcribe:backlog-expanded", (event) => {
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
  }));

  eventUnlisteners.push(await listen<TranscribeBacklogStatus>("transcribe:backlog-warning", (event) => {
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
  }));

  // Listen for app-wide errors from backend
  eventUnlisteners.push(await listen<ErrorEvent>("app:error", (event) => {
    showErrorToast(event.payload.error, event.payload.context);
  }));

  // Listen for audio cues (beep on recording start/stop)
  eventUnlisteners.push(await listen<string>("audio:cue", (event) => {
    const type = event.payload as "start" | "stop";
    if (settings?.audio_cues) {
      playAudioCue(type);
    }
  }));

  eventUnlisteners.push(await listen<number>("audio:level", (event) => {
    if (!dom.vadMeterFill) return;
    const level = Math.max(0, Math.min(1, event.payload ?? 0));
    // Convert to dB scale for display (-60dB to 0dB)
    const db = levelToDb(level);
    const percent = thresholdToPercent(level);
    dom.vadMeterFill.style.width = `${percent}%`;

    // Update dBm display
    if (dom.vadLevelDbm) {
      if (db <= -60) {
        dom.vadLevelDbm.textContent = "-∞ dB";
      } else {
        dom.vadLevelDbm.textContent = `${db.toFixed(0)} dB`;
      }
    }
  }));

  // Listen for dynamic sustain threshold updates from backend
  eventUnlisteners.push(await listen<number>("vad:dynamic-threshold", (event) => {
    setDynamicSustainThreshold(event.payload ?? 0.01);
    updateThresholdMarkers();
  }));

  // Initialize live transcript dump for crash recovery
  initLiveDump();
}

async function checkModelOnStartup() {
  try {
    const settings = await invoke<Settings>("get_settings");
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
        const modelPanel = document.querySelector('[data-panel="model"]');
        if (modelPanel) {
          // Expand the panel if it's collapsed
          const collapseButton = modelPanel.querySelector('[data-panel-collapse="model"]') as HTMLButtonElement;
          if (collapseButton && isPanelCollapsed("model")) {
            setPanelCollapsed("model", false);
          }

          // Scroll to the panel
          modelPanel.scrollIntoView({ behavior: "smooth", block: "start" });

          // Highlight the panel briefly
          modelPanel.classList.add('panel-highlight');
          setTimeout(() => {
            modelPanel.classList.remove('panel-highlight');
          }, 2000);
        }
      }, 1000);
    }
  } catch (error) {
    console.error("Failed to check model availability:", error);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap()
    .then(() => {
      initWindowStatePersistence();
      return checkModelOnStartup();
    })
    .catch((error) => {
      console.error("bootstrap failed", error);
    });
});

// Cleanup event listeners on window unload to prevent memory leaks
window.addEventListener("beforeunload", () => {
  cleanupEventListeners();
});
