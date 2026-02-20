// DOM event listeners setup

import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  AIFallbackProvider,
  AnalysisToolStatus,
  Settings,
} from "./types";
import { settings } from "./state";
import * as dom from "./dom-refs";
import { persistSettings, updateOverlayStyleVisibility, applyOverlaySharedUi, updateTranscribeVadVisibility, updateTranscribeThreshold, renderAIFallbackSettingsUi, renderTopicKeywords } from "./settings";
import { renderSettings } from "./settings";
import { renderHero, updateDeviceLineClamp, updateThresholdMarkers } from "./ui-state";
import { refreshModels, refreshModelsDir } from "./models";
import { setHistoryTab, buildConversationHistory, buildConversationText, buildExportText, setSearchQuery, setTopicKeywords, DEFAULT_TOPICS } from "./history";
import { isPanelId, togglePanel } from "./panels";
import { setupHotkeyRecorder } from "./hotkeys";
import { updateRangeAria } from "./accessibility";
import { showToast } from "./toast";
import { dbToLevel, VAD_DB_FLOOR } from "./ui-helpers";
import { updateChaptersVisibility } from "./chapters";

// =====================================================================
// =====================================================================
// Voice Analysis launcher helpers
// =====================================================================

let analysisInProgress = false;
const ANALYSIS_GPU_WARNING_MESSAGE =
  "Voice Analysis runs in a separate app and may heavily load GPU/VRAM. Running Trispr and analysis in parallel can slow down your system.\n\nContinue?";

const AI_FALLBACK_PROVIDER_IDS: AIFallbackProvider[] = ["claude", "openai", "gemini"];

function normalizeAIFallbackProvider(provider?: string): AIFallbackProvider {
  if (provider && AI_FALLBACK_PROVIDER_IDS.includes(provider as AIFallbackProvider)) {
    return provider as AIFallbackProvider;
  }
  return "claude";
}

function getAIFallbackProviderSettings(provider: AIFallbackProvider) {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  return null;
}

function ensureAIFallbackSettingsDefaults() {
  if (!settings) return;
  if (!settings.ai_fallback) {
    settings.ai_fallback = {
      enabled: false,
      provider: "claude",
      model: "claude-3-5-sonnet-20241022",
      temperature: 0.3,
      max_tokens: 4000,
      custom_prompt_enabled: false,
      custom_prompt:
        "Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.",
      use_default_prompt: true,
    };
  }
  if (!settings.providers) {
    settings.providers = {
      claude: {
        api_key_stored: false,
        available_models: ["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022", "claude-3-opus-20240229"],
        preferred_model: "claude-3-5-sonnet-20241022",
      },
      openai: {
        api_key_stored: false,
        available_models: ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini"],
        preferred_model: "gpt-4o-mini",
      },
      gemini: {
        api_key_stored: false,
        available_models: ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
        preferred_model: "gemini-2.0-flash",
      },
    };
  }
}

function ensureContinuousDumpDefaults() {
  if (!settings) return;
  settings.auto_save_mic_audio ??= false;
  settings.continuous_dump_enabled ??= true;
  settings.continuous_dump_profile ??= "balanced";
  settings.continuous_soft_flush_ms ??= 10000;
  settings.continuous_silence_flush_ms ??= 1200;
  settings.continuous_hard_cut_ms ??= 45000;
  settings.continuous_min_chunk_ms ??= 1000;
  settings.continuous_pre_roll_ms ??= 300;
  settings.continuous_post_roll_ms ??= 200;
  settings.continuous_idle_keepalive_ms ??= 60000;
  settings.continuous_mic_override_enabled ??= false;
  settings.continuous_mic_soft_flush_ms ??= settings.continuous_soft_flush_ms;
  settings.continuous_mic_silence_flush_ms ??= settings.continuous_silence_flush_ms;
  settings.continuous_mic_hard_cut_ms ??= settings.continuous_hard_cut_ms;
  settings.continuous_system_override_enabled ??= false;
  settings.continuous_system_soft_flush_ms ??= settings.continuous_soft_flush_ms;
  settings.continuous_system_silence_flush_ms ??= settings.continuous_silence_flush_ms;
  settings.continuous_system_hard_cut_ms ??= settings.continuous_hard_cut_ms;
}

function applyContinuousProfile(profile: "balanced" | "low_latency" | "high_quality") {
  if (!settings) return;
  if (profile === "low_latency") {
    settings.continuous_soft_flush_ms = 8000;
    settings.continuous_silence_flush_ms = 900;
    settings.continuous_hard_cut_ms = 30000;
    settings.continuous_min_chunk_ms = 800;
    settings.continuous_pre_roll_ms = 200;
    settings.continuous_post_roll_ms = 150;
    settings.continuous_idle_keepalive_ms = 45000;
  } else if (profile === "high_quality") {
    settings.continuous_soft_flush_ms = 12000;
    settings.continuous_silence_flush_ms = 1600;
    settings.continuous_hard_cut_ms = 60000;
    settings.continuous_min_chunk_ms = 1500;
    settings.continuous_pre_roll_ms = 450;
    settings.continuous_post_roll_ms = 300;
    settings.continuous_idle_keepalive_ms = 75000;
  } else {
    settings.continuous_soft_flush_ms = 10000;
    settings.continuous_silence_flush_ms = 1200;
    settings.continuous_hard_cut_ms = 45000;
    settings.continuous_min_chunk_ms = 1000;
    settings.continuous_pre_roll_ms = 300;
    settings.continuous_post_roll_ms = 200;
    settings.continuous_idle_keepalive_ms = 60000;
  }

  if (!settings.continuous_system_override_enabled) {
    settings.continuous_system_soft_flush_ms = settings.continuous_soft_flush_ms;
    settings.continuous_system_silence_flush_ms = settings.continuous_silence_flush_ms;
    settings.continuous_system_hard_cut_ms = settings.continuous_hard_cut_ms;
  }
  if (!settings.continuous_mic_override_enabled) {
    settings.continuous_mic_soft_flush_ms = settings.continuous_soft_flush_ms;
    settings.continuous_mic_silence_flush_ms = settings.continuous_silence_flush_ms;
    settings.continuous_mic_hard_cut_ms = settings.continuous_hard_cut_ms;
  }
  const systemSoftFlush = settings.continuous_system_soft_flush_ms ?? settings.continuous_soft_flush_ms ?? 10000;
  const systemSilenceFlush =
    settings.continuous_system_silence_flush_ms ?? settings.continuous_silence_flush_ms ?? 1200;
  const preRollMs = settings.continuous_pre_roll_ms ?? 300;
  settings.transcribe_batch_interval_ms = Math.max(
    4000,
    Math.min(15000, systemSoftFlush),
  );
  settings.transcribe_vad_silence_ms = Math.max(
    200,
    Math.min(5000, systemSilenceFlush),
  );
  settings.transcribe_chunk_overlap_ms = Math.max(
    0,
    Math.min(3000, preRollMs),
  );
}

