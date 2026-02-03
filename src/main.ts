import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Settings {
  mode: "ptt" | "vad";
  hotkey_ptt: string;
  hotkey_toggle: string;
  input_device: string;
  language_mode: "auto";
  model:
    | "whisper-tiny"
    | "whisper-base"
    | "whisper-small"
    | "whisper-medium"
    | "whisper-large-v3"
    | "whisper-large-v3-turbo";
  cloud_fallback: boolean;
  audio_cues: boolean;
  audio_cues_volume: number;
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
let devices: AudioDevice[] = [];
let models: ModelInfo[] = [];
const modelProgress = new Map<string, DownloadProgress>();
let currentStatus: "idle" | "recording" | "transcribing" = "idle";

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;

const statusLabel = $("status-label");
const statusDot = $("status-dot") as HTMLSpanElement | null;
const statusMessage = $("status-message");
const engineLabel = $("engine-label");
const cloudState = $("cloud-state");
const modeState = $("mode-state");
const deviceState = $("device-state");
const modeSelect = $("mode-select") as HTMLSelectElement | null;
const pttHotkey = $("ptt-hotkey") as HTMLInputElement | null;
const pttHotkeyRecord = $("ptt-hotkey-record") as HTMLButtonElement | null;
const pttHotkeyStatus = $("ptt-hotkey-status") as HTMLSpanElement | null;
const toggleHotkey = $("toggle-hotkey") as HTMLInputElement | null;
const toggleHotkeyRecord = $("toggle-hotkey-record") as HTMLButtonElement | null;
const toggleHotkeyStatus = $("toggle-hotkey-status") as HTMLSpanElement | null;
const deviceSelect = $("device-select") as HTMLSelectElement | null;
const modelSelect = $("model-select") as HTMLSelectElement | null;
const languageSelect = $("language-select") as HTMLSelectElement | null;
const cloudToggle = $("cloud-toggle") as HTMLInputElement | null;
const audioCuesToggle = $("audio-cues-toggle") as HTMLInputElement | null;
const audioCuesVolume = $("audio-cues-volume") as HTMLInputElement | null;
const audioCuesVolumeValue = $("audio-cues-volume-value");
const historyList = $("history-list");
const historyInput = $("history-input") as HTMLInputElement | null;
const historyAdd = $("history-add");
const modelList = $("model-list");

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
  return date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function renderHero() {
  if (!settings) return;
  if (cloudState) cloudState.textContent = settings.cloud_fallback ? "On" : "Off";
  if (modeState) modeState.textContent = settings.mode === "ptt" ? "PTT" : "VAD";
  const device = devices.find((item) => item.id === settings?.input_device);
  if (deviceState) deviceState.textContent = device?.label ?? "Default";
  if (engineLabel) engineLabel.textContent = "whisper.cpp (GPU auto)";
  setStatus(currentStatus);
}

function renderSettings() {
  if (!settings) return;
  if (modeSelect) modeSelect.value = settings.mode;
  if (pttHotkey) pttHotkey.value = settings.hotkey_ptt;
  if (toggleHotkey) toggleHotkey.value = settings.hotkey_toggle;
  const hotkeysEnabled = settings.mode === "ptt";
  if (pttHotkey) pttHotkey.disabled = !hotkeysEnabled;
  if (pttHotkeyRecord) pttHotkeyRecord.disabled = !hotkeysEnabled;
  if (toggleHotkey) toggleHotkey.disabled = !hotkeysEnabled;
  if (toggleHotkeyRecord) toggleHotkeyRecord.disabled = !hotkeysEnabled;
  if (deviceSelect) deviceSelect.value = settings.input_device;
  if (modelSelect) modelSelect.value = settings.model;
  if (languageSelect) languageSelect.value = settings.language_mode;
  if (cloudToggle) cloudToggle.checked = settings.cloud_fallback;
  if (audioCuesToggle) audioCuesToggle.checked = settings.audio_cues;
  if (audioCuesVolume) audioCuesVolume.value = Math.round(settings.audio_cues_volume * 100).toString();
  if (audioCuesVolumeValue) {
    audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
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

function renderHistory() {
  if (!historyList) return;
  if (!history.length) {
    historyList.innerHTML =
      "<div class=\"history-item\"><div><div class=\"history-text\">No transcripts yet.</div><div class=\"history-meta\">Start dictating to build your history.</div></div></div>";
    return;
  }

  historyList.innerHTML = "";
  history.forEach((entry) => {
    const wrapper = document.createElement("div");
    wrapper.className = "history-item";

    const textWrap = document.createElement("div");
    const text = document.createElement("div");
    text.className = "history-text";
    text.textContent = entry.text;

    const meta = document.createElement("div");
    meta.className = "history-meta";
    meta.textContent = `${formatTime(entry.timestamp_ms)} · ${entry.source}`;

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
  if (!modelList) return;
  modelList.innerHTML = "";

  if (!models.length) {
    modelList.innerHTML =
      "<div class=\"model-card\"><div class=\"model-title\">No models available</div><div class=\"model-meta\">Check your configuration.</div></div>";
    return;
  }

  models.forEach((model) => {
    const card = document.createElement("div");
    card.className = "model-card";
    card.dataset.selected = settings?.model === model.id ? "true" : "false";

    const title = document.createElement("div");
    title.className = "model-title";
    title.textContent = model.label;

    const meta = document.createElement("div");
    meta.className = "model-meta";
    meta.textContent = `${formatSize(model.size_mb)} • ${model.file_name}`;

    const badge = document.createElement("div");
    badge.className = "model-badge";
    badge.textContent = model.installed ? "Installed" : model.downloading ? "Downloading" : "Not installed";

    const actions = document.createElement("div");
    actions.className = "model-actions";

    const button = document.createElement("button");
    button.textContent = model.installed ? "Installed" : model.downloading ? "Downloading..." : "Download";
    button.disabled = model.installed || model.downloading;
    button.addEventListener("click", async () => {
      try {
        await invoke("download_model", { modelId: model.id });
      } catch (error) {
        console.error("download_model failed", error);
      }
    });

    const progress = document.createElement("div");
    progress.className = "model-progress";
    progress.textContent = formatProgress(modelProgress.get(model.id));

    actions.appendChild(button);
    actions.appendChild(progress);

    card.appendChild(title);
    card.appendChild(meta);
    card.appendChild(badge);
    card.appendChild(actions);

    modelList.appendChild(card);
  });
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
  type: "ptt" | "toggle",
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
        // Save to settings
        if (type === "ptt") {
          settings.hotkey_ptt = hotkeyString;
        } else {
          settings.hotkey_toggle = hotkeyString;
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

  // Hotkey recording functionality
  setupHotkeyRecorder("ptt", pttHotkey, pttHotkeyRecord, pttHotkeyStatus);
  setupHotkeyRecorder("toggle", toggleHotkey, toggleHotkeyRecord, toggleHotkeyStatus);

  deviceSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.input_device = deviceSelect.value;
    await persistSettings();
    renderHero();
  });

  modelSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.model = modelSelect.value as Settings["model"];
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
  history = await invoke<HistoryEntry[]>("get_history");
  models = await invoke<ModelInfo[]>("list_models");

  renderDevices();
  renderSettings();
  renderHero();
  setStatus("idle");
  renderHistory();
  renderModels();
  wireEvents();

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

  await listen<HistoryEntry[]>("history:updated", (event) => {
    history = event.payload ?? history;
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
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap().catch((error) => {
    console.error("bootstrap failed", error);
  });
});
