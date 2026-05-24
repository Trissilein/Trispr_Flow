// Global app chrome wiring (R2 slice 6).

import { invoke } from "@tauri-apps/api/core";
import { isAssistantCoreAvailable, settings } from "../state";
import * as dom from "../dom-refs";
import { renderSettings } from "../settings";
import { renderAIFallbackSettingsUi } from "../settings/ai-refinement.settings";
import { persistSettings } from "../settings-persist";
import { renderHero, updateDeviceLineClamp } from "../ui-state";
import { isPanelId, togglePanel } from "../panels";
import { initHotkeyStatusListener, setupHotkeyRecorder } from "../hotkeys";
import {
  getOllamaRuntimeCardState,
  refreshOllamaInstalledModels,
  refreshOllamaRuntimeState,
  renderOllamaModelManager,
} from "../ollama-models";
import { DEFAULT_ACCENT_COLOR, applyAccentColor } from "../utils";
import { syncWorkflowAgentConsoleState } from "../workflow-agent-console";
import { renderTaskCaptureTab } from "../task-capture-config";

type MainTab =
  | "transcription"
  | "settings"
  | "ai-refinement"
  | "voice-output"
  | "video"
  | "agent"
  | "modules"
  | "task-capture";

let aiRefinementTabRefreshInFlight: Promise<void> | null = null;
let _resizeWired = false;

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

function taskCaptureTabAvailable(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("task_capture") ?? false;
}

function syncMainTabAvailability(): void {
  const aiAvailable = aiRefinementTabAvailable();
  const voiceAvailable = voiceOutputTabAvailable();
  const videoAvailable = videoTabAvailable();
  const agentAvailable = agentTabAvailable();
  const taskCaptureAvailable = taskCaptureTabAvailable();
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
  if (dom.tabBtnTaskCapture) {
    dom.tabBtnTaskCapture.hidden = !taskCaptureAvailable;
    dom.tabBtnTaskCapture.setAttribute("aria-hidden", (!taskCaptureAvailable).toString());
    if (taskCaptureAvailable) {
      dom.tabBtnTaskCapture.removeAttribute("tabindex");
    } else {
      dom.tabBtnTaskCapture.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabTaskCapture) {
    dom.tabTaskCapture.hidden = !taskCaptureAvailable;
    if (!taskCaptureAvailable) {
      dom.tabTaskCapture.classList.remove("active");
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
  if (dom.tabBtnTaskCapture?.classList.contains("active")) return "task-capture";
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
  if (!videoTabAvailable() && activeTab === "video") {
    switchMainTab("transcription");
    return;
  }
  if (!agentTabAvailable() && activeTab === "agent") {
    switchMainTab("transcription");
    return;
  }
  if (!taskCaptureTabAvailable() && activeTab === "task-capture") {
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
  if (resolvedTab === "task-capture" && !taskCaptureTabAvailable()) {
    resolvedTab = "transcription";
  }

  const isTranscription = resolvedTab === "transcription";
  const isSettings = resolvedTab === "settings";
  const isAiRefinement = resolvedTab === "ai-refinement";
  const isVoiceOutput = resolvedTab === "voice-output";
  const isVideo = resolvedTab === "video";
  const isAgent = resolvedTab === "agent";
  const isModules = resolvedTab === "modules";
  const isTaskCapture = resolvedTab === "task-capture";

  dom.tabBtnTranscription?.classList.toggle("active", isTranscription);
  dom.tabBtnSettings?.classList.toggle("active", isSettings);
  dom.tabBtnAiRefinement?.classList.toggle("active", isAiRefinement);
  dom.tabBtnVoiceOutput?.classList.toggle("active", isVoiceOutput);
  dom.tabBtnVideo?.classList.toggle("active", isVideo);
  dom.tabBtnAgent?.classList.toggle("active", isAgent);
  dom.tabBtnModules?.classList.toggle("active", isModules);
  dom.tabBtnTaskCapture?.classList.toggle("active", isTaskCapture);

  dom.tabBtnTranscription?.setAttribute("aria-selected", isTranscription.toString());
  dom.tabBtnSettings?.setAttribute("aria-selected", isSettings.toString());
  dom.tabBtnAiRefinement?.setAttribute("aria-selected", isAiRefinement.toString());
  dom.tabBtnVoiceOutput?.setAttribute("aria-selected", isVoiceOutput.toString());
  dom.tabBtnVideo?.setAttribute("aria-selected", isVideo.toString());
  dom.tabBtnAgent?.setAttribute("aria-selected", isAgent.toString());
  dom.tabBtnModules?.setAttribute("aria-selected", isModules.toString());
  dom.tabBtnTaskCapture?.setAttribute("aria-selected", isTaskCapture.toString());

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
  if (dom.tabTaskCapture) {
    dom.tabTaskCapture.style.removeProperty("display");
    dom.tabTaskCapture.classList.toggle("active", isTaskCapture);
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

  if (isTaskCapture) {
    void renderTaskCaptureTab();
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
      savedTab === "modules" ||
      savedTab === "task-capture"
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

export function wireAppChrome(): void {
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
  dom.tabBtnTaskCapture?.addEventListener("click", () => {
    switchMainTab("task-capture");
  });

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

  if (!_resizeWired) {
    const _onResize = () => updateDeviceLineClamp();
    window.addEventListener("resize", _onResize);
    _resizeWired = true;
  }

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
}