async function refreshAIFallbackModels(provider: AIFallbackProvider) {
  if (!settings) return;
  const models = await invoke<string[]>("fetch_available_models", { provider });
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return;
  providerSettings.available_models = models;
  if (!providerSettings.preferred_model || !models.includes(providerSettings.preferred_model)) {
    providerSettings.preferred_model = models[0] ?? "";
  }
  if (settings.ai_fallback.provider === provider) {
    if (!models.includes(settings.ai_fallback.model)) {
      settings.ai_fallback.model = providerSettings.preferred_model || models[0] || "";
    }
  }
}

function setAnalysisBusy(isBusy: boolean) {
  analysisInProgress = isBusy;
  if (dom.analyseButton) {
    dom.analyseButton.disabled = isBusy;
  }
  if (dom.analyseButtonText) {
    dom.analyseButtonText.textContent = isBusy ? "Launching..." : "Analyse";
  }
  if (dom.analyseSpinner) {
    dom.analyseSpinner.style.display = isBusy ? "inline-block" : "none";
  }
}

function analysisOverrideParentPath(): string | null {
  const overridePath = settings?.analysis_tool_path_override?.trim();
  if (!overridePath) return null;
  const lastSeparator = Math.max(overridePath.lastIndexOf("\\"), overridePath.lastIndexOf("/"));
  if (lastSeparator <= 0) return null;
  return overridePath.slice(0, lastSeparator);
}

function analysisExePickerDefaultPath(status: AnalysisToolStatus): string | undefined {
  const overrideParent = analysisOverrideParentPath();
  if (overrideParent) {
    return overrideParent;
  }

  const preferredDir = (status.candidate_dirs || []).find((candidateDir) => {
    const normalized = (candidateDir || "").toLowerCase();
    return normalized.includes("trispr analysis") || normalized.includes("com.trispr.analysis");
  });
  if (preferredDir?.trim()) {
    return preferredDir.trim();
  }

  for (const candidateDir of status.candidate_dirs || []) {
    const trimmed = candidateDir?.trim();
    if (trimmed) {
      return trimmed;
    }
  }

  return undefined;
}

