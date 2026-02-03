import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Settings {
  mode: "ptt" | "vad";
  hotkey_ptt: string;
  hotkey_toggle: string;
  input_device: string;
  language_mode: "auto";
  model: string;
  cloud_fallback: boolean;
  audio_cues: boolean;
  audio_cues_volume: number;
  ptt_use_vad: boolean;
  vad_threshold: number;
  vad_threshold_start: number;
  vad_threshold_sustain: number;
  vad_silence_ms: number;
  transcribe_enabled: boolean;
  transcribe_hotkey: string;
  transcribe_output_device: string;
  transcribe_vad_mode: boolean;
  transcribe_vad_threshold: number;
  transcribe_vad_silence_ms: number;
  transcribe_batch_interval_ms: number;
  transcribe_chunk_overlap_ms: number;
  transcribe_input_gain_db: number;
  capture_enabled: boolean;
  model_source: "default" | "custom";
  model_custom_url: string;
  overlay_color: string;
  overlay_min_radius: number;
  overlay_max_radius: number;
  overlay_rise_ms: number;
  overlay_fall_ms: number;
  overlay_opacity_inactive: number;
  overlay_opacity_active: number;
  overlay_pos_x: number;
  overlay_pos_y: number;
}

interface HistoryEntry {
  id: string;
  text: string;
  timestamp_ms: number;
  source: string;
}

interface AudioDevice {
  id: string;
  label: string;
}

interface ModelInfo {
  id: string;
  label: string;
  file_name: string;
  size_mb: number;
  installed: boolean;
  downloading: boolean;
  path?: string;
  source: string;
  available: boolean;
  download_url?: string;
  removable: boolean;
}

interface DownloadProgress {
  id: string;
  downloaded: number;
  total?: number;
}

interface DownloadComplete {
  id: string;
  path: string;
}

interface DownloadError {
  id: string;
  error: string;
}

interface ValidationResult {
  valid: boolean;
  error: string | null;
  formatted: string | null;
}

interface AppErrorType {
  type: "AudioDevice" | "Transcription" | "Hotkey" | "Storage" | "Network" | "Window" | "Other";
  message: string;
}

interface ErrorEvent {
  error: AppErrorType;
  timestamp: number;
  context?: string;
}

let settings: Settings | null = null;
let history: HistoryEntry[] = [];
let transcribeHistory: HistoryEntry[] = [];
let devices: AudioDevice[] = [];
let outputDevices: AudioDevice[] = [];
let models: ModelInfo[] = [];
const modelProgress = new Map<string, DownloadProgress>();
let currentStatus: "idle" | "recording" | "transcribing" = "idle";
let currentHistoryTab: "mic" | "system" | "conversation" = "mic";
let dynamicSustainThreshold: number = 0.01;

// Convert linear level (0-1) to dB (assuming 0dB = 1.0)
function levelToDb(level: number): number {
  if (level <= 0.00001) return -100;
  return 20 * Math.log10(level);
}

// Convert linear threshold (0-1) to percentage position on dB scale (-60 to 0)
function thresholdToPercent(threshold: number): number {
  const db = levelToDb(threshold);
  // Scale: -60dB = 0%, 0dB = 100%
  const percent = ((db + 60) / 60) * 100;
  return Math.max(0, Math.min(100, percent));
}

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;

