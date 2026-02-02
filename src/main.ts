import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Settings {
  mode: "ptt" | "toggle";
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
const simulateButton = $("simulate-transcribe");
const modeSelect = $("mode-select") as HTMLSelectElement | null;
const pttHotkey = $("ptt-hotkey") as HTMLInputElement | null;
const toggleHotkey = $("toggle-hotkey") as HTMLInputElement | null;
const deviceSelect = $("device-select") as HTMLSelectElement | null;
const modelSelect = $("model-select") as HTMLSelectElement | null;
const languageSelect = $("language-select") as HTMLSelectElement | null;
const cloudToggle = $("cloud-toggle") as HTMLInputElement | null;
const historyList = $("history-list");
const historyInput = $("history-input") as HTMLInputElement | null;
const historyAdd = $("history-add");
const modelList = $("model-list");

const defaultSettings: Settings = {
  mode: "ptt",
  hotkey_ptt: "CommandOrControl+Shift+Space",
  hotkey_toggle: "CommandOrControl+Shift+M",
  input_device: "default",
  language_mode: "auto",
  model: "whisper-large-v3",
  cloud_fallback: false,
};

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
  if (modeState) modeState.textContent = settings.mode === "ptt" ? "PTT" : "Toggle";
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
  if (deviceSelect) deviceSelect.value = settings.input_device;
  if (modelSelect) modelSelect.value = settings.model;
  if (languageSelect) languageSelect.value = settings.language_mode;
  if (cloudToggle) cloudToggle.checked = settings.cloud_fallback;
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

function wireEvents() {
  modeSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.mode = modeSelect.value as Settings["mode"];
    await persistSettings();
    renderHero();
  });

  pttHotkey?.addEventListener("change", async () => {
    if (!settings) return;
    settings.hotkey_ptt = pttHotkey.value.trim() || defaultSettings.hotkey_ptt;
    await persistSettings();
  });

  toggleHotkey?.addEventListener("change", async () => {
    if (!settings) return;
    settings.hotkey_toggle = toggleHotkey.value.trim() || defaultSettings.hotkey_toggle;
    await persistSettings();
  });

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

  historyAdd?.addEventListener("click", async () => {
    if (!historyInput?.value.trim()) return;
    history = await invoke("add_history_entry", {
      text: historyInput.value.trim(),
      source: settings?.cloud_fallback ? "cloud" : "local",
    });
    historyInput.value = "";
    renderHistory();
  });

  simulateButton?.addEventListener("click", async () => {
    const stamp = new Date().toLocaleTimeString();
    const sample = `Simulated transcript (${stamp})`;
    history = await invoke("add_history_entry", {
      text: sample,
      source: settings?.cloud_fallback ? "cloud" : "local",
    });
    renderHistory();
  });
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
  });
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap().catch((error) => {
    console.error("bootstrap failed", error);
  });
});
