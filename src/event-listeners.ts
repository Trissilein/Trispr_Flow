// DOM event listeners setup

import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Settings, TranscriptionAnalysis } from "./types";
import { settings } from "./state";
import * as dom from "./dom-refs";
import { persistSettings, updateOverlayStyleVisibility, applyOverlaySharedUi, updateTranscribeVadVisibility, updateTranscribeThreshold } from "./settings";
import { renderSettings } from "./settings";
import { renderHero, updateDeviceLineClamp, updateThresholdMarkers } from "./ui-state";
import { refreshModels, refreshModelsDir } from "./models";
import { applyPanelCollapsed, setHistoryTab, buildConversationHistory, buildConversationText, buildExportText, setSearchQuery, renderSpeakerSegments, buildSpeakerExportTxt } from "./history";
import { setupHotkeyRecorder } from "./hotkeys";
import { updateRangeAria } from "./accessibility";
import { showToast } from "./toast";
import { dbToLevel, VAD_DB_FLOOR } from "./ui-helpers";
import { updateChaptersVisibility } from "./chapters";

// =====================================================================
// Voice Analysis Dialog helpers
// =====================================================================

let lastAnalysisResults: TranscriptionAnalysis | null = null;
let analysisInProgress = false;

function openAnalysisDialog() {
  const dialog = document.getElementById("analysis-dialog");
  if (!dialog) return;
  dialog.style.display = "flex";
  // Reset all stages
  setAnalysisStage("file", "pending");
  setAnalysisStage("engine", "pending");
  setAnalysisStage("run", "pending");
  const fileDetail = document.getElementById("stage-detail-file");
  if (fileDetail) fileDetail.textContent = "";
  const runDetail = document.getElementById("stage-detail-run");
  if (runDetail) runDetail.textContent = "";
  // Hide results and footer
  const results = document.getElementById("analysis-results-area");
  const footer = document.getElementById("analysis-dialog-footer");
  if (results) { results.style.display = "none"; results.innerHTML = ""; }
  if (footer) footer.style.display = "none";
  // Show copy button in case it was hidden on previous error
  const copyBtn = document.getElementById("analysis-copy-btn");
  if (copyBtn) (copyBtn as HTMLElement).style.display = "";
  lastAnalysisResults = null;
}

function closeAnalysisDialog() {
  const dialog = document.getElementById("analysis-dialog");
  if (dialog) dialog.style.display = "none";
  analysisInProgress = false;
}

function setAnalysisStage(id: string, state: "pending" | "active" | "done" | "error") {
  const el = document.getElementById(`analysis-stage-${id}`);
  if (!el) return;
  el.className = `analysis-stage ${state}`;
}

function showAnalysisResultsInDialog(analysis: TranscriptionAnalysis) {
  const results = document.getElementById("analysis-results-area");
  const footer = document.getElementById("analysis-dialog-footer");
  if (!results || !footer) return;

  const mins = Math.floor(analysis.duration_s / 60);
  const secs = Math.floor(analysis.duration_s % 60);
  const segmentsHtml = renderSpeakerSegments(analysis.segments);

  results.innerHTML = `
    <div class="analysis-results-meta">
      <span>${mins}m ${secs}s</span>
      <span>•</span>
      <span>${analysis.total_speakers} speaker(s)</span>
      <span>•</span>
      <span>${analysis.segments.length} segment(s)</span>
    </div>
    ${segmentsHtml}
  `;
  results.style.display = "block";
  footer.style.display = "flex";
  lastAnalysisResults = analysis;
}