const statusLabel = $("status-label");
const statusDot = $("status-dot") as HTMLSpanElement | null;
const statusMessage = $("status-message");
const engineLabel = $("engine-label");
const cloudState = $("cloud-state");
const modeState = $("mode-state");
const deviceState = $("device-state");
const modelState = $("model-state");
const modeSelect = $("mode-select") as HTMLSelectElement | null;
const pttHotkey = $("ptt-hotkey") as HTMLInputElement | null;
const pttHotkeyRecord = $("ptt-hotkey-record") as HTMLButtonElement | null;
const pttHotkeyStatus = $("ptt-hotkey-status") as HTMLSpanElement | null;
const toggleHotkey = $("toggle-hotkey") as HTMLInputElement | null;
const toggleHotkeyRecord = $("toggle-hotkey-record") as HTMLButtonElement | null;
const toggleHotkeyStatus = $("toggle-hotkey-status") as HTMLSpanElement | null;
const deviceSelect = $("device-select") as HTMLSelectElement | null;
const languageSelect = $("language-select") as HTMLSelectElement | null;
const cloudToggle = $("cloud-toggle") as HTMLInputElement | null;
const audioCuesToggle = $("audio-cues-toggle") as HTMLInputElement | null;
const audioCuesVolume = $("audio-cues-volume") as HTMLInputElement | null;
const pttUseVadToggle = $("ptt-use-vad-toggle") as HTMLInputElement | null;
const audioCuesVolumeValue = $("audio-cues-volume-value");
const hotkeysBlock = $("hotkeys-block");
const vadBlock = $("vad-block");
const vadThreshold = $("vad-threshold") as HTMLInputElement | null;
const vadThresholdValue = $("vad-threshold-value");
const vadSilence = $("vad-silence") as HTMLInputElement | null;
const vadSilenceValue = $("vad-silence-value");
const vadMeterFill = $("vad-meter-fill");
const vadLevelDbm = $("vad-level-dbm");
const vadMarkerStart = $("vad-marker-start");
const vadMarkerSustain = $("vad-marker-sustain");
const transcribeStatus = $("transcribe-status");
const transcribeHotkey = $("transcribe-hotkey") as HTMLInputElement | null;
const transcribeHotkeyRecord = $("transcribe-hotkey-record") as HTMLButtonElement | null;
const transcribeHotkeyStatus = $("transcribe-hotkey-status") as HTMLSpanElement | null;
const transcribeDeviceSelect = $("transcribe-device-select") as HTMLSelectElement | null;
const transcribeVadToggle = $("transcribe-vad-toggle") as HTMLInputElement | null;
const transcribeVadThreshold = $("transcribe-vad-threshold") as HTMLInputElement | null;
const transcribeVadThresholdValue = $("transcribe-vad-threshold-value");
const transcribeVadThresholdField = $("transcribe-vad-threshold-field");
const transcribeVadSilenceField = $("transcribe-vad-silence-field");
const transcribeVadSilence = $("transcribe-vad-silence") as HTMLInputElement | null;
const transcribeVadSilenceValue = $("transcribe-vad-silence-value");
const transcribeMeterFill = $("transcribe-meter-fill");
const transcribeMeterDb = $("transcribe-meter-db");
const transcribeMeterThreshold = $("transcribe-meter-threshold");
const transcribeThresholdDb = $("transcribe-threshold-db");
const transcribeThresholdLabel = $("transcribe-threshold-label");
const transcribeBatchField = $("transcribe-batch-field");
const transcribeBatchInterval = $("transcribe-batch-interval") as HTMLInputElement | null;
const transcribeBatchValue = $("transcribe-batch-value");
const transcribeOverlapField = $("transcribe-overlap-field");
const transcribeChunkOverlap = $("transcribe-chunk-overlap") as HTMLInputElement | null;
const transcribeOverlapValue = $("transcribe-overlap-value");
const transcribeGain = $("transcribe-gain") as HTMLInputElement | null;
const transcribeGainValue = $("transcribe-gain-value");
const overlayColor = $("overlay-color") as HTMLInputElement | null;
const overlayMinRadius = $("overlay-min-radius") as HTMLInputElement | null;
const overlayMinRadiusValue = $("overlay-min-radius-value");
const overlayMaxRadius = $("overlay-max-radius") as HTMLInputElement | null;
const overlayMaxRadiusValue = $("overlay-max-radius-value");
const overlayRise = $("overlay-rise") as HTMLInputElement | null;
const overlayRiseValue = $("overlay-rise-value");
const overlayFall = $("overlay-fall") as HTMLInputElement | null;
const overlayFallValue = $("overlay-fall-value");
const overlayOpacityInactive = $("overlay-opacity-inactive") as HTMLInputElement | null;
const overlayOpacityInactiveValue = $("overlay-opacity-inactive-value");
const overlayOpacityActive = $("overlay-opacity-active") as HTMLInputElement | null;
const overlayOpacityActiveValue = $("overlay-opacity-active-value");
const overlayPosX = $("overlay-pos-x") as HTMLInputElement | null;
const overlayPosY = $("overlay-pos-y") as HTMLInputElement | null;
const historyList = $("history-list");
const historyInput = $("history-input") as HTMLInputElement | null;
const historyAdd = $("history-add");
const historyCompose = document.querySelector(".history-compose") as HTMLDivElement | null;
const historyTabMic = $("history-tab-mic");
const historyTabSystem = $("history-tab-system");
const historyTabConversation = $("history-tab-conversation");
const historyCopyConversation = $("history-copy-conversation") as HTMLButtonElement | null;
const historyDetachConversation = $("history-detach-conversation") as HTMLButtonElement | null;
const conversationFontControls = $("conversation-font-controls");
const conversationFontSize = $("conversation-font-size") as HTMLInputElement | null;
const conversationFontSizeValue = $("conversation-font-size-value");
const modelSourceSelect = $("model-source-select") as HTMLSelectElement | null;
const modelCustomUrl = $("model-custom-url") as HTMLInputElement | null;
const modelCustomUrlField = $("model-custom-url-field") as HTMLDivElement | null;
const modelRefresh = $("model-refresh") as HTMLButtonElement | null;
const modelListInstalled = $("model-list-installed");
const modelListAvailable = $("model-list-available");

function setStatus(state: "idle" | "recording" | "transcribing") {
  currentStatus = state;
  if (statusDot) statusDot.dataset.state = state;
  if (!statusLabel) return;
  statusLabel.textContent =
    state === "idle" ? "Idle" : state === "recording" ? "Recording" : "Transcribing";
  if (statusMessage) statusMessage.textContent = "";
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp);
  const base = date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const hundredths = Math.floor(date.getMilliseconds() / 10)
    .toString()
    .padStart(2, "0");
  return `${base}.${hundredths}`;
}

function renderHero() {
  if (!settings) return;
  if (cloudState) cloudState.textContent = settings.cloud_fallback ? "On" : "Off";
  if (modeState) modeState.textContent = settings.mode === "ptt" ? "PTT" : "VAD";
  const device = devices.find((item) => item.id === settings?.input_device);
  if (deviceState) deviceState.textContent = device?.label ?? "Default";
  if (modelState) {
    const active = models.find((model) => model.id === settings?.model);
    modelState.textContent = active?.label ?? settings?.model ?? "—";
  }
  if (engineLabel) engineLabel.textContent = "whisper.cpp (GPU auto)";
  setStatus(currentStatus);
}