function normalizePathForCompare(input: string): string {
  return input.replace(/\//g, "\\").toLowerCase();
}

function isAnalysisExePath(path: string): boolean {
  const normalized = normalizePathForCompare(path).trim();
  return normalized.endsWith("\\trispr-analysis.exe") || normalized === "trispr-analysis.exe";
}

async function chooseLocalAnalysisExecutable(status: AnalysisToolStatus): Promise<AnalysisToolStatus | null> {
  const defaultPath = analysisExePickerDefaultPath(status);
  const selected = await openDialog({
    title: "Select trispr-analysis.exe",
    filters: [{ name: "Executable", extensions: ["exe"] }],
    multiple: false,
    directory: false,
    defaultPath,
  });

  if (!selected || Array.isArray(selected)) {
    return null;
  }

  const selectedPath = selected as string;
  if (!isAnalysisExePath(selectedPath)) {
    throw new Error("Please select trispr-analysis.exe.");
  }

  const previousOverride = settings?.analysis_tool_path_override ?? "";
  if (settings) {
    settings.analysis_tool_path_override = selectedPath;
    await persistSettings();
  }

  const refreshed = await invoke<AnalysisToolStatus>("analysis_tool_status");
  if (!refreshed.installed || !refreshed.executable_path) {
    if (settings) {
      settings.analysis_tool_path_override = previousOverride;
      await persistSettings();
    }
    throw new Error("Selected file is not a valid Trispr Analysis executable.");
  }

  if (normalizePathForCompare(refreshed.executable_path) !== normalizePathForCompare(selectedPath)) {
    if (settings) {
      settings.analysis_tool_path_override = refreshed.executable_path;
      await persistSettings();
    }
  }

  if (!isAnalysisExePath(refreshed.executable_path)) {
    if (settings) {
      settings.analysis_tool_path_override = previousOverride;
      await persistSettings();
    }
    throw new Error("Selected file is not a valid Trispr Analysis executable.");
  }
  return refreshed;
}

async function ensureAnalysisToolReady(): Promise<AnalysisToolStatus> {
  const status = await invoke<AnalysisToolStatus>("analysis_tool_status");
  if (status.installed) {
    return status;
  }

  const shouldPickLocalExe = window.confirm(
    "Trispr Analysis is not installed.\n\nDo you want to pick a local trispr-analysis.exe now?",
  );
  if (!shouldPickLocalExe) {
    throw new Error("Analysis canceled: no Trispr Analysis executable selected.");
  }

  const refreshed = await chooseLocalAnalysisExecutable(status);
  if (!refreshed?.installed) {
    throw new Error("Analysis canceled: selected executable is invalid or missing.");
  }
  return refreshed;
}

async function confirmAnalysisGpuWarning(): Promise<boolean> {
  if (settings?.analysis_parallel_warning_ack) {
    return true;
  }

  const accepted = window.confirm(ANALYSIS_GPU_WARNING_MESSAGE);
  if (accepted && settings) {
    settings.analysis_parallel_warning_ack = true;
    await persistSettings();
  }

  return accepted;
}

async function launchExternalAnalysis(audioPath: string): Promise<void> {
  await ensureAnalysisToolReady();

  const accepted = await confirmAnalysisGpuWarning();
  if (!accepted) {
    throw new Error("Analysis canceled by user.");
  }

  const launchResult = await invoke<{ status: string }>("analysis_tool_launch", { audioPath });
  if (launchResult?.status !== "launched") {
    throw new Error("Failed to launch external analysis tool.");
  }
}
// Custom vocabulary helper functions
function addVocabRow(original: string, replacement: string) {
  if (!dom.postprocVocabRows) return;

  const row = document.createElement("div");
  row.className = "vocab-row";

  const originalInput = document.createElement("input");
  originalInput.type = "text";
  originalInput.value = original;
  originalInput.placeholder = "api";
  originalInput.className = "vocab-input";

  const replacementInput = document.createElement("input");
  replacementInput.type = "text";
  replacementInput.value = replacement;
  replacementInput.placeholder = "API";
  replacementInput.className = "vocab-input";

  const removeBtn = document.createElement("button");
  removeBtn.textContent = "×";
  removeBtn.className = "vocab-remove";
  removeBtn.title = "Remove entry";

  // Update settings when inputs change
  const updateVocab = async () => {
    if (!settings) return;
    const rows = dom.postprocVocabRows?.querySelectorAll(".vocab-row");
    const vocab: Record<string, string> = {};
    rows?.forEach((r) => {
      const inputs = r.querySelectorAll("input");
      const orig = inputs[0]?.value.trim();
      const repl = inputs[1]?.value.trim();
      if (orig && repl) {
        vocab[orig] = repl;
      }
    });
    settings.postproc_custom_vocab = vocab;
    await persistSettings();
  };

  originalInput.addEventListener("change", updateVocab);
  replacementInput.addEventListener("change", updateVocab);

  removeBtn.addEventListener("click", async () => {
    row.remove();
    await updateVocab();
  });

  row.appendChild(originalInput);
  row.appendChild(replacementInput);
  row.appendChild(removeBtn);
  dom.postprocVocabRows.appendChild(row);
}

// Main tab switching
type MainTab = "transcription" | "settings";

function switchMainTab(tab: MainTab) {
  // Update button states
  const isTranscription = tab === "transcription";

  dom.tabBtnTranscription?.classList.toggle("active", isTranscription);
  dom.tabBtnSettings?.classList.toggle("active", !isTranscription);

  dom.tabBtnTranscription?.setAttribute("aria-selected", isTranscription.toString());
  dom.tabBtnSettings?.setAttribute("aria-selected", (!isTranscription).toString());

  // Update tab content visibility — clear any inline display styles first
  if (dom.tabTranscription) {
    dom.tabTranscription.style.removeProperty("display");
    dom.tabTranscription.classList.toggle("active", isTranscription);
  }
  if (dom.tabSettings) {
    dom.tabSettings.style.removeProperty("display");
    dom.tabSettings.classList.toggle("active", !isTranscription);
  }

  // Persist to localStorage
  try {
    localStorage.setItem("trispr-active-tab", tab);
  } catch (error) {
    console.error("Failed to persist active tab", error);
  }
}

// Initialize tab state from localStorage
export function initMainTab() {
  try {
    const savedTab = localStorage.getItem("trispr-active-tab") as MainTab | null;
    if (savedTab === "settings" || savedTab === "transcription") {
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

export function renderVocabulary() {
  if (!settings || !dom.postprocVocabRows) return;

  // Clear existing rows
  dom.postprocVocabRows.innerHTML = "";

  // Check if vocabulary is empty
  const vocabEntries = Object.entries(settings.postproc_custom_vocab || {});

  if (vocabEntries.length === 0) {
    // Show empty state
    const emptyState = document.createElement("div");
    emptyState.className = "vocab-empty-state";
    emptyState.innerHTML = `
      <div class="vocab-empty-icon">📝</div>
      <div class="vocab-empty-text">No vocabulary entries yet</div>
      <div class="vocab-empty-hint">Click "Add Entry" to define custom word replacements</div>
    `;
    dom.postprocVocabRows.appendChild(emptyState);
  } else {
    // Add rows from settings
    for (const [original, replacement] of vocabEntries) {
      addVocabRow(original, replacement);
    }
  }
}

export function wireEvents() {
  ensureAIFallbackSettingsDefaults();
  ensureContinuousDumpDefaults();

  // Main tab switching
  dom.tabBtnTranscription?.addEventListener("click", () => {
    switchMainTab("transcription");
  });

  dom.tabBtnSettings?.addEventListener("click", () => {
    switchMainTab("settings");
  });

  dom.captureEnabledToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.capture_enabled = dom.captureEnabledToggle!.checked;
    await persistSettings();
    renderHero();
  });

  dom.transcribeEnabledToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.transcribe_enabled = dom.transcribeEnabledToggle!.checked;
    await persistSettings();
    renderHero();
  });

  dom.modeSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.mode = dom.modeSelect!.value as Settings["mode"];
    await persistSettings();
    renderHero();
  });

  dom.modelSourceSelect?.addEventListener("change", async () => {
    if (!settings || !dom.modelSourceSelect) return;
    settings.model_source = dom.modelSourceSelect.value as Settings["model_source"];
    await persistSettings();
    renderSettings();
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

  dom.historyTabMic?.addEventListener("click", () => setHistoryTab("mic"));
  dom.historyTabSystem?.addEventListener("click", () => setHistoryTab("system"));
  dom.historyTabConversation?.addEventListener("click", () => setHistoryTab("conversation"));

  dom.historyCopyConversation?.addEventListener("click", async () => {
    const entries = buildConversationHistory();
    if (!entries.length) return;
    const transcript = buildConversationText(entries);
    await navigator.clipboard.writeText(transcript);
  });

  dom.analyseButton?.addEventListener("click", async () => {
    if (analysisInProgress) {
      showToast({
        type: "info",
        title: "Analysis launcher busy",
        message: "Please wait until the current launch attempt completes.",
        duration: 2500,
      });
      return;
    }

    let audioPath: string;
    try {
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const selected = await openDialog({
        title: "Select Audio File for External Analysis",
        filters: [{ name: "Audio Files", extensions: ["opus", "wav", "mp3", "m4a"] }],
        multiple: false,
        directory: false,
        defaultPath: recordingsDir,
      });
      if (!selected) {
        return;
      }
      audioPath = selected as string;
    } catch (error) {
      showToast({ type: "error", title: "File selection failed", message: String(error), duration: 4000 });
      return;
    }

    setAnalysisBusy(true);
    try {
      await launchExternalAnalysis(audioPath);
      const fileName = audioPath.split(/[/\\]/).pop() ?? audioPath;
      showToast({
        type: "success",
        title: "Analysis launched",
        message: `Opened external analysis tool for ${fileName}`,
        duration: 3500,
      });
    } catch (error) {
      console.error("External analysis launch failed:", error);
      showToast({
        type: "error",
        title: "Analysis launch failed",
        message: String(error),
        duration: 6000,
      });
    } finally {
      setAnalysisBusy(false);
    }
  });

  dom.historyExport?.addEventListener("click", async () => {
    const entries = buildConversationHistory();
    if (!entries.length) {
      showToast({
        type: "warning",
        title: "Nothing to export",
        message: "No transcript entries available",
        duration: 3000,
      });
      return;
    }

    const format = (dom.exportFormat?.value as "txt" | "md" | "json") || "txt";
    const exportContent = buildExportText(entries, format);

    // Determine file extension
    const ext = format === "md" ? "md" : format;
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const filename = `transcript-${timestamp}.${ext}`;

    try {
      // Save file using Tauri
      await invoke("save_transcript", {
        filename,
        content: exportContent,
        format,
      });

      showToast({
        type: "success",
        title: "Export successful",
        message: `Transcript saved as ${filename}`,
        duration: 4000,
      });
    } catch (error) {
      console.error("Export failed:", error);
      showToast({
        type: "error",
        title: "Export failed",
        message: String(error),
        duration: 5000,
      });
    }
  });

  dom.historySearch?.addEventListener("input", () => {
    if (!dom.historySearch) return;
    const query = dom.historySearch.value;
    setSearchQuery(query);
  });

  dom.historySearchClear?.addEventListener("click", () => {
    if (!dom.historySearch) return;
    dom.historySearch.value = "";
    setSearchQuery("");
    dom.historySearch.focus();
  });

  dom.conversationFontSize?.addEventListener("input", () => {
    if (!dom.conversationFontSize) return;
    const size = Number(dom.conversationFontSize.value);
    document.documentElement.style.setProperty("--conversation-font-size", `${size}px`);
    if (dom.conversationFontSizeValue) {
      dom.conversationFontSizeValue.textContent = `${size}px`;
    }
    updateRangeAria("conversation-font-size", size);
    localStorage.setItem("conversationFontSize", size.toString());
  });

  // Hotkey recording functionality
  setupHotkeyRecorder("ptt", dom.pttHotkey, dom.pttHotkeyRecord, dom.pttHotkeyStatus);
  setupHotkeyRecorder("toggle", dom.toggleHotkey, dom.toggleHotkeyRecord, dom.toggleHotkeyStatus);
  setupHotkeyRecorder("transcribe", dom.transcribeHotkey, dom.transcribeHotkeyRecord, dom.transcribeHotkeyStatus);
  setupHotkeyRecorder("toggleActivationWords", dom.toggleActivationWordsHotkey, dom.toggleActivationWordsHotkeyRecord, dom.toggleActivationWordsHotkeyStatus);

  window.addEventListener("resize", () => updateDeviceLineClamp());

  dom.deviceSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.input_device = dom.deviceSelect!.value;
    await persistSettings();
    renderHero();
  });

  dom.transcribeDeviceSelect?.addEventListener("change", async () => {
    if (!settings || !dom.transcribeDeviceSelect) return;
    settings.transcribe_output_device = dom.transcribeDeviceSelect.value;
    await persistSettings();
  });

  dom.transcribeVadToggle?.addEventListener("change", async () => {
    if (!settings || !dom.transcribeVadToggle) return;
    settings.transcribe_vad_mode = dom.transcribeVadToggle.checked;
    if (dom.transcribeBatchField) {
      const disabled = settings.transcribe_vad_mode;
      dom.transcribeBatchField.classList.toggle("is-disabled", disabled);
      dom.transcribeBatchInterval?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeOverlapField) {
      const disabled = settings.transcribe_vad_mode;
      dom.transcribeOverlapField.classList.toggle("is-disabled", disabled);
      dom.transcribeChunkOverlap?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeVadThresholdField) {
      const disabled = !settings.transcribe_vad_mode;
      dom.transcribeVadThresholdField.classList.toggle("is-disabled", disabled);
      dom.transcribeVadThreshold?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeVadSilenceField) {
      const disabled = !settings.transcribe_vad_mode;
      dom.transcribeVadSilenceField.classList.toggle("is-disabled", disabled);
      dom.transcribeVadSilence?.toggleAttribute("disabled", disabled);
    }
    updateTranscribeVadVisibility(settings.transcribe_vad_mode);
    await persistSettings();
  });

  dom.transcribeVadThreshold?.addEventListener("input", () => {
    if (!settings || !dom.transcribeVadThreshold) return;
    const rawDb = Number(dom.transcribeVadThreshold.value);
    const clampedDb = Math.max(VAD_DB_FLOOR, Math.min(0, rawDb));
    settings.transcribe_vad_threshold = Math.min(1, Math.max(0, dbToLevel(clampedDb)));
    if (dom.transcribeVadThresholdValue) {
      dom.transcribeVadThresholdValue.textContent = `${Math.round(clampedDb)} dB`;
    }
    updateRangeAria("transcribe-vad-threshold", clampedDb);
    updateTranscribeThreshold(settings.transcribe_vad_threshold);
  });

  dom.transcribeVadThreshold?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.transcribeVadSilence?.addEventListener("input", () => {
    if (!settings || !dom.transcribeVadSilence) return;
    const value = Number(dom.transcribeVadSilence.value);
    settings.transcribe_vad_silence_ms = Math.max(200, Math.min(5000, value));
    settings.continuous_system_silence_flush_ms = settings.transcribe_vad_silence_ms;
    if (!settings.continuous_system_override_enabled) {
      settings.continuous_silence_flush_ms = settings.transcribe_vad_silence_ms;
    }
    if (dom.transcribeVadSilenceValue) {
      dom.transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
    }
    updateRangeAria("transcribe-vad-silence", value);
  });

  dom.transcribeVadSilence?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.transcribeBatchInterval?.addEventListener("input", () => {
    if (!settings || !dom.transcribeBatchInterval) return;
    const value = Number(dom.transcribeBatchInterval.value);
    settings.transcribe_batch_interval_ms = Math.max(4000, Math.min(15000, value));
    settings.continuous_system_soft_flush_ms = settings.transcribe_batch_interval_ms;
    if (!settings.continuous_system_override_enabled) {
      settings.continuous_soft_flush_ms = settings.transcribe_batch_interval_ms;
    }
    if (dom.transcribeBatchValue) {
      dom.transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
    }
    updateRangeAria("transcribe-batch-interval", value);
  });

  dom.transcribeBatchInterval?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.transcribeChunkOverlap?.addEventListener("input", () => {
    if (!settings || !dom.transcribeChunkOverlap) return;
    const value = Number(dom.transcribeChunkOverlap.value);
    settings.transcribe_chunk_overlap_ms = Math.max(0, Math.min(3000, value));
    settings.continuous_pre_roll_ms = settings.transcribe_chunk_overlap_ms;
    if (settings.transcribe_chunk_overlap_ms > settings.transcribe_batch_interval_ms) {
      settings.transcribe_chunk_overlap_ms = Math.floor(settings.transcribe_batch_interval_ms / 2);
      dom.transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
      settings.continuous_pre_roll_ms = settings.transcribe_chunk_overlap_ms;
    }
    if (dom.transcribeOverlapValue) {
      dom.transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
    }
    updateRangeAria("transcribe-chunk-overlap", settings.transcribe_chunk_overlap_ms);
  });

  dom.transcribeChunkOverlap?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.transcribeGain?.addEventListener("input", () => {
    if (!settings || !dom.transcribeGain) return;
    const value = Number(dom.transcribeGain.value);
    settings.transcribe_input_gain_db = Math.max(-30, Math.min(30, value));
    if (dom.transcribeGainValue) {
      const gain = Math.round(settings.transcribe_input_gain_db);
      dom.transcribeGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
    }
    updateRangeAria("transcribe-gain", value);
  });

  dom.transcribeGain?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.languageSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_mode = dom.languageSelect!.value as Settings["language_mode"];
    await persistSettings();
  });

  dom.languagePinnedToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_pinned = dom.languagePinnedToggle!.checked;
    await persistSettings();
  });

  dom.cloudToggle?.addEventListener("change", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.enabled = dom.cloudToggle!.checked;
    settings.cloud_fallback = settings.ai_fallback.enabled;
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
  });

  dom.audioCuesToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.audio_cues = dom.audioCuesToggle!.checked;
    await persistSettings();
  });

  dom.pttUseVadToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.ptt_use_vad = dom.pttUseVadToggle!.checked;
    await persistSettings();
  });

  dom.audioCuesVolume?.addEventListener("input", () => {
    if (!settings || !dom.audioCuesVolume) return;
    const value = Number(dom.audioCuesVolume.value);
    settings.audio_cues_volume = Math.min(1, Math.max(0, value / 100));
    if (dom.audioCuesVolumeValue) {
      dom.audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
    }
    updateRangeAria("audio-cues-volume", value);
  });

  dom.audioCuesVolume?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.hallucinationFilterToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.hallucination_filter_enabled = dom.hallucinationFilterToggle!.checked;
    await persistSettings();
  });

  dom.activationWordsToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.activation_words_enabled = dom.activationWordsToggle!.checked;
    await persistSettings();
    renderSettings();
  });

  dom.activationWordsList?.addEventListener("change", async () => {
    if (!settings || !dom.activationWordsList) return;
    const lines = dom.activationWordsList.value
      .split('\n')
      .map(line => line.trim())
      .filter(line => line.length > 0);
    settings.activation_words = lines;
    await persistSettings();
  });

  // Quality & Encoding event listeners
  dom.opusEnabledToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.opus_enabled = dom.opusEnabledToggle!.checked;
    await persistSettings();
  });

  dom.opusBitrateSelect?.addEventListener("change", async () => {
    if (!settings || !dom.opusBitrateSelect) return;
    settings.opus_bitrate_kbps = parseInt(dom.opusBitrateSelect.value, 10);
    await persistSettings();
  });

  dom.autoSaveSystemAudioToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.auto_save_system_audio = dom.autoSaveSystemAudioToggle!.checked;
    await persistSettings();
  });

  dom.autoSaveMicAudioToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.auto_save_mic_audio = dom.autoSaveMicAudioToggle!.checked;
    await persistSettings();
  });

  dom.continuousDumpEnabledToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.continuous_dump_enabled = dom.continuousDumpEnabledToggle!.checked;
    await persistSettings();
  });

  dom.continuousDumpProfile?.addEventListener("change", async () => {
    if (!settings || !dom.continuousDumpProfile) return;
    settings.continuous_dump_profile = dom.continuousDumpProfile.value as "balanced" | "low_latency" | "high_quality";
    applyContinuousProfile(settings.continuous_dump_profile);
    renderSettings();
    await persistSettings();
  });

  dom.continuousHardCut?.addEventListener("input", () => {
    if (!settings || !dom.continuousHardCut) return;
    const value = Math.max(15000, Math.min(120000, Number(dom.continuousHardCut.value)));
    settings.continuous_hard_cut_ms = value;
    if (!settings.continuous_system_override_enabled) settings.continuous_system_hard_cut_ms = value;
    if (!settings.continuous_mic_override_enabled) settings.continuous_mic_hard_cut_ms = value;
    if (dom.continuousHardCutValue) dom.continuousHardCutValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-hard-cut", value);
  });
  dom.continuousHardCut?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousMinChunk?.addEventListener("input", () => {
    if (!settings || !dom.continuousMinChunk) return;
    const value = Math.max(250, Math.min(5000, Number(dom.continuousMinChunk.value)));
    settings.continuous_min_chunk_ms = value;
    if (dom.continuousMinChunkValue) dom.continuousMinChunkValue.textContent = `${(value / 1000).toFixed(1)}s`;
    updateRangeAria("continuous-min-chunk", value);
  });
  dom.continuousMinChunk?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousPreRoll?.addEventListener("input", () => {
    if (!settings || !dom.continuousPreRoll) return;
    const value = Math.max(0, Math.min(1500, Number(dom.continuousPreRoll.value)));
    settings.continuous_pre_roll_ms = value;
    settings.transcribe_chunk_overlap_ms = Math.max(0, Math.min(3000, value));
    if (dom.continuousPreRollValue) dom.continuousPreRollValue.textContent = `${(value / 1000).toFixed(2)}s`;
    if (dom.transcribeChunkOverlap) dom.transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
    if (dom.transcribeOverlapValue) dom.transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
    updateRangeAria("continuous-pre-roll", value);
  });
  dom.continuousPreRoll?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousPostRoll?.addEventListener("input", () => {
    if (!settings || !dom.continuousPostRoll) return;
    const value = Math.max(0, Math.min(1500, Number(dom.continuousPostRoll.value)));
    settings.continuous_post_roll_ms = value;
    if (dom.continuousPostRollValue) dom.continuousPostRollValue.textContent = `${(value / 1000).toFixed(2)}s`;
    updateRangeAria("continuous-post-roll", value);
  });
  dom.continuousPostRoll?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousKeepalive?.addEventListener("input", () => {
    if (!settings || !dom.continuousKeepalive) return;
    const value = Math.max(10000, Math.min(120000, Number(dom.continuousKeepalive.value)));
    settings.continuous_idle_keepalive_ms = value;
    if (dom.continuousKeepaliveValue) dom.continuousKeepaliveValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-keepalive", value);
  });
  dom.continuousKeepalive?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousSystemOverrideToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.continuous_system_override_enabled = dom.continuousSystemOverrideToggle!.checked;
    if (!settings.continuous_system_override_enabled) {
      settings.continuous_system_soft_flush_ms = settings.continuous_soft_flush_ms!;
      settings.continuous_system_silence_flush_ms = settings.continuous_silence_flush_ms!;
      settings.continuous_system_hard_cut_ms = settings.continuous_hard_cut_ms!;
    }
    renderSettings();
    await persistSettings();
  });

  dom.continuousSystemSoftFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousSystemSoftFlush) return;
    const value = Math.max(4000, Math.min(30000, Number(dom.continuousSystemSoftFlush.value)));
    settings.continuous_system_soft_flush_ms = value;
    settings.transcribe_batch_interval_ms = Math.max(4000, Math.min(15000, value));
    if (dom.continuousSystemSoftFlushValue) dom.continuousSystemSoftFlushValue.textContent = `${Math.round(value / 1000)}s`;
    if (dom.transcribeBatchInterval) dom.transcribeBatchInterval.value = settings.transcribe_batch_interval_ms.toString();
    if (dom.transcribeBatchValue) dom.transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
    updateRangeAria("continuous-system-soft-flush", value);
  });
  dom.continuousSystemSoftFlush?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousSystemSilenceFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousSystemSilenceFlush) return;
    const value = Math.max(300, Math.min(5000, Number(dom.continuousSystemSilenceFlush.value)));
    settings.continuous_system_silence_flush_ms = value;
    settings.transcribe_vad_silence_ms = Math.max(200, Math.min(5000, value));
    if (dom.continuousSystemSilenceFlushValue) dom.continuousSystemSilenceFlushValue.textContent = `${(value / 1000).toFixed(1)}s`;
    if (dom.transcribeVadSilence) dom.transcribeVadSilence.value = settings.transcribe_vad_silence_ms.toString();
    if (dom.transcribeVadSilenceValue) dom.transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
    updateRangeAria("continuous-system-silence-flush", value);
  });
  dom.continuousSystemSilenceFlush?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousSystemHardCut?.addEventListener("input", () => {
    if (!settings || !dom.continuousSystemHardCut) return;
    const value = Math.max(15000, Math.min(120000, Number(dom.continuousSystemHardCut.value)));
    settings.continuous_system_hard_cut_ms = value;
    if (dom.continuousSystemHardCutValue) dom.continuousSystemHardCutValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-system-hard-cut", value);
  });
  dom.continuousSystemHardCut?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousMicOverrideToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.continuous_mic_override_enabled = dom.continuousMicOverrideToggle!.checked;
    if (!settings.continuous_mic_override_enabled) {
      settings.continuous_mic_soft_flush_ms = settings.continuous_soft_flush_ms!;
      settings.continuous_mic_silence_flush_ms = settings.continuous_silence_flush_ms!;
      settings.continuous_mic_hard_cut_ms = settings.continuous_hard_cut_ms!;
    }
    renderSettings();
    await persistSettings();
  });

  dom.continuousMicSoftFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicSoftFlush) return;
    const value = Math.max(4000, Math.min(30000, Number(dom.continuousMicSoftFlush.value)));
    settings.continuous_mic_soft_flush_ms = value;
    if (dom.continuousMicSoftFlushValue) dom.continuousMicSoftFlushValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-mic-soft-flush", value);
  });
  dom.continuousMicSoftFlush?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousMicSilenceFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicSilenceFlush) return;
    const value = Math.max(300, Math.min(5000, Number(dom.continuousMicSilenceFlush.value)));
    settings.continuous_mic_silence_flush_ms = value;
    if (dom.continuousMicSilenceFlushValue) dom.continuousMicSilenceFlushValue.textContent = `${(value / 1000).toFixed(1)}s`;
    updateRangeAria("continuous-mic-silence-flush", value);
  });
  dom.continuousMicSilenceFlush?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  dom.continuousMicHardCut?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicHardCut) return;
    const value = Math.max(15000, Math.min(120000, Number(dom.continuousMicHardCut.value)));
    settings.continuous_mic_hard_cut_ms = value;
    if (dom.continuousMicHardCutValue) dom.continuousMicHardCutValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-mic-hard-cut", value);
  });
  dom.continuousMicHardCut?.addEventListener("change", async () => { if (settings) await persistSettings(); });

  // Post-processing event listeners
  dom.postprocEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_enabled = dom.postprocEnabled!.checked;
    await persistSettings();
    renderSettings();
  });

  dom.postprocLanguage?.addEventListener("change", async () => {
    if (!settings || !dom.postprocLanguage) return;
    settings.postproc_language = dom.postprocLanguage.value;
    await persistSettings();
  });

  dom.postprocPunctuation?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_punctuation_enabled = dom.postprocPunctuation!.checked;
    await persistSettings();
  });

  dom.postprocCapitalization?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_capitalization_enabled = dom.postprocCapitalization!.checked;
    await persistSettings();
  });

  dom.postprocNumbers?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_numbers_enabled = dom.postprocNumbers!.checked;
    await persistSettings();
  });

  dom.postprocCustomVocabEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_custom_vocab_enabled = dom.postprocCustomVocabEnabled!.checked;
    await persistSettings();
    renderSettings();
  });

  dom.postprocVocabAdd?.addEventListener("click", () => {
    addVocabRow("", "");
  });

  // AI fallback event listeners
  dom.aiFallbackEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.enabled = dom.aiFallbackEnabled!.checked;
    settings.cloud_fallback = settings.ai_fallback.enabled;
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
  });

  dom.aiFallbackProvider?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackProvider) return;
    ensureAIFallbackSettingsDefaults();
    const provider = normalizeAIFallbackProvider(dom.aiFallbackProvider.value);
    settings.ai_fallback.provider = provider;
    settings.postproc_llm_provider = provider;

    try {
      await refreshAIFallbackModels(provider);
    } catch (error) {
      console.warn(`Failed to refresh models for ${provider}:`, error);
    }

    const providerSettings = getAIFallbackProviderSettings(provider);
    if (providerSettings) {
      if (!providerSettings.preferred_model) {
        providerSettings.preferred_model = providerSettings.available_models[0] ?? "";
      }
      settings.ai_fallback.model = providerSettings.preferred_model;
      settings.postproc_llm_model = settings.ai_fallback.model;
    }
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
  });

  dom.aiFallbackModel?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackModel) return;
    ensureAIFallbackSettingsDefaults();
    const provider = normalizeAIFallbackProvider(settings.ai_fallback.provider);
    settings.ai_fallback.model = dom.aiFallbackModel.value;
    settings.postproc_llm_model = settings.ai_fallback.model;
    const providerSettings = getAIFallbackProviderSettings(provider);
    if (providerSettings) {
      providerSettings.preferred_model = settings.ai_fallback.model;
    }
    await persistSettings();
  });

  dom.aiFallbackSaveKeyBtn?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = normalizeAIFallbackProvider(settings.ai_fallback.provider);
    const apiKey = dom.aiFallbackApiKeyInput?.value?.trim() ?? "";
    if (!apiKey) {
      showToast({
        type: "warning",
        title: "Missing API key",
        message: "Paste an API key before saving.",
        duration: 3000,
      });
      return;
    }
    try {
      await invoke("save_provider_api_key", { provider, apiKey });
      const providerSettings = getAIFallbackProviderSettings(provider);
      if (providerSettings) {
        providerSettings.api_key_stored = true;
      }
      if (dom.aiFallbackApiKeyInput) {
        dom.aiFallbackApiKeyInput.value = "";
      }
      renderAIFallbackSettingsUi();
      showToast({
        type: "success",
        title: "API key saved",
        message: `Stored API key for ${provider}.`,
        duration: 2500,
      });
    } catch (error) {
      showToast({
        type: "error",
        title: "API key save failed",
        message: String(error),
        duration: 5000,
      });
    }
  });

  dom.aiFallbackClearKeyBtn?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = normalizeAIFallbackProvider(settings.ai_fallback.provider);
    try {
      await invoke("clear_provider_api_key", { provider });
      const providerSettings = getAIFallbackProviderSettings(provider);
      if (providerSettings) {
        providerSettings.api_key_stored = false;
      }
      renderAIFallbackSettingsUi();
      showToast({
        type: "info",
        title: "API key removed",
        message: `Removed API key for ${provider}.`,
        duration: 2500,
      });
    } catch (error) {
      showToast({
        type: "error",
        title: "API key remove failed",
        message: String(error),
        duration: 5000,
      });
    }
  });

  dom.aiFallbackTestKeyBtn?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = normalizeAIFallbackProvider(settings.ai_fallback.provider);
    const apiKey = dom.aiFallbackApiKeyInput?.value?.trim() ?? "";
    if (!apiKey) {
      showToast({
        type: "warning",
        title: "Missing API key",
        message: "Paste an API key in the field to test the connection.",
        duration: 3000,
      });
      return;
    }
    try {
      const result = await invoke<{ message?: string }>("test_provider_connection", { provider, apiKey });
      await refreshAIFallbackModels(provider);
      renderAIFallbackSettingsUi();
      showToast({
        type: "success",
        title: "Connection test passed",
        message: result?.message ?? `Provider ${provider} accepted the key format.`,
        duration: 3500,
      });
    } catch (error) {
      showToast({
        type: "error",
        title: "Connection test failed",
        message: String(error),
        duration: 5000,
      });
    }
  });

  dom.aiFallbackTemperature?.addEventListener("input", () => {
    if (!settings || !dom.aiFallbackTemperature) return;
    ensureAIFallbackSettingsDefaults();
    const value = Math.max(0, Math.min(1, Number(dom.aiFallbackTemperature.value)));
    settings.ai_fallback.temperature = value;
    if (dom.aiFallbackTemperatureValue) {
      dom.aiFallbackTemperatureValue.textContent = value.toFixed(2);
    }
    updateRangeAria("ai-fallback-temperature", Math.round(value * 100));
  });

  dom.aiFallbackTemperature?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.aiFallbackMaxTokens?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackMaxTokens) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.max_tokens = Math.max(128, Math.min(8192, Number(dom.aiFallbackMaxTokens.value)));
    await persistSettings();
  });

  dom.aiFallbackCustomPromptEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.custom_prompt_enabled = dom.aiFallbackCustomPromptEnabled!.checked;
    settings.ai_fallback.use_default_prompt = !settings.ai_fallback.custom_prompt_enabled;
    if (settings.ai_fallback.custom_prompt_enabled && settings.ai_fallback.custom_prompt.trim().length > 0) {
      settings.postproc_llm_prompt = settings.ai_fallback.custom_prompt;
    }
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackCustomPrompt?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value.trim();
    settings.ai_fallback.use_default_prompt = settings.ai_fallback.custom_prompt.length === 0;
    if (settings.ai_fallback.custom_prompt.length > 0) {
      settings.postproc_llm_prompt = settings.ai_fallback.custom_prompt;
    }
    await persistSettings();
  });

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

  dom.micGain?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

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

  dom.vadThreshold?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.vadSilence?.addEventListener("input", () => {
    if (!settings || !dom.vadSilence) return;
    const value = Math.max(200, Math.min(4000, Number(dom.vadSilence.value)));
    settings.vad_silence_ms = value;
    if (dom.vadSilenceValue) {
      dom.vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
    }
    updateRangeAria("vad-silence", value);
  });

  dom.vadSilence?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayColor) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_color = dom.overlayColor.value;
    } else {
      settings.overlay_color = dom.overlayColor.value;
    }
  });

  dom.overlayColor?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayMinRadius?.addEventListener("input", () => {
    if (!settings || !dom.overlayMinRadius || !dom.overlayMaxRadius) return;
    settings.overlay_min_radius = Number(dom.overlayMinRadius.value);
    if (settings.overlay_min_radius > settings.overlay_max_radius) {
      settings.overlay_max_radius = settings.overlay_min_radius;
      dom.overlayMaxRadius.value = Math.round(settings.overlay_max_radius).toString();
    }
    if (dom.overlayMinRadiusValue) {
      dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (dom.overlayMaxRadiusValue) {
      dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
    updateRangeAria("overlay-min-radius", settings.overlay_min_radius);
  });

  dom.overlayMinRadius?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayMaxRadius?.addEventListener("input", () => {
    if (!settings || !dom.overlayMaxRadius || !dom.overlayMinRadius) return;
    settings.overlay_max_radius = Number(dom.overlayMaxRadius.value);
    if (settings.overlay_max_radius < settings.overlay_min_radius) {
      settings.overlay_min_radius = settings.overlay_max_radius;
      dom.overlayMinRadius.value = Math.round(settings.overlay_min_radius).toString();
    }
    if (dom.overlayMinRadiusValue) {
      dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (dom.overlayMaxRadiusValue) {
      dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
    updateRangeAria("overlay-max-radius", settings.overlay_max_radius);
  });

  dom.overlayMaxRadius?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayRise?.addEventListener("input", () => {
    if (!settings || !dom.overlayRise) return;
    const value = Number(dom.overlayRise.value);
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_rise_ms = value;
    } else {
      settings.overlay_rise_ms = value;
    }
    if (dom.overlayRiseValue) dom.overlayRiseValue.textContent = `${value}`;
    updateRangeAria("overlay-rise", value);
  });

  dom.overlayRise?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayFall?.addEventListener("input", () => {
    if (!settings || !dom.overlayFall) return;
    const value = Number(dom.overlayFall.value);
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_fall_ms = value;
    } else {
      settings.overlay_fall_ms = value;
    }
    if (dom.overlayFallValue) dom.overlayFallValue.textContent = `${value}`;
    updateRangeAria("overlay-fall", value);
  });

  dom.overlayFall?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayOpacityInactive?.addEventListener("input", () => {
    if (!settings || !dom.overlayOpacityInactive || !dom.overlayOpacityActive) return;
    const value = Math.min(1, Math.max(0.05, Number(dom.overlayOpacityInactive.value) / 100));
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_opacity_inactive = value;
      if (settings.overlay_kitt_opacity_active < settings.overlay_kitt_opacity_inactive) {
        settings.overlay_kitt_opacity_active = settings.overlay_kitt_opacity_inactive;
        dom.overlayOpacityActive.value = Math.round(settings.overlay_kitt_opacity_active * 100).toString();
      }
      if (dom.overlayOpacityInactiveValue) {
        dom.overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_inactive * 100)}%`;
      }
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_active * 100)}%`;
      }
    } else {
      settings.overlay_opacity_inactive = value;
      if (settings.overlay_opacity_active < settings.overlay_opacity_inactive) {
        settings.overlay_opacity_active = settings.overlay_opacity_inactive;
        dom.overlayOpacityActive.value = Math.round(settings.overlay_opacity_active * 100).toString();
      }
      if (dom.overlayOpacityInactiveValue) {
        dom.overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_opacity_inactive * 100)}%`;
      }
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
      }
    }
    updateRangeAria("overlay-opacity-inactive", Number(dom.overlayOpacityInactive.value));
  });

  dom.overlayOpacityInactive?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayOpacityActive?.addEventListener("input", () => {
    if (!settings || !dom.overlayOpacityActive || !dom.overlayOpacityInactive) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      const value = Math.min(
        1,
        Math.max(settings.overlay_kitt_opacity_inactive, Number(dom.overlayOpacityActive.value) / 100)
      );
      settings.overlay_kitt_opacity_active = value;
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_active * 100)}%`;
      }
    } else {
      const value = Math.min(
        1,
        Math.max(settings.overlay_opacity_inactive, Number(dom.overlayOpacityActive.value) / 100)
      );
      settings.overlay_opacity_active = value;
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
      }
    }
    updateRangeAria("overlay-opacity-active", Number(dom.overlayOpacityActive.value));
  });

  dom.overlayOpacityActive?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayPosX?.addEventListener("change", async () => {
    if (!settings || !dom.overlayPosX) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_pos_x = Number(dom.overlayPosX.value);
    } else {
      settings.overlay_pos_x = Number(dom.overlayPosX.value);
    }
    await persistSettings();
  });

  dom.overlayPosY?.addEventListener("change", async () => {
    if (!settings || !dom.overlayPosY) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_pos_y = Number(dom.overlayPosY.value);
    } else {
      settings.overlay_pos_y = Number(dom.overlayPosY.value);
    }
    await persistSettings();
  });

  dom.overlayStyle?.addEventListener("change", async () => {
    if (!settings || !dom.overlayStyle) return;
    settings.overlay_style = dom.overlayStyle.value;
    updateOverlayStyleVisibility(dom.overlayStyle.value);
    applyOverlaySharedUi(dom.overlayStyle.value);
    await persistSettings();
  });

  dom.overlayKittMinWidth?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittMinWidth) return;
    settings.overlay_kitt_min_width = Number(dom.overlayKittMinWidth.value);
    if (dom.overlayKittMinWidthValue) dom.overlayKittMinWidthValue.textContent = `${Math.round(settings.overlay_kitt_min_width)}`;
    updateRangeAria("overlay-kitt-min-width", settings.overlay_kitt_min_width);
  });

  dom.overlayKittMinWidth?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayKittMaxWidth?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittMaxWidth) return;
    settings.overlay_kitt_max_width = Number(dom.overlayKittMaxWidth.value);
    if (dom.overlayKittMaxWidthValue) dom.overlayKittMaxWidthValue.textContent = `${Math.round(settings.overlay_kitt_max_width)}`;
    updateRangeAria("overlay-kitt-max-width", settings.overlay_kitt_max_width);
  });

  dom.overlayKittMaxWidth?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayKittHeight?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittHeight) return;
    settings.overlay_kitt_height = Number(dom.overlayKittHeight.value);
    if (dom.overlayKittHeightValue) dom.overlayKittHeightValue.textContent = `${Math.round(settings.overlay_kitt_height)}`;
    updateRangeAria("overlay-kitt-height", settings.overlay_kitt_height);
  });

  dom.overlayKittHeight?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  // Apply Overlay Settings button
  const applyOverlayBtn = document.getElementById("apply-overlay-btn");
  applyOverlayBtn?.addEventListener("click", async () => {
    if (!settings) return;
    await persistSettings();
    showToast({ title: "Applied", message: "Overlay settings applied", type: "success" });
  });

  // Chapter settings
  dom.chaptersEnabled?.addEventListener("change", async () => {
    if (!settings || !dom.chaptersEnabled) return;
    settings.chapters_enabled = dom.chaptersEnabled.checked;

    // Toggle visibility of chapter settings
    if (dom.chaptersSettings) {
      dom.chaptersSettings.style.display = dom.chaptersEnabled.checked ? "block" : "none";
    }

    await persistSettings();
    renderSettings();
    updateChaptersVisibility();
  });

  dom.chaptersShowIn?.addEventListener("change", async () => {
    if (!settings || !dom.chaptersShowIn) return;
    settings.chapters_show_in = dom.chaptersShowIn.value as "conversation" | "all";
    await persistSettings();
    updateChaptersVisibility();
  });

  dom.chaptersMethod?.addEventListener("change", async () => {
    if (!settings || !dom.chaptersMethod) return;
    settings.chapters_method = dom.chaptersMethod.value as "silence" | "time" | "hybrid";
    await persistSettings();
    updateChaptersVisibility();
  });

  // Topic keywords reset
  dom.topicKeywordsReset?.addEventListener("click", async () => {
    setTopicKeywords(DEFAULT_TOPICS);
    await renderTopicKeywords();
    await persistSettings();
  });
}
