// Main entry point - Bootstrap and backend event listeners

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Settings, HistoryEntry, AudioDevice, ModelInfo, DownloadProgress, DownloadComplete, DownloadError, ErrorEvent } from "./types";
import {
  setSettings,
  setHistory,
  setTranscribeHistory,
  setDevices,
  setOutputDevices,
  setModels,
  setDynamicSustainThreshold,
  modelProgress
} from "./state";
import * as dom from "./dom-refs";
import { renderSettings } from "./settings";
import { renderDevices, renderOutputDevices } from "./devices";
import { renderHero, setCaptureStatus, setTranscribeStatus, updateThresholdMarkers } from "./ui-state";
import { renderHistory, initPanelState, setHistoryTab } from "./history";
import { renderModels, refreshModels, refreshModelsDir } from "./models";
import { wireEvents } from "./event-listeners";
import { showToast, showErrorToast } from "./toast";
import { playAudioCue } from "./audio-cues";
import { levelToDb, thresholdToPercent } from "./ui-helpers";

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
  initPanelState();
  initConversationView();

  await listen<Settings>("settings-changed", (event) => {
    setSettings(event.payload ?? null);
    renderSettings();
    renderHero();
    renderModels();
    refreshModelsDir();
  });

  await listen<string>("capture:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setCaptureStatus(state ?? "idle");
  });

  await listen<string>("transcribe:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setTranscribeStatus(state ?? "idle");
  });

  await listen<number>("transcribe:level", (event) => {
    if (!dom.transcribeMeterFill) return;
    const level = Math.max(0, Math.min(1, event.payload ?? 0));
    dom.transcribeMeterFill.style.width = `${Math.round(level * 100)}%`;
  });

  await listen<number>("transcribe:db", (event) => {
    if (!dom.transcribeMeterDb) return;
    const value = event.payload ?? -60;
    const clamped = Math.max(-60, Math.min(0, value));
    dom.transcribeMeterDb.textContent = `${clamped.toFixed(1)} dB`;
  });

  await listen<HistoryEntry[]>("history:updated", (event) => {
    setHistory(event.payload ?? []);
    renderHistory();
  });

  await listen<HistoryEntry[]>("transcribe:history-updated", (event) => {
    setTranscribeHistory(event.payload ?? []);
    renderHistory();
  });

  await listen<{ text: string; source: string }>("transcription:result", () => {
    if (dom.statusMessage) dom.statusMessage.textContent = "";
  });

  await listen<DownloadProgress>("model:download-progress", (event) => {
    modelProgress.set(event.payload.id, event.payload);
    import("./state").then(({ models, setModels }) => {
      const updatedModels = models.map((model) =>
        model.id === event.payload.id ? { ...model, downloading: true } : model
      );
      setModels(updatedModels);
      renderModels();
    });
  });

  await listen<DownloadComplete>("model:download-complete", async (event) => {
    modelProgress.delete(event.payload.id);
    await refreshModels();
  });

  await listen<DownloadError>("model:download-error", async (event) => {
    console.error("model download error", event.payload.error);
    modelProgress.delete(event.payload.id);
    await refreshModels();
  });

  await listen<string>("transcription:error", (event) => {
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
  });

  // Listen for app-wide errors from backend
  await listen<ErrorEvent>("app:error", (event) => {
    showErrorToast(event.payload.error, event.payload.context);
  });

  // Listen for audio cues (beep on recording start/stop)
  await listen<string>("audio:cue", (event) => {
    const type = event.payload as "start" | "stop";
    import("./state").then(({ settings }) => {
      if (settings?.audio_cues) {
        playAudioCue(type);
      }
    });
  });

  await listen<number>("audio:level", (event) => {
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
  });

  // Listen for dynamic sustain threshold updates from backend
  await listen<number>("vad:dynamic-threshold", (event) => {
    setDynamicSustainThreshold(event.payload ?? 0.01);
    updateThresholdMarkers();
  });
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap().catch((error) => {
    console.error("bootstrap failed", error);
  });
});