function renderSettings() {
  if (!settings) return;
  if (modeSelect) modeSelect.value = settings.mode;
  if (pttHotkey) pttHotkey.value = settings.hotkey_ptt;
  if (toggleHotkey) toggleHotkey.value = settings.hotkey_toggle;
  const hotkeysEnabled = settings.mode === "ptt";
  if (hotkeysBlock) hotkeysBlock.classList.toggle("hidden", !hotkeysEnabled);
  if (vadBlock) vadBlock.classList.toggle("hidden", hotkeysEnabled);
  if (deviceSelect) deviceSelect.value = settings.input_device;
  if (languageSelect) languageSelect.value = settings.language_mode;
  if (modelSourceSelect) modelSourceSelect.value = settings.model_source;
  if (modelCustomUrl) modelCustomUrl.value = settings.model_custom_url ?? "";
  if (modelCustomUrlField) {
    modelCustomUrlField.classList.toggle("hidden", settings.model_source !== "custom");
  }
  if (cloudToggle) cloudToggle.checked = settings.cloud_fallback;
  if (audioCuesToggle) audioCuesToggle.checked = settings.audio_cues;
  if (pttUseVadToggle) pttUseVadToggle.checked = settings.ptt_use_vad;
  if (audioCuesVolume) audioCuesVolume.value = Math.round(settings.audio_cues_volume * 100).toString();
  if (audioCuesVolumeValue) {
    audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
  }
  // Display start threshold in the slider (main user-facing threshold)
  if (vadThreshold) vadThreshold.value = Math.round(settings.vad_threshold_start * 100).toString();
  if (vadThresholdValue) vadThresholdValue.textContent = `${Math.round(settings.vad_threshold_start * 100)}%`;
  if (vadSilence) vadSilence.value = settings.vad_silence_ms.toString();
  if (vadSilenceValue) vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
  // Initialize dynamic sustain threshold from settings
  if (settings.vad_threshold_sustain > 0) {
    dynamicSustainThreshold = settings.vad_threshold_sustain;
  }
  // Update threshold markers on settings change
  updateThresholdMarkers();
  if (transcribeStatus && !transcribeStatus.textContent) {
    transcribeStatus.textContent = "Idle";
  }
  if (transcribeHotkey) transcribeHotkey.value = settings.transcribe_hotkey;
  if (transcribeDeviceSelect) transcribeDeviceSelect.value = settings.transcribe_output_device;
  if (transcribeVadToggle) transcribeVadToggle.checked = settings.transcribe_vad_mode;
  if (transcribeVadThreshold) {
    transcribeVadThreshold.value = Math.round(settings.transcribe_vad_threshold * 100).toString();
  }
  if (transcribeVadThresholdValue) {
    transcribeVadThresholdValue.textContent = `${Math.round(settings.transcribe_vad_threshold * 100)}%`;
  }
  if (transcribeVadSilence) {
    transcribeVadSilence.value = settings.transcribe_vad_silence_ms.toString();
  }
  if (transcribeVadSilenceValue) {
    transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
  }
  updateTranscribeThreshold(settings.transcribe_vad_threshold);
  updateTranscribeVadVisibility(settings.transcribe_vad_mode);
  if (transcribeBatchInterval) {
    transcribeBatchInterval.value = settings.transcribe_batch_interval_ms.toString();
  }
  if (transcribeBatchValue) {
    transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
  }
  if (transcribeChunkOverlap) {
    transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
  }
  if (transcribeOverlapValue) {
    transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
  }
  if (transcribeGain) {
    transcribeGain.value = Math.round(settings.transcribe_input_gain_db).toString();
  }
  if (transcribeGainValue) {
    const gain = Math.round(settings.transcribe_input_gain_db);
    transcribeGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
  }
  if (transcribeBatchField) {
    const disabled = settings.transcribe_vad_mode;
    transcribeBatchField.classList.toggle("is-disabled", disabled);
    transcribeBatchInterval?.toggleAttribute("disabled", disabled);
  }
  if (transcribeOverlapField) {
    const disabled = settings.transcribe_vad_mode;
    transcribeOverlapField.classList.toggle("is-disabled", disabled);
    transcribeChunkOverlap?.toggleAttribute("disabled", disabled);
  }
  if (transcribeVadThresholdField) {
    const disabled = !settings.transcribe_vad_mode;
    transcribeVadThresholdField.classList.toggle("is-disabled", disabled);
    transcribeVadThreshold?.toggleAttribute("disabled", disabled);
  }
  if (transcribeVadSilenceField) {
    const disabled = !settings.transcribe_vad_mode;
    transcribeVadSilenceField.classList.toggle("is-disabled", disabled);
    transcribeVadSilence?.toggleAttribute("disabled", disabled);
  }
  if (overlayColor) overlayColor.value = settings.overlay_color;
  if (overlayMinRadius) overlayMinRadius.value = Math.round(settings.overlay_min_radius).toString();
  if (overlayMinRadiusValue) overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
  if (overlayMaxRadius) overlayMaxRadius.value = Math.round(settings.overlay_max_radius).toString();
  if (overlayMaxRadiusValue) overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
  if (overlayRise) overlayRise.value = settings.overlay_rise_ms.toString();
  if (overlayRiseValue) overlayRiseValue.textContent = `${settings.overlay_rise_ms}`;
  if (overlayFall) overlayFall.value = settings.overlay_fall_ms.toString();
  if (overlayFallValue) overlayFallValue.textContent = `${settings.overlay_fall_ms}`;
  if (overlayOpacityInactive) {
    overlayOpacityInactive.value = Math.round(settings.overlay_opacity_inactive * 100).toString();
  }
  if (overlayOpacityInactiveValue) {
    overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_opacity_inactive * 100)}%`;
  }
  if (overlayOpacityActive) {
    overlayOpacityActive.value = Math.round(settings.overlay_opacity_active * 100).toString();
  }
  if (overlayOpacityActiveValue) {
    overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
  }
  if (overlayPosX) overlayPosX.value = Math.round(settings.overlay_pos_x).toString();
  if (overlayPosY) overlayPosY.value = Math.round(settings.overlay_pos_y).toString();
}

const TRANSCRIBE_DB_FLOOR = -60;

function updateTranscribeVadVisibility(enabled: boolean) {
  if (transcribeMeterThreshold) {
    transcribeMeterThreshold.style.display = enabled ? "block" : "none";
  }
  if (transcribeThresholdLabel) {
    transcribeThresholdLabel.style.display = enabled ? "block" : "none";
  }
}

function updateTranscribeThreshold(threshold: number) {
  const db = threshold <= 0.00001 ? TRANSCRIBE_DB_FLOOR : Math.max(TRANSCRIBE_DB_FLOOR, 20 * Math.log10(threshold));
  if (transcribeThresholdDb) {
    transcribeThresholdDb.textContent = `${db.toFixed(1)} dB`;
  }
  if (transcribeMeterThreshold) {
    const pos = (db - TRANSCRIBE_DB_FLOOR) / (0 - TRANSCRIBE_DB_FLOOR);
    transcribeMeterThreshold.style.left = `${Math.round(pos * 100)}%`;
  }
}

function renderDevices() {
  if (!deviceSelect) return;
  deviceSelect.innerHTML = "";
  devices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    deviceSelect.appendChild(option);
  });
}

function renderOutputDevices() {
  if (!transcribeDeviceSelect) return;
  transcribeDeviceSelect.innerHTML = "";
  outputDevices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    transcribeDeviceSelect.appendChild(option);
  });
}

function buildConversationHistory(): HistoryEntry[] {
  const combined = [...history, ...transcribeHistory];
  return combined.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
}

function buildConversationText(entries: HistoryEntry[]) {
  return entries
    .map((entry) => {
      const speaker = entry.source === "output" ? "System Audio" : "Microphone";
      return `[${formatTime(entry.timestamp_ms)}] ${speaker}: ${entry.text}`;
    })
    .join("\n");
}

function applyPanelCollapsed(panelId: string, collapsed: boolean) {
  const panel = document.querySelector(`[data-panel="${panelId}"]`) as HTMLElement | null;
  if (!panel) return;
  panel.classList.toggle("panel-collapsed", collapsed);
  localStorage.setItem(`panelCollapsed:${panelId}`, collapsed ? "1" : "0");
}

function initPanelState() {
  const panelIds = ["output", "capture", "system", "interface", "model"];
  panelIds.forEach((id) => {
    const collapsed = localStorage.getItem(`panelCollapsed:${id}`) === "1";
    applyPanelCollapsed(id, collapsed);
  });
}

function renderHistory() {
  if (!historyList) return;
  const dataset =
    currentHistoryTab === "mic"
      ? history
      : currentHistoryTab === "system"
        ? transcribeHistory
        : buildConversationHistory();

  if (historyCompose) {
    historyCompose.style.display = currentHistoryTab === "mic" ? "flex" : "none";
  }
  if (historyCopyConversation) {
    historyCopyConversation.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }
  if (historyDetachConversation) {
    historyDetachConversation.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }
  if (conversationFontControls) {
    conversationFontControls.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }

  if (!dataset.length) {
    const emptyMessage =
      currentHistoryTab === "mic"
        ? "Start dictating to build your microphone history."
        : currentHistoryTab === "system"
          ? "Start system audio capture to build your output history."
          : "Build microphone or system audio entries to generate the conversation view.";
    historyList.innerHTML =
      `<div class="history-item"><div><div class="history-text">No transcripts yet.</div><div class="history-meta">${emptyMessage}</div></div></div>`;
    return;
  }

  historyList.innerHTML = "";

  if (currentHistoryTab === "conversation") {
    const block = document.createElement("div");
    block.className = "conversation-block";
    block.textContent = buildConversationText(dataset);
    historyList.appendChild(block);
    return;
  }

  dataset.forEach((entry) => {
    const wrapper = document.createElement("div");
    wrapper.className = "history-item";

    const textWrap = document.createElement("div");
    textWrap.className = "history-content";
    const text = document.createElement("div");
    text.className = "history-text";
    text.textContent = entry.text;

    const meta = document.createElement("div");
    meta.className = "history-meta";
    if (currentHistoryTab === "conversation") {
      const speaker =
        entry.source === "output"
          ? "System Audio"
          : entry.source && entry.source !== "local"
            ? `Microphone (${entry.source})`
            : "Microphone";
      meta.textContent = `${formatTime(entry.timestamp_ms)} · ${speaker}`;
    } else {
      meta.textContent = `${formatTime(entry.timestamp_ms)} · ${entry.source}`;
    }

    textWrap.appendChild(text);
    textWrap.appendChild(meta);

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const copyButton = document.createElement("button");
    copyButton.textContent = "Copy";
    copyButton.addEventListener("click", async () => {
      await navigator.clipboard.writeText(entry.text);
    });

    actions.appendChild(copyButton);

    wrapper.appendChild(textWrap);
    wrapper.appendChild(actions);

    historyList.appendChild(wrapper);
  });
}

function setHistoryTab(tab: "mic" | "system" | "conversation") {
  currentHistoryTab = tab;
  if (historyTabMic) historyTabMic.classList.toggle("active", tab === "mic");
  if (historyTabSystem) historyTabSystem.classList.toggle("active", tab === "system");
  if (historyTabConversation) historyTabConversation.classList.toggle("active", tab === "conversation");
  renderHistory();
}

function formatSize(sizeMb: number) {
  if (sizeMb >= 1024) {
    return `${(sizeMb / 1024).toFixed(1)} GB`;
  }
  return `${sizeMb} MB`;
}

function formatProgress(progress?: DownloadProgress) {
  if (!progress) return "";
  if (progress.total && progress.total > 0) {
    const percent = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
    return `${percent}%`;
  }
  const mb = Math.round(progress.downloaded / (1024 * 1024));
  return `${mb} MB`;
}

function renderModels() {
  if (!modelListInstalled || !modelListAvailable) return;
  modelListInstalled.innerHTML = "";
  modelListAvailable.innerHTML = "";

  const installedModels = models.filter((model) => model.installed);
  const availableModels = models.filter((model) => !model.installed && model.available);

  if (settings && installedModels.length) {
    const hasActive = installedModels.some((model) => model.id === settings?.model);
    if (!hasActive) {
      settings.model = installedModels[0].id;
      persistSettings();
    }
  }

  const renderGroup = (container: HTMLElement, group: ModelInfo[], emptyText: string) => {
    if (!group.length) {
      container.innerHTML = `<div class="model-item"><div class="model-name">${emptyText}</div></div>`;
      return;
    }

    group.forEach((model) => {
      const item = document.createElement("div");
      item.className = "model-item";
      const isActive = settings?.model === model.id;
      if (isActive) {
        item.classList.add("selected");
      }
      if (model.installed) {
        item.classList.add("selectable");
        item.addEventListener("click", async () => {
          if (!settings) return;
          settings.model = model.id;
          await persistSettings();
          renderModels();
        });
      }

      const header = document.createElement("div");
      header.className = "model-header";

      const name = document.createElement("div");
      name.className = "model-name";
      name.textContent = model.label;

      const size = document.createElement("div");
      size.className = "model-size";
      size.textContent = model.size_mb > 0 ? formatSize(model.size_mb) : "Size unknown";

      header.appendChild(name);
      header.appendChild(size);

      const meta = document.createElement("div");
      meta.className = "model-meta";
      const source = model.source ? ` • ${model.source}` : "";
      meta.textContent = `${model.file_name}${source}`;

      const pathLine = document.createElement("div");
      pathLine.className = "model-meta";
      if (model.path) {
        pathLine.textContent = model.path;
      }

      const status = document.createElement("div");
      status.className = `model-status ${model.installed ? "downloaded" : "available"}${
        isActive ? " active" : ""
      }`;
      status.textContent = model.installed
        ? isActive
          ? "Active"
          : model.removable
            ? "Installed"
            : "Installed (external)"
        : model.downloading
          ? "Downloading"
          : "Available";

      const actions = document.createElement("div");
      actions.className = "model-actions";

      if (model.installed) {
        const removeBtn = document.createElement("button");
        removeBtn.textContent = model.removable ? "Remove" : "Locked";
        removeBtn.disabled = !model.removable;
        removeBtn.addEventListener("click", async (event) => {
          event.stopPropagation();
          if (!model.removable) return;
          try {
            await invoke("remove_model", { fileName: model.file_name });
            await refreshModels();
          } catch (error) {
            console.error("remove_model failed", error);
          }
        });
        actions.appendChild(removeBtn);
      } else {
        const button = document.createElement("button");
        button.textContent = model.downloading ? "Downloading..." : "Download";
        button.disabled = model.downloading;
        button.addEventListener("click", async (event) => {
          event.stopPropagation();
          try {
            if (!model.download_url) {
              console.error("No download URL for model", model.id);
              return;
            }
            await invoke("download_model", {
              modelId: model.id,
              downloadUrl: model.download_url,
              fileName: model.file_name,
            });
          } catch (error) {
            console.error("download_model failed", error);
          }
        });
        actions.appendChild(button);
      }

      const progress = document.createElement("div");
      progress.className = "model-progress";
      progress.textContent = formatProgress(modelProgress.get(model.id));

      item.appendChild(header);
      item.appendChild(meta);
      item.appendChild(status);
      if (model.path) {
        item.appendChild(pathLine);
      }
      item.appendChild(actions);
      item.appendChild(progress);

      container.appendChild(item);
    });
  };

  renderGroup(modelListInstalled, installedModels, "No installed models");
  renderGroup(modelListAvailable, availableModels, "No models available");
  renderHero();
}

async function refreshModels() {
  models = await invoke<ModelInfo[]>("list_models");
  renderModels();
}

async function persistSettings() {
  if (!settings) return;
  try {
    await invoke("save_settings", { settings });
  } catch (error) {
    console.error("save_settings failed", error);
  }
}

// Hotkey Recorder Setup
function setupHotkeyRecorder(
  type: "ptt" | "toggle" | "transcribe",
  input: HTMLInputElement | null,
  recordBtn: HTMLButtonElement | null,
  statusEl: HTMLSpanElement | null
) {
  if (!input || !recordBtn || !statusEl) return;

  let isRecording = false;
  let recordedKeys: Set<string> = new Set();

  const updateStatus = (message: string, type: "success" | "error" | "info") => {
    statusEl.textContent = message;
    statusEl.className = `hotkey-status ${type}`;
  };

  const validateHotkey = async (hotkey: string) => {
    try {
      const result = await invoke<ValidationResult>("validate_hotkey", { key: hotkey });

      if (result.valid) {
        input.classList.remove("invalid");
        input.classList.add("valid");
        updateStatus("✓ Valid hotkey", "success");
        return true;
      } else {
        input.classList.remove("valid");
        input.classList.add("invalid");
        updateStatus(result.error || "Invalid hotkey", "error");
        return false;
      }
    } catch (error) {
      input.classList.remove("valid");
      input.classList.add("invalid");
      updateStatus(`Error: ${error}`, "error");
      return false;
    }
  };

  const stopRecording = () => {
    isRecording = false;
    recordBtn.textContent = "🎹 Record";
    recordBtn.classList.remove("recording");
    input.classList.remove("recording");
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("keyup", handleKeyUp);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();

    // Add modifiers
    if (e.ctrlKey) recordedKeys.add("Ctrl");
    if (e.shiftKey) recordedKeys.add("Shift");
    if (e.altKey) recordedKeys.add("Alt");
    if (e.metaKey) recordedKeys.add("Command");

    // Add the actual key - use e.code for better reliability with special characters
    const isModifier = ["Control", "Shift", "Alt", "Meta"].includes(e.key);
    if (!isModifier) {
      // Use e.key for display (shows actual character like "^")
      // But handle special cases
      let keyName = e.key;

      // For single character keys, uppercase them
      if (keyName.length === 1) {
        keyName = keyName.toUpperCase();
      }

      recordedKeys.add(keyName);
    }

    // Display current combination
    const keysArray = Array.from(recordedKeys);
    const hotkeyString = keysArray.join("+");
    input.value = hotkeyString;
  };

  const handleKeyUp = async (e: KeyboardEvent) => {
    // When all keys are released, finalize the hotkey
    if (!e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey && recordedKeys.size > 1) {
      stopRecording();

      const hotkeyString = Array.from(recordedKeys).join("+");

      // Validate
      const isValid = await validateHotkey(hotkeyString);

      if (isValid && settings) {
        if (type === "ptt") {
          settings.hotkey_ptt = hotkeyString;
        } else if (type === "toggle") {
          settings.hotkey_toggle = hotkeyString;
        } else {
          settings.transcribe_hotkey = hotkeyString;
        }
        await persistSettings();
      }

      recordedKeys.clear();
    }
  };

  // Record button click
  recordBtn.addEventListener("click", () => {
    if (isRecording) {
      stopRecording();
      updateStatus("Recording cancelled", "info");
    } else {
      isRecording = true;
      recordedKeys.clear();
      recordBtn.textContent = "⏺ Recording...";
      recordBtn.classList.add("recording");
      input.classList.add("recording");
      input.value = "";
      updateStatus("Press your key combination...", "info");

      document.addEventListener("keydown", handleKeyDown);
      document.addEventListener("keyup", handleKeyUp);
    }
  });

  // Initial validation
  if (input.value.trim()) {
    validateHotkey(input.value.trim());
  }
}

function wireEvents() {
  modeSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.mode = modeSelect.value as Settings["mode"];
    await persistSettings();
    renderHero();
  });

  modelSourceSelect?.addEventListener("change", async () => {
    if (!settings || !modelSourceSelect) return;
    settings.model_source = modelSourceSelect.value as Settings["model_source"];
    await persistSettings();
    renderSettings();
    await refreshModels();
  });

  modelCustomUrl?.addEventListener("change", async () => {
    if (!settings || !modelCustomUrl) return;
    settings.model_custom_url = modelCustomUrl.value.trim();
    await persistSettings();
  });

  modelRefresh?.addEventListener("click", async () => {
    if (!settings) return;
    if (modelCustomUrl) {
      settings.model_custom_url = modelCustomUrl.value.trim();
    }
    await persistSettings();
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

  historyTabMic?.addEventListener("click", () => setHistoryTab("mic"));
  historyTabSystem?.addEventListener("click", () => setHistoryTab("system"));
  historyTabConversation?.addEventListener("click", () => setHistoryTab("conversation"));

  historyCopyConversation?.addEventListener("click", async () => {
    const entries = buildConversationHistory();
    if (!entries.length) return;
    const transcript = buildConversationText(entries);
    await navigator.clipboard.writeText(transcript);
  });

  historyDetachConversation?.addEventListener("click", async () => {
    await invoke("open_conversation_window");
  });

  conversationFontSize?.addEventListener("input", () => {
    if (!conversationFontSize) return;
    const size = Number(conversationFontSize.value);
    document.documentElement.style.setProperty("--conversation-font-size", `${size}px`);
    if (conversationFontSizeValue) {
      conversationFontSizeValue.textContent = `${size}px`;
    }
    localStorage.setItem("conversationFontSize", size.toString());
  });

  // Hotkey recording functionality
  setupHotkeyRecorder("ptt", pttHotkey, pttHotkeyRecord, pttHotkeyStatus);
  setupHotkeyRecorder("toggle", toggleHotkey, toggleHotkeyRecord, toggleHotkeyStatus);
  setupHotkeyRecorder("transcribe", transcribeHotkey, transcribeHotkeyRecord, transcribeHotkeyStatus);

  deviceSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.input_device = deviceSelect.value;
    await persistSettings();
    renderHero();
  });

  transcribeDeviceSelect?.addEventListener("change", async () => {
    if (!settings || !transcribeDeviceSelect) return;
    settings.transcribe_output_device = transcribeDeviceSelect.value;
    await persistSettings();
  });

  transcribeVadToggle?.addEventListener("change", async () => {
    if (!settings || !transcribeVadToggle) return;
    settings.transcribe_vad_mode = transcribeVadToggle.checked;
    if (transcribeBatchField) {
      const disabled = settings.transcribe_vad_mode;
      transcribeBatchField.classList.toggle("is-disabled", disabled);
      transcribeBatchInterval?.toggleAttribute("disabled", disabled);
    }
    if (transcribeOverlapField) {
      const disabled = settings.transcribe_vad_mode;
      transcribeOverlapField.classList.toggle("is-disabled", disabled);
      transcribeChunkOverlap?.toggleAttribute("disabled", disabled);
    }
    if (transcribeVadThresholdField) {
      const disabled = !settings.transcribe_vad_mode;
      transcribeVadThresholdField.classList.toggle("is-disabled", disabled);
      transcribeVadThreshold?.toggleAttribute("disabled", disabled);
    }
    if (transcribeVadSilenceField) {
      const disabled = !settings.transcribe_vad_mode;
      transcribeVadSilenceField.classList.toggle("is-disabled", disabled);
      transcribeVadSilence?.toggleAttribute("disabled", disabled);
    }
    updateTranscribeVadVisibility(settings.transcribe_vad_mode);
    await persistSettings();
  });

  transcribeVadThreshold?.addEventListener("input", () => {
    if (!settings || !transcribeVadThreshold) return;
    const value = Number(transcribeVadThreshold.value);
    settings.transcribe_vad_threshold = Math.min(1, Math.max(0, value / 100));
    if (transcribeVadThresholdValue) {
      transcribeVadThresholdValue.textContent = `${Math.round(settings.transcribe_vad_threshold * 100)}%`;
    }
    updateTranscribeThreshold(settings.transcribe_vad_threshold);
  });

  transcribeVadThreshold?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  transcribeVadSilence?.addEventListener("input", () => {
    if (!settings || !transcribeVadSilence) return;
    const value = Number(transcribeVadSilence.value);
    settings.transcribe_vad_silence_ms = Math.max(200, Math.min(5000, value));
    if (transcribeVadSilenceValue) {
      transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
    }
  });

  transcribeVadSilence?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  transcribeBatchInterval?.addEventListener("input", () => {
    if (!settings || !transcribeBatchInterval) return;
    const value = Number(transcribeBatchInterval.value);
    settings.transcribe_batch_interval_ms = Math.max(4000, Math.min(15000, value));
    if (transcribeBatchValue) {
      transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
    }
  });

  transcribeBatchInterval?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  transcribeChunkOverlap?.addEventListener("input", () => {
    if (!settings || !transcribeChunkOverlap) return;
    const value = Number(transcribeChunkOverlap.value);
    settings.transcribe_chunk_overlap_ms = Math.max(0, Math.min(3000, value));
    if (settings.transcribe_chunk_overlap_ms > settings.transcribe_batch_interval_ms) {
      settings.transcribe_chunk_overlap_ms = Math.floor(settings.transcribe_batch_interval_ms / 2);
      transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
    }
    if (transcribeOverlapValue) {
      transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
    }
  });

  transcribeChunkOverlap?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  transcribeGain?.addEventListener("input", () => {
    if (!settings || !transcribeGain) return;
    const value = Number(transcribeGain.value);
    settings.transcribe_input_gain_db = Math.max(0, Math.min(24, value));
    if (transcribeGainValue) {
      const gain = Math.round(settings.transcribe_input_gain_db);
      transcribeGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
    }
  });

  transcribeGain?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  languageSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_mode = languageSelect.value as Settings["language_mode"];
    await persistSettings();
  });

  cloudToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.cloud_fallback = cloudToggle.checked;
    await persistSettings();
    renderHero();
  });

  audioCuesToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.audio_cues = audioCuesToggle.checked;
    await persistSettings();
  });

  pttUseVadToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.ptt_use_vad = pttUseVadToggle.checked;
    await persistSettings();
  });

  audioCuesVolume?.addEventListener("input", () => {
    if (!settings || !audioCuesVolume) return;
    const value = Number(audioCuesVolume.value);
    settings.audio_cues_volume = Math.min(1, Math.max(0, value / 100));
    if (audioCuesVolumeValue) {
      audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
    }
  });

  audioCuesVolume?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  vadThreshold?.addEventListener("input", () => {
    if (!settings || !vadThreshold) return;
    const value = Number(vadThreshold.value);
    const threshold = Math.min(1, Math.max(0, value / 100));

    // Update the start threshold (main threshold)
    settings.vad_threshold_start = threshold;
    // Keep legacy field in sync
    settings.vad_threshold = threshold;

    if (vadThresholdValue) {
      vadThresholdValue.textContent = `${Math.round(threshold * 100)}%`;
    }

    // Update threshold markers
    updateThresholdMarkers();
  });

  vadThreshold?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  vadSilence?.addEventListener("input", () => {
    if (!settings || !vadSilence) return;
    const value = Math.max(200, Math.min(4000, Number(vadSilence.value)));
    settings.vad_silence_ms = value;
    if (vadSilenceValue) {
      vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
    }
  });

  vadSilence?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayColor?.addEventListener("input", () => {
    if (!settings || !overlayColor) return;
    settings.overlay_color = overlayColor.value;
  });

  overlayColor?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayMinRadius?.addEventListener("input", () => {
    if (!settings || !overlayMinRadius || !overlayMaxRadius) return;
    settings.overlay_min_radius = Number(overlayMinRadius.value);
    if (settings.overlay_min_radius > settings.overlay_max_radius) {
      settings.overlay_max_radius = settings.overlay_min_radius;
      overlayMaxRadius.value = Math.round(settings.overlay_max_radius).toString();
    }
    if (overlayMinRadiusValue) {
      overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (overlayMaxRadiusValue) {
      overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
  });

  overlayMinRadius?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayMaxRadius?.addEventListener("input", () => {
    if (!settings || !overlayMaxRadius || !overlayMinRadius) return;
    settings.overlay_max_radius = Number(overlayMaxRadius.value);
    if (settings.overlay_max_radius < settings.overlay_min_radius) {
      settings.overlay_min_radius = settings.overlay_max_radius;
      overlayMinRadius.value = Math.round(settings.overlay_min_radius).toString();
    }
    if (overlayMinRadiusValue) {
      overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (overlayMaxRadiusValue) {
      overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
  });

  overlayMaxRadius?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayRise?.addEventListener("input", () => {
    if (!settings || !overlayRise) return;
    settings.overlay_rise_ms = Number(overlayRise.value);
    if (overlayRiseValue) overlayRiseValue.textContent = `${settings.overlay_rise_ms}`;
  });

  overlayRise?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayFall?.addEventListener("input", () => {
    if (!settings || !overlayFall) return;
    settings.overlay_fall_ms = Number(overlayFall.value);
    if (overlayFallValue) overlayFallValue.textContent = `${settings.overlay_fall_ms}`;
  });

  overlayFall?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayOpacityInactive?.addEventListener("input", () => {
    if (!settings || !overlayOpacityInactive || !overlayOpacityActive) return;
    const value = Math.min(1, Math.max(0.05, Number(overlayOpacityInactive.value) / 100));
    settings.overlay_opacity_inactive = value;
    if (settings.overlay_opacity_active < settings.overlay_opacity_inactive) {
      settings.overlay_opacity_active = settings.overlay_opacity_inactive;
      overlayOpacityActive.value = Math.round(settings.overlay_opacity_active * 100).toString();
    }
    if (overlayOpacityInactiveValue) {
      overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_opacity_inactive * 100)}%`;
    }
    if (overlayOpacityActiveValue) {
      overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
    }
  });

  overlayOpacityInactive?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayOpacityActive?.addEventListener("input", () => {
    if (!settings || !overlayOpacityActive || !overlayOpacityInactive) return;
    const value = Math.min(1, Math.max(settings.overlay_opacity_inactive, Number(overlayOpacityActive.value) / 100));
    settings.overlay_opacity_active = value;
    if (overlayOpacityActiveValue) {
      overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
    }
  });

  overlayOpacityActive?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  overlayPosX?.addEventListener("change", async () => {
    if (!settings || !overlayPosX) return;
    settings.overlay_pos_x = Number(overlayPosX.value);
    await persistSettings();
  });

  overlayPosY?.addEventListener("change", async () => {
    if (!settings || !overlayPosY) return;
    settings.overlay_pos_y = Number(overlayPosY.value);
    await persistSettings();
  });

  // Apply Overlay Settings button
  const applyOverlayBtn = document.getElementById("apply-overlay-btn");
  applyOverlayBtn?.addEventListener("click", async () => {
    if (!settings) return;
    await persistSettings();
    showToast({ title: "Applied", message: "Overlay settings applied", type: "success" });
  });

  historyAdd?.addEventListener("click", async () => {
    if (!historyInput?.value.trim()) return;
    history = await invoke("add_history_entry", {
      text: historyInput.value.trim(),
      source: settings?.cloud_fallback ? "cloud" : "local",
    });
    historyInput.value = "";
    renderHistory();
  });

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
  if (conversationFontSize) {
    conversationFontSize.value = size.toString();
  }
  if (conversationFontSizeValue) {
    conversationFontSizeValue.textContent = `${size}px`;
  }
}

