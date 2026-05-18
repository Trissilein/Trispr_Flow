// DOM event listeners setup

import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "./types";
import {
  isAssistantCoreAvailable,
  settings,
} from "./state";
import * as dom from "./dom-refs";
import {
  persistSettings,
  renderSettings,
  renderAIFallbackSettingsUi,
  ensureContinuousDumpDefaults,
} from "./settings";
import { renderHero, updateDeviceLineClamp, updateThresholdMarkers } from "./ui-state";
import { refreshModels, refreshModelsDir } from "./models";
import { syncHistoryAliasesIntoSettings } from "./history-preferences";
import { isPanelId, togglePanel } from "./panels";
import { setupHotkeyRecorder, initHotkeyStatusListener } from "./hotkeys";
import { updateRangeAria } from "./accessibility";
import { dbToLevel, VAD_DB_FLOOR } from "./ui-helpers";
import {
  getOllamaRuntimeCardState,
  refreshOllamaInstalledModels,
  refreshOllamaRuntimeState,
  renderOllamaModelManager,
} from "./ollama-models";
import { applyAccentColor, DEFAULT_ACCENT_COLOR } from "./utils";
import { syncWorkflowAgentConsoleState } from "./workflow-agent-console";
import { wireHistory } from "./wiring/history.wire";
import { wireOverlay } from "./wiring/overlay.wire";
import { wireTranscription } from "./wiring/transcription.wire";
import { wireAiRefinement } from "./wiring/ai-refinement.wire";
import { wireVoiceOutput } from "./wiring/voice-output.wire";
import { onChangePersist, scheduleSettingsRender } from "./wiring/wire-helpers";

// Cleanup registry for window-level listeners added by wireEvents()
const _windowCleanups: Array<() => void> = [];
export function cleanupWindowListeners(): void {
  _windowCleanups.forEach((fn) => fn());
  _windowCleanups.length = 0;
}





// Main tab switching
type MainTab =
  | "transcription"
  | "settings"
  | "ai-refinement"
  | "voice-output"
  | "video"
  | "agent"
  | "modules";
let aiRefinementTabRefreshInFlight: Promise<void> | null = null;

function aiRefinementTabAvailable(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("ai_refinement") ?? false;
}

function voiceOutputTabAvailable(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("output_voice_tts") ?? false;
}

function videoTabAvailable(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("output_video_generation") ?? false;
}

function agentTabAvailable(): boolean {
  return isAssistantCoreAvailable();
}

