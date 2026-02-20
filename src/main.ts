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
  TranscribeBacklogStatus
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
  modelProgress
} from "./state";
import * as dom from "./dom-refs";
import { renderSettings } from "./settings";
import { renderDevices, renderOutputDevices } from "./devices";
import { renderHero, setCaptureStatus, setTranscribeStatus, updateThresholdMarkers } from "./ui-state";
import { renderHistory, setHistoryTab, initSpeakerLabelEditing } from "./history";
import { initPanelState, isPanelCollapsed, setPanelCollapsed } from "./panels";
import { renderModels, refreshModels, refreshModelsDir } from "./models";
import { wireEvents, initMainTab } from "./event-listeners";
import { dismissToast, showToast, showErrorToast } from "./toast";
import { playAudioCue } from "./audio-cues";
import { levelToDb, thresholdToPercent } from "./ui-helpers";
import { dumpHistoryToFile, initLiveDump } from "./live-dump";
import { initChaptersUI, refreshChapters } from "./chapters";

// Track event listeners for cleanup to prevent memory leaks
let eventUnlisteners: Array<() => void> = [];
let backlogWarningToastId: string | null = null;

function cleanupEventListeners() {
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

  const fetchedModels = await invoke<ModelInfo[]>("list_models");
  setModels(fetchedModels);

  renderDevices();
  renderOutputDevices();
  renderSettings();
  renderHero();
  setCaptureStatus("idle");
  setTranscribeStatus("idle");
  renderHistory();
  renderModels();
  await refreshModelsDir();
  wireEvents();
  initMainTab();
  initPanelState();
  initConversationView();
  initChaptersUI();
  initSpeakerLabelEditing();

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
    renderSettings();
    renderHero();
    renderModels();
    refreshModelsDir();
  }));

  eventUnlisteners.push(await listen<string>("capture:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setCaptureStatus(state ?? "idle");
  }));

  eventUnlisteners.push(await listen<string>("transcribe:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setTranscribeStatus(state ?? "idle");
  }));

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

  eventUnlisteners.push(await listen<{ text: string; source: string }>("transcription:result", () => {
    if (dom.statusMessage) dom.statusMessage.textContent = "";
  }));

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

  eventUnlisteners.push(await listen<string>("transcription:error", (event) => {
    console.error("transcription error", event.payload);
    setCaptureStatus("idle");
    if (dom.statusMessage) dom.statusMessage.textContent = `Error: ${event.payload}`;

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