// Toast Notification System
type ToastType = "error" | "success" | "warning" | "info";

interface ToastOptions {
  type?: ToastType;
  title: string;
  message: string;
  duration?: number;
  icon?: string;
}

let toastCounter = 0;

function showToast(options: ToastOptions) {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const id = `toast-${++toastCounter}`;
  const type = options.type || "info";
  const duration = options.duration || 5000;

  const icons: Record<ToastType, string> = {
    error: "❌",
    success: "✅",
    warning: "⚠️",
    info: "ℹ️",
  };

  const icon = options.icon || icons[type];

  const toast = document.createElement("div");
  toast.id = id;
  toast.className = `toast ${type}`;
  toast.innerHTML = `
    <span class="toast-icon">${icon}</span>
    <div class="toast-content">
      <div class="toast-title">${options.title}</div>
      <div class="toast-message">${options.message}</div>
    </div>
    <button class="toast-close" title="Close">×</button>
  `;

  const closeBtn = toast.querySelector(".toast-close");
  closeBtn?.addEventListener("click", () => removeToast(id));

  container.appendChild(toast);

  // Auto-remove after duration
  if (duration > 0) {
    setTimeout(() => removeToast(id), duration);
  }
}

function removeToast(id: string) {
  const toast = document.getElementById(id);
  if (!toast) return;

  toast.classList.add("removing");

  setTimeout(() => {
    toast.remove();
  }, 200);
}