function showAnalysisErrorInDialog(message: string, isSidecarError: boolean) {
  const results = document.getElementById("analysis-results-area");
  const footer = document.getElementById("analysis-dialog-footer");
  if (!results || !footer) return;

  const bodyText = isSidecarError
    ? `Voice Analysis engine could not start.<br>Run: <code>pip install -r sidecar/vibevoice-asr/requirements.txt</code>`
    : message;

  results.innerHTML = `
    <div class="analysis-error">
      <p class="analysis-error-title">Analysis failed</p>
      <p class="analysis-error-msg">${bodyText}</p>
    </div>
  `;
  results.style.display = "block";

  // Hide copy button — nothing to copy on error
  const copyBtn = document.getElementById("analysis-copy-btn");
  if (copyBtn) (copyBtn as HTMLElement).style.display = "none";

  footer.style.display = "flex";
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
    if (!panelId) return;
    button.addEventListener("click", () => {
      const panel = document.querySelector(`[data-panel="${panelId}"]`);
      const collapsed = panel?.classList.contains("panel-collapsed") ?? false;
      applyPanelCollapsed(panelId, !collapsed);
    });
  });

  document.querySelectorAll<HTMLElement>(".panel-header").forEach((header) => {
    const panel = header.closest<HTMLElement>(".panel");
    const panelId = panel?.dataset.panel;
    if (!panelId) return;
    header.addEventListener("click", (event) => {
      const target = event.target as HTMLElement | null;
      if (!target) return;
      if (target.closest(".panel-actions")) return;
      if (target.closest("button, input, select, textarea, a, label")) return;
      const collapsed = panel?.classList.contains("panel-collapsed") ?? false;
      applyPanelCollapsed(panelId, !collapsed);
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
    if (analysisInProgress) return; // Prevent re-entry while analysis is running

    // Step 1: File picker — before opening dialog so Cancel doesn't flash the UI
    let audioPath: string | null = null;
    try {
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const selected = await openDialog({
        title: "Select Audio File for Voice Analysis",
        filters: [{ name: "Audio Files", extensions: ["opus", "wav", "mp3", "m4a"] }],
        multiple: false,
        directory: false,
        defaultPath: recordingsDir,
      });
      if (!selected) return; // User cancelled
      audioPath = selected as string;
    } catch (err) {
      showToast({ type: "error", title: "File selection failed", message: String(err), duration: 4000 });
      return;
    }

    // Step 2: Open dialog and mark in-progress
    analysisInProgress = true;
    openAnalysisDialog();

    // Mark file stage done
    const fileName = (audioPath as string).split(/[/\\]/).pop() ?? audioPath;
    setAnalysisStage("file", "done");
    const fileDetail = document.getElementById("stage-detail-file");
    if (fileDetail) fileDetail.textContent = fileName;

    try {
      const isParallel = settings?.parallel_mode ?? false;

      // Step 3: Start engine
      setAnalysisStage("engine", "active");
      await invoke("start_sidecar");
      setAnalysisStage("engine", "done");

      // Step 4: Run analysis
      setAnalysisStage("run", "active");

      if (isParallel) {
        const result = await invoke<any>("parallel_transcribe", {
          audioPath,
          precision: settings?.vibevoice_precision || "fp16",
          language: settings?.language_mode || "auto",
        });

        if (result.vibevoice && !result.vibevoice.error) {
          const analysis: TranscriptionAnalysis = {
            segments: result.vibevoice.segments.map((seg: any) => ({
              speaker_id: seg.speaker,
              start_time: seg.start_time,
              end_time: seg.end_time,
              text: seg.text,
            })),
            duration_s: result.vibevoice.metadata.duration,
            total_speakers: result.vibevoice.metadata.num_speakers,
            processing_time_ms: result.vibevoice.metadata.processing_time * 1000,
          };
          setAnalysisStage("run", "done");
          showAnalysisResultsInDialog(analysis);
        } else {
          throw new Error("Parallel analysis returned no VibeVoice results");
        }
      } else {
        const result = await invoke<any>("sidecar_transcribe", {
          audioPath,
          precision: settings?.vibevoice_precision || "fp16",
          language: settings?.language_mode || "auto",
        });

        const analysis: TranscriptionAnalysis = {
          segments: result.segments.map((seg: any) => ({
            speaker_id: seg.speaker,
            start_time: seg.start_time,
            end_time: seg.end_time,
            text: seg.text,
          })),
          duration_s: result.metadata.duration,
          total_speakers: result.metadata.num_speakers,
          processing_time_ms: result.metadata.processing_time * 1000,
        };
        setAnalysisStage("run", "done");
        showAnalysisResultsInDialog(analysis);
      }
    } catch (error) {
      console.error("Voice Analysis failed:", error);
      const msg = String(error);
      const isSidecarError =
        msg.includes("Sidecar") || msg.includes("sidecar") ||
        msg.includes("pip") || msg.includes("fastapi") ||
        msg.includes("No module");

      // Mark the active stage as error
      ["file", "engine", "run"].forEach((id) => {
        const el = document.getElementById(`analysis-stage-${id}`);
        if (el?.classList.contains("active")) setAnalysisStage(id, "error");
      });

      showAnalysisErrorInDialog(msg, isSidecarError);
    } finally {
      // analysisInProgress is reset by closeAnalysisDialog() when user clicks Done/X
      // But if dialog never opened (shouldn't happen), unblock anyway:
    }
  });

  // Voice Analysis dialog — close buttons
  document.getElementById("analysis-dialog-close")?.addEventListener("click", closeAnalysisDialog);
  document.getElementById("analysis-done-btn")?.addEventListener("click", closeAnalysisDialog);

  // Close on backdrop click
  document.getElementById("analysis-dialog")?.addEventListener("click", (e) => {
    if (e.target === e.currentTarget) closeAnalysisDialog();
  });

  // Copy transcript
  document.getElementById("analysis-copy-btn")?.addEventListener("click", async () => {
    if (!lastAnalysisResults) return;
    const text = buildSpeakerExportTxt(lastAnalysisResults);
    await navigator.clipboard.writeText(text);
    showToast({ type: "success", title: "Copied", message: "Voice Analysis transcript copied to clipboard", duration: 2500 });
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
    if (settings.transcribe_chunk_overlap_ms > settings.transcribe_batch_interval_ms) {
      settings.transcribe_chunk_overlap_ms = Math.floor(settings.transcribe_batch_interval_ms / 2);
      dom.transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
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
    settings.cloud_fallback = dom.cloudToggle!.checked;
    await persistSettings();
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

  dom.vibevoicePrecisionSelect?.addEventListener("change", async () => {
    if (!settings || !dom.vibevoicePrecisionSelect) return;
    settings.vibevoice_precision = dom.vibevoicePrecisionSelect.value as "fp16" | "int8";
    await persistSettings();
  });

  dom.autoSaveSystemAudioToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.auto_save_system_audio = dom.autoSaveSystemAudioToggle!.checked;
    await persistSettings();
  });

  dom.parallelModeToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.parallel_mode = dom.parallelModeToggle!.checked;
    await persistSettings();
  });

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
    const { setTopicKeywords, DEFAULT_TOPICS } = await import("./history");
    const { renderTopicKeywords, persistSettings } = await import("./settings");
    setTopicKeywords(DEFAULT_TOPICS);
    await renderTopicKeywords();
    await persistSettings();
  });
}