function syncMainTabAvailability(): void {
  const aiAvailable = aiRefinementTabAvailable();
  const voiceAvailable = voiceOutputTabAvailable();
  const videoAvailable = videoTabAvailable();
  const agentAvailable = agentTabAvailable();
  if (dom.tabBtnAiRefinement) {
    dom.tabBtnAiRefinement.hidden = !aiAvailable;
    dom.tabBtnAiRefinement.setAttribute("aria-hidden", (!aiAvailable).toString());
    if (aiAvailable) {
      dom.tabBtnAiRefinement.removeAttribute("tabindex");
    } else {
      dom.tabBtnAiRefinement.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabAiRefinement) {
    dom.tabAiRefinement.hidden = !aiAvailable;
    if (!aiAvailable) {
      dom.tabAiRefinement.classList.remove("active");
    }
  }
  if (dom.tabBtnVoiceOutput) {
    dom.tabBtnVoiceOutput.hidden = !voiceAvailable;
    dom.tabBtnVoiceOutput.setAttribute("aria-hidden", (!voiceAvailable).toString());
    if (voiceAvailable) {
      dom.tabBtnVoiceOutput.removeAttribute("tabindex");
    } else {
      dom.tabBtnVoiceOutput.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabVoiceOutput) {
    dom.tabVoiceOutput.hidden = !voiceAvailable;
    if (!voiceAvailable) {
      dom.tabVoiceOutput.classList.remove("active");
    }
  }
  if (dom.tabBtnVideo) {
    dom.tabBtnVideo.hidden = !videoAvailable;
    dom.tabBtnVideo.setAttribute("aria-hidden", (!videoAvailable).toString());
    if (videoAvailable) {
      dom.tabBtnVideo.removeAttribute("tabindex");
    } else {
      dom.tabBtnVideo.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabVideo) {
    dom.tabVideo.hidden = !videoAvailable;
    if (!videoAvailable) {
      dom.tabVideo.classList.remove("active");
    }
  }
  if (dom.tabBtnAgent) {
    dom.tabBtnAgent.hidden = !agentAvailable;
    dom.tabBtnAgent.setAttribute("aria-hidden", (!agentAvailable).toString());
    if (agentAvailable) {
      dom.tabBtnAgent.removeAttribute("tabindex");
    } else {
      dom.tabBtnAgent.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabAgent) {
    dom.tabAgent.hidden = !agentAvailable;
    if (!agentAvailable) {
      dom.tabAgent.classList.remove("active");
    }
  }
}

function getActiveMainTabFromDom(): MainTab {
  if (dom.tabBtnSettings?.classList.contains("active")) return "settings";
  if (dom.tabBtnAiRefinement?.classList.contains("active")) return "ai-refinement";
  if (dom.tabBtnVoiceOutput?.classList.contains("active")) return "voice-output";
  if (dom.tabBtnVideo?.classList.contains("active")) return "video";
  if (dom.tabBtnAgent?.classList.contains("active")) return "agent";
  if (dom.tabBtnModules?.classList.contains("active")) return "modules";
  return "transcription";
}

export function reconcileMainTabVisibility(): void {
  syncMainTabAvailability();
  const activeTab = getActiveMainTabFromDom();
  if (!aiRefinementTabAvailable() && activeTab === "ai-refinement") {
    switchMainTab("transcription");
    return;
  }
  if (!voiceOutputTabAvailable() && activeTab === "voice-output") {
    switchMainTab("transcription");
    return;
  }
  if (!agentTabAvailable() && activeTab === "agent") {
    switchMainTab("transcription");
  }
}

async function refreshAiRefinementTabState(): Promise<void> {
  if (aiRefinementTabRefreshInFlight) {
    await aiRefinementTabRefreshInFlight;
    return;
  }

  const refreshTask = (async () => {
    await refreshOllamaRuntimeState({ force: true });
    if (getOllamaRuntimeCardState().healthy) {
      await refreshOllamaInstalledModels();
    }
    renderAIFallbackSettingsUi();
    renderOllamaModelManager();
  })();

  aiRefinementTabRefreshInFlight = refreshTask;
  try {
    await refreshTask;
  } finally {
    if (aiRefinementTabRefreshInFlight === refreshTask) {
      aiRefinementTabRefreshInFlight = null;
    }
  }
}

export function openMainTab(tab: MainTab) {
  switchMainTab(tab);
}

function switchMainTab(tab: MainTab) {
  syncMainTabAvailability();
  let resolvedTab: MainTab = tab;
  if (resolvedTab === "ai-refinement" && !aiRefinementTabAvailable()) {
    resolvedTab = "transcription";
  }
  if (resolvedTab === "voice-output" && !voiceOutputTabAvailable()) {
    resolvedTab = "transcription";
  }
  if (resolvedTab === "video" && !videoTabAvailable()) {
    resolvedTab = "transcription";
  }
  if (resolvedTab === "agent" && !agentTabAvailable()) {
    resolvedTab = "transcription";
  }

  const isTranscription = resolvedTab === "transcription";
  const isSettings = resolvedTab === "settings";
  const isAiRefinement = resolvedTab === "ai-refinement";
  const isVoiceOutput = resolvedTab === "voice-output";
  const isVideo = resolvedTab === "video";
  const isAgent = resolvedTab === "agent";
  const isModules = resolvedTab === "modules";

  dom.tabBtnTranscription?.classList.toggle("active", isTranscription);
  dom.tabBtnSettings?.classList.toggle("active", isSettings);
  dom.tabBtnAiRefinement?.classList.toggle("active", isAiRefinement);
  dom.tabBtnVoiceOutput?.classList.toggle("active", isVoiceOutput);
  dom.tabBtnVideo?.classList.toggle("active", isVideo);
  dom.tabBtnAgent?.classList.toggle("active", isAgent);
  dom.tabBtnModules?.classList.toggle("active", isModules);

  dom.tabBtnTranscription?.setAttribute("aria-selected", isTranscription.toString());
  dom.tabBtnSettings?.setAttribute("aria-selected", isSettings.toString());
  dom.tabBtnAiRefinement?.setAttribute("aria-selected", isAiRefinement.toString());
  dom.tabBtnVoiceOutput?.setAttribute("aria-selected", isVoiceOutput.toString());
  dom.tabBtnVideo?.setAttribute("aria-selected", isVideo.toString());
  dom.tabBtnAgent?.setAttribute("aria-selected", isAgent.toString());
  dom.tabBtnModules?.setAttribute("aria-selected", isModules.toString());

  // Update tab content visibility — clear any inline display styles first
  if (dom.tabTranscription) {
    dom.tabTranscription.style.removeProperty("display");
    dom.tabTranscription.classList.toggle("active", isTranscription);
  }
  if (dom.tabSettings) {
    dom.tabSettings.style.removeProperty("display");
    dom.tabSettings.classList.toggle("active", isSettings);
  }
  if (dom.tabAiRefinement) {
    dom.tabAiRefinement.style.removeProperty("display");
    dom.tabAiRefinement.classList.toggle("active", isAiRefinement);
  }
  if (dom.tabVoiceOutput) {
    dom.tabVoiceOutput.style.removeProperty("display");
    dom.tabVoiceOutput.classList.toggle("active", isVoiceOutput);
  }
  if (dom.tabVideo) {
    dom.tabVideo.style.removeProperty("display");
    dom.tabVideo.classList.toggle("active", isVideo);
  }
  if (dom.tabAgent) {
    dom.tabAgent.style.removeProperty("display");
    dom.tabAgent.classList.toggle("active", isAgent);
  }
  if (dom.tabModules) {
    dom.tabModules.style.removeProperty("display");
    dom.tabModules.classList.toggle("active", isModules);
  }

  // Persist to localStorage
  try {
    localStorage.setItem("trispr-active-tab", resolvedTab);
  } catch (error) {
    console.error("Failed to persist active tab", error);
  }

  if (isAiRefinement) {
    void (async () => {
      try {
        await refreshAiRefinementTabState();
      } catch (error) {
        console.warn("Failed to refresh Ollama runtime on tab switch:", error);
      }
    })();
  }
}

// Initialize tab state from localStorage
export function initMainTab() {
  syncMainTabAvailability();
  try {
    const savedTab = localStorage.getItem("trispr-active-tab") as MainTab | null;
    if (
      savedTab === "settings" ||
      savedTab === "transcription" ||
      savedTab === "ai-refinement" ||
      savedTab === "voice-output" ||
      savedTab === "video" ||
      savedTab === "agent" ||
      savedTab === "modules"
    ) {
      switchMainTab(savedTab);
    } else {
      // Default to transcription tab
      switchMainTab("transcription");
    }
  } catch (error) {
    console.error("Failed to load active tab", error);
    switchMainTab("transcription");
  }
}

export function wireEvents() {
  ensureContinuousDumpDefaults();
  if (syncHistoryAliasesIntoSettings()) {
    void persistSettings();
  }

  // Main tab switching
  dom.tabBtnTranscription?.addEventListener("click", () => {
    switchMainTab("transcription");
  });

  dom.tabBtnSettings?.addEventListener("click", () => {
    switchMainTab("settings");
  });

  dom.tabBtnAiRefinement?.addEventListener("click", () => {
    switchMainTab("ai-refinement");
  });
  dom.tabBtnVoiceOutput?.addEventListener("click", () => {
    switchMainTab("voice-output");
  });
  dom.tabBtnVideo?.addEventListener("click", () => {
    switchMainTab("video");
  });
  dom.tabBtnAgent?.addEventListener("click", () => {
    switchMainTab("agent");
  });
  dom.tabBtnModules?.addEventListener("click", () => {
    switchMainTab("modules");
  });

  // Transcription + Whisper Backend (R2 slice 4).
  wireTranscription();

  // AI Refinement + Post-Processing + Language controls + Local Runtime (R2 slice 5).
  wireAiRefinement();

  const setProductMode = async (nextMode: "transcribe" | "assistant") => {
    if (!settings) return;
    if (nextMode === "assistant" && !isAssistantCoreAvailable()) {
      settings.product_mode = "transcribe";
      renderSettings();
      renderHero();
      return;
    }
    settings.product_mode = nextMode;
    renderSettings();
    syncWorkflowAgentConsoleState();
    await persistSettings();
    renderHero();
  };

  dom.productModeTranscribeBtn?.addEventListener("click", async () => {
    await setProductMode("transcribe");
  });

  dom.productModeAssistantBtn?.addEventListener("click", async () => {
    await setProductMode("assistant");
  });

  const setGlobalOnlineMode = async (onlineEnabled: boolean) => {
    if (!settings?.workflow_agent) return;
    settings.workflow_agent.online_enabled = onlineEnabled;
    renderSettings();
    syncWorkflowAgentConsoleState();
    await persistSettings();
    renderHero();
  };

  dom.globalOnlineOfflineBtn?.addEventListener("click", async () => {
    await setGlobalOnlineMode(false);
  });

  dom.globalOnlineEnabledBtn?.addEventListener("click", async () => {
    await setGlobalOnlineMode(true);
  });

  dom.modelSourceSelect?.addEventListener("change", async () => {
    if (!settings || !dom.modelSourceSelect) return;
    settings.model_source = dom.modelSourceSelect.value as Settings["model_source"];
    await persistSettings();
    scheduleSettingsRender();
    await refreshModels();
  });

  dom.modelCustomUrl?.addEventListener("change", async () => {
    if (!settings || !dom.modelCustomUrl) return;
    settings.model_custom_url = dom.modelCustomUrl.value.trim();
    await persistSettings();
  });

  dom.modelRefresh?.addEventListener("click", async () => {
    if (!settings) return;
    if (dom.modelCustomUrl) {
      settings.model_custom_url = dom.modelCustomUrl.value.trim();
    }
    await persistSettings();
    if (settings.model_source === "default") {
      try {
        await invoke("clear_hidden_external_models");
      } catch (error) {
        console.error("clear_hidden_external_models failed", error);
      }
    }
    await refreshModels();
  });

  dom.modelStorageBrowse?.addEventListener("click", async () => {
    if (!settings) return;
    const dir = await invoke<string | null>("pick_model_dir");
    if (!dir) return;
    settings.model_storage_dir = dir;
    await persistSettings();
    await refreshModelsDir();
    await refreshModels();
  });

  dom.modelStorageReset?.addEventListener("click", async () => {
    if (!settings) return;
    settings.model_storage_dir = "";
    if (dom.modelStoragePath) {
      dom.modelStoragePath.value = "";
    }
    await persistSettings();
    await refreshModelsDir();
    await refreshModels();
  });

  dom.modelStoragePath?.addEventListener("change", async () => {
    if (!settings || !dom.modelStoragePath) return;
    settings.model_storage_dir = dom.modelStoragePath.value.trim();
    await persistSettings();
    await refreshModelsDir();
    await refreshModels();
  });

  document.querySelectorAll<HTMLButtonElement>(".panel-collapse-btn").forEach((button) => {
    const panelId = button.dataset.panelCollapse;
    if (!panelId || !isPanelId(panelId)) return;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      togglePanel(panelId);
    });
  });

  document.querySelectorAll<HTMLElement>(".panel-header").forEach((header) => {
    const panel = header.closest<HTMLElement>(".panel");
    const panelId = panel?.dataset.panel;
    if (!panelId || !isPanelId(panelId)) return;
    header.addEventListener("click", (event) => {
      const target = event.target as HTMLElement | null;
      if (!target) return;
      if (target.closest(".panel-actions")) return;
      if (target.closest("button, input, select, textarea, a, label")) return;
      togglePanel(panelId);
    });
  });

  wireHistory();

  dom.analyseButton?.addEventListener("click", () => {
    switchMainTab("modules");
    window.dispatchEvent(new CustomEvent("modules:focus", { detail: "analysis" }));
  });
  dom.openModulesBtn?.addEventListener("click", () => {
    switchMainTab("modules");
  });

  dom.openRecordingsBtn?.addEventListener("click", () => {
    void invoke("open_recordings_directory");
  });

  // Hotkey recording functionality + registration status listener
  initHotkeyStatusListener();
  setupHotkeyRecorder("productModeToggle", dom.productModeHotkey, dom.productModeHotkeyRecord, dom.productModeHotkeyStatus);
  setupHotkeyRecorder("ttsStop", dom.ttsStopHotkey, dom.ttsStopHotkeyRecord, dom.ttsStopHotkeyStatus);

  const _onResize = () => updateDeviceLineClamp();
  window.addEventListener("resize", _onResize);
  _windowCleanups.push(() => window.removeEventListener("resize", _onResize));

  dom.micGain?.addEventListener("input", () => {
    if (!settings || !dom.micGain) return;
    const value = Number(dom.micGain.value);
    settings.mic_input_gain_db = Math.max(-30, Math.min(30, value));
    if (dom.micGainValue) {
      const gain = Math.round(settings.mic_input_gain_db);
      dom.micGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
    }
    updateRangeAria("mic-gain", value);
  });

  onChangePersist(dom.micGain);

  dom.vadThreshold?.addEventListener("input", () => {
    if (!settings || !dom.vadThreshold) return;
    const rawDb = Number(dom.vadThreshold.value);
    const clampedDb = Math.max(VAD_DB_FLOOR, Math.min(0, rawDb));
    const threshold = Math.min(1, Math.max(0, dbToLevel(clampedDb)));

    // Update the start threshold (main threshold)
    settings.vad_threshold_start = threshold;
    // Keep legacy field in sync
    settings.vad_threshold = threshold;

    if (dom.vadThresholdValue) {
      dom.vadThresholdValue.textContent = `${Math.round(clampedDb)} dB`;
    }

    updateRangeAria("vad-threshold", clampedDb);
    // Update threshold markers
    updateThresholdMarkers();
  });

  onChangePersist(dom.vadThreshold);

  dom.vadSilence?.addEventListener("input", () => {
    if (!settings || !dom.vadSilence) return;
    const value = Math.max(200, Math.min(4000, Number(dom.vadSilence.value)));
    settings.vad_silence_ms = value;
    if (dom.vadSilenceValue) {
      dom.vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
    }
    updateRangeAria("vad-silence", value);
  });

  onChangePersist(dom.vadSilence);

  wireOverlay();

  // Accent color picker — live preview while dragging
  dom.accentColor?.addEventListener("input", () => {
    if (!settings || !dom.accentColor) return;
    settings.accent_color = dom.accentColor.value;
    applyAccentColor(dom.accentColor.value);
  });

  // Accent color picker — persist when picker closes
  dom.accentColor?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  // Reset accent color to default teal
  dom.accentColorReset?.addEventListener("click", async () => {
    if (!settings) return;
    settings.accent_color = DEFAULT_ACCENT_COLOR;
    if (dom.accentColor) dom.accentColor.value = DEFAULT_ACCENT_COLOR;
    applyAccentColor(DEFAULT_ACCENT_COLOR);
    await persistSettings();
  });

  // Voice Output (TTS) — provider chain, devices, voices, sliders, Piper, Qwen3, test button.
  // See src/wiring/voice-output.wire.ts (R2 slice 3).
  wireVoiceOutput();
}