function showErrorToast(error: AppErrorType, context?: string) {
  const typeMapping: Record<string, string> = {
    AudioDevice: "Audio Device Issue",
    Transcription: "Transcription Failed",
    Hotkey: "Hotkey Problem",
    Storage: "Storage Error",
    Network: "Network Problem",
    Window: "Window Error",
    Other: "Error",
  };

  showToast({
    type: "error",
    title: typeMapping[error.type] || "Error",
    message: context ? `${context}: ${error.message}` : error.message,
    duration: 7000,
  });
}

// Audio cue playback using Web Audio API
let audioContext: AudioContext | null = null;

function playAudioCue(type: "start" | "stop") {
  try {
    // Initialize AudioContext lazily (requires user interaction first)
    if (!audioContext) {
      audioContext = new AudioContext();
    }

    const now = audioContext.currentTime;
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    // Different frequencies for start and stop
    if (type === "start") {
      // Rising beep: 600Hz -> 800Hz
      oscillator.frequency.setValueAtTime(600, now);
      oscillator.frequency.linearRampToValueAtTime(800, now + 0.1);
    } else {
      // Falling beep: 800Hz -> 600Hz
      oscillator.frequency.setValueAtTime(800, now);
      oscillator.frequency.linearRampToValueAtTime(600, now + 0.1);
    }

    // Quick fade in/out
    const volume = settings?.audio_cues_volume ?? 0.3;
    const target = Math.max(0, Math.min(1, volume));
    gainNode.gain.setValueAtTime(0, now);
    gainNode.gain.linearRampToValueAtTime(target, now + 0.01);
    gainNode.gain.linearRampToValueAtTime(0, now + 0.1);

    oscillator.start(now);
    oscillator.stop(now + 0.1);
  } catch (error) {
    console.error("Failed to play audio cue:", error);
  }
}

