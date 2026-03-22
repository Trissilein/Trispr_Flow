// Global application state
import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  QuantizeProgress,
  RecordingState,
  HistoryTab,
  OllamaPullProgress,
  OverlayHealthEvent,
  RuntimeDiagnostics,
  StartupStatus,
} from "./types";

export let settings: Settings | null = null;
export let history: HistoryEntry[] = [];
export let transcribeHistory: HistoryEntry[] = [];
// Runtime session anchor used by export range "session" (resets on app restart).
export const appRuntimeStartedMs = Date.now();
export let devices: AudioDevice[] = [];
export let outputDevices: AudioDevice[] = [];
export let models: ModelInfo[] = [];
export const modelProgress = new Map<string, DownloadProgress>();
export const quantizeProgress = new Map<string, QuantizeProgress>();
export const ollamaPullProgress = new Map<string, OllamaPullProgress>();
export let currentCaptureStatus: RecordingState = "idle";
export let currentTranscribeStatus: RecordingState = "idle";
export let currentHistoryTab: HistoryTab = "mic";
export let dynamicSustainThreshold: number = 0.01;
export let startupStatus: StartupStatus | null = null;
export let runtimeDiagnostics: RuntimeDiagnostics | null = null;
export let overlayHealth: OverlayHealthEvent | null = null;

// State setters
export function setSettings(newSettings: Settings | null) {
  settings = newSettings;
}

/** Returns true only when AI refinement is explicitly enabled in settings. */
export function isRefinementEnabled(): boolean {
  const moduleEnabled = settings?.module_settings?.enabled_modules?.includes("ai_refinement") ?? false;
  return moduleEnabled && settings?.ai_fallback?.enabled === true;
}

export function updateSettings(newSettings: Partial<Settings>) {
  if (settings) {
    // Deep merge for nested objects like setup and ai_fallback
    if (newSettings.setup) {
      settings.setup = { ...settings.setup, ...newSettings.setup };
    }
    if (newSettings.ai_fallback) {
      settings.ai_fallback = { ...settings.ai_fallback, ...newSettings.ai_fallback };
    }
    
    // Merge the rest
    Object.assign(settings, { ...newSettings, setup: settings.setup, ai_fallback: settings.ai_fallback });
  }
}

export function setHistory(newHistory: HistoryEntry[]) {
  history = newHistory;
}

export function setTranscribeHistory(newHistory: HistoryEntry[]) {
  transcribeHistory = newHistory;
}

export function setDevices(newDevices: AudioDevice[]) {
  devices = newDevices;
}

export function setOutputDevices(newDevices: AudioDevice[]) {
  outputDevices = newDevices;
}

export function setModels(newModels: ModelInfo[]) {
  models = newModels;
}

export function setCurrentCaptureStatus(status: RecordingState) {
  currentCaptureStatus = status;
}

export function setCurrentTranscribeStatus(status: RecordingState) {
  currentTranscribeStatus = status;
}

export function setCurrentHistoryTab(tab: HistoryTab) {
  currentHistoryTab = tab;
}

export function setDynamicSustainThreshold(threshold: number) {
  dynamicSustainThreshold = threshold;
}

export function setStartupStatus(status: StartupStatus | null) {
  startupStatus = status;
}

export function setRuntimeDiagnostics(diagnostics: RuntimeDiagnostics | null) {
  runtimeDiagnostics = diagnostics;
}

export function setOverlayHealth(health: OverlayHealthEvent | null) {
  overlayHealth = health;
}

// Model descriptions constant
export const MODEL_DESCRIPTIONS: Record<
  string,
  { summary: string; speed: string; accuracy: string; languages: string }
> = {
  "whisper-large-v3": {
    summary: "Best overall quality. Largest model with highest accuracy.",
    speed: "Slowest",
    accuracy: "Highest",
    languages: "Multilingual",
  },
  "whisper-large-v3-turbo": {
    summary: "Speed-optimized large model with strong accuracy.",
    speed: "Very fast",
    accuracy: "High",
    languages: "Multilingual",
  },
  "ggml-distil-large-v3": {
    summary: "Distilled variant focused on speed with near‑large quality.",
    speed: "Fastest",
    accuracy: "High (EN‑focused)",
    languages: "Primarily English",
  },
  "ggml-large-v3-turbo-q5_0": {
    summary: "Quantized large-v3-turbo for smaller size and faster runtime.",
    speed: "Fast",
    accuracy: "Slightly lower",
    languages: "Multilingual",
  },
  "ggml-large-v3-turbo-q8_0": {
    summary: "Q8 quantized large-v3-turbo with higher fidelity than q5 at larger size.",
    speed: "Moderate",
    accuracy: "High",
    languages: "Multilingual",
  },
  "ggml-large-v3-q5_0": {
    summary: "Quantized large-v3 for balanced quality and size reduction.",
    speed: "Moderately slow",
    accuracy: "Slightly lower than full",
    languages: "Multilingual",
  },
  "ggml-large-v3-q8_0": {
    summary: "Q8 quantized large-v3 prioritizing quality with moderate size reduction.",
    speed: "Slow",
    accuracy: "High",
    languages: "Multilingual",
  },
  "ggml-distil-large-v3-q5_0": {
    summary: "Quantized distil model for ultra-fast inference with minimal accuracy loss.",
    speed: "Fastest",
    accuracy: "Medium (EN-focused)",
    languages: "Primarily English",
  },
  "ggml-distil-large-v3-q8_0": {
    summary: "Q8 quantized distil model with better wording stability than q5.",
    speed: "Fast",
    accuracy: "Medium-high (EN-focused)",
    languages: "Primarily English",
  },
  "ggml-large-v3-turbo-german-q5_0": {
    summary: "Quantized German-optimized model for efficient German speech recognition.",
    speed: "Fast",
    accuracy: "Slightly lower",
    languages: "German-optimized",
  },
  "ggml-large-v3-turbo-german-q8_0": {
    summary: "Q8 German-optimized model for higher quality German speech recognition.",
    speed: "Moderate",
    accuracy: "High",
    languages: "German-optimized",
  },
};