async function bootstrap() {
  settings = await invoke<Settings>("get_settings");
  devices = await invoke<AudioDevice[]>("list_audio_devices");
  outputDevices = await invoke<AudioDevice[]>("list_output_devices");
  history = await invoke<HistoryEntry[]>("get_history");
  transcribeHistory = await invoke<HistoryEntry[]>("get_transcribe_history");
  models = await invoke<ModelInfo[]>("list_models");

  renderDevices();
  renderOutputDevices();
  renderSettings();
  renderHero();
  setStatus("idle");
  renderHistory();
  renderModels();
  wireEvents();
  initPanelState();
  initConversationView();

  await listen<Settings>("settings-changed", (event) => {
    settings = event.payload ?? settings;
    renderSettings();
    renderHero();
    renderModels();
  });

  await listen<string>("capture:state", (event) => {
    const state = event.payload as "idle" | "recording" | "transcribing";
    setStatus(state ?? "idle");
  });

  await listen<string>("transcribe:state", (event) => {
    if (transcribeStatus) {
      const state = event.payload;
      transcribeStatus.textContent =
        state === "recording" ? "Monitoring" : state === "transcribing" ? "Transcribing" : "Idle";
    }
  });

  await listen<number>("transcribe:level", (event) => {
    if (!transcribeMeterFill) return;
    const level = Math.max(0, Math.min(1, event.payload ?? 0));
    transcribeMeterFill.style.width = `${Math.round(level * 100)}%`;
  });

  await listen<number>("transcribe:db", (event) => {
    if (!transcribeMeterDb) return;
    const value = event.payload ?? -60;
    const clamped = Math.max(-60, Math.min(0, value));
    transcribeMeterDb.textContent = `${clamped.toFixed(1)} dB`;
  });

  await listen<HistoryEntry[]>("history:updated", (event) => {
    history = event.payload ?? history;
    renderHistory();
  });

  await listen<HistoryEntry[]>("transcribe:history-updated", (event) => {
    transcribeHistory = event.payload ?? transcribeHistory;
    renderHistory();
  });

  await listen<{ text: string; source: string }>("transcription:result", () => {
    if (statusMessage) statusMessage.textContent = "";
  });

  await listen<DownloadProgress>("model:download-progress", (event) => {
    modelProgress.set(event.payload.id, event.payload);
    models = models.map((model) =>
      model.id === event.payload.id ? { ...model, downloading: true } : model
    );
    renderModels();
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
    setStatus("idle");
    if (statusMessage) statusMessage.textContent = `Error: ${event.payload}`;

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
    if (settings?.audio_cues) {
      playAudioCue(type);
    }
  });

  await listen<number>("audio:level", (event) => {
    if (!vadMeterFill) return;
    const level = Math.max(0, Math.min(1, event.payload ?? 0));
    // Convert to dB scale for display (-60dB to 0dB)
    const db = levelToDb(level);
    const percent = thresholdToPercent(level);
    vadMeterFill.style.width = `${percent}%`;

    // Update dBm display
    if (vadLevelDbm) {
      if (db <= -60) {
        vadLevelDbm.textContent = "-∞ dB";
      } else {
        vadLevelDbm.textContent = `${db.toFixed(0)} dB`;
      }
    }
  });

  // Listen for dynamic sustain threshold updates from backend
  await listen<number>("vad:dynamic-threshold", (event) => {
    dynamicSustainThreshold = event.payload ?? 0.01;
    updateThresholdMarkers();
  });
}

// Update threshold marker positions
function updateThresholdMarkers() {
  if (vadMarkerStart && settings) {
    const startPercent = thresholdToPercent(settings.vad_threshold_start);
    vadMarkerStart.style.left = `${startPercent}%`;
  }
  if (vadMarkerSustain) {
    const sustainPercent = thresholdToPercent(dynamicSustainThreshold);
    vadMarkerSustain.style.left = `${sustainPercent}%`;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap().catch((error) => {
    console.error("bootstrap failed", error);
  });
});
