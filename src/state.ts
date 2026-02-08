// Global application state
import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  RecordingState,
  HistoryTab,
} from "./types";

export let settings: Settings | null = null;
export let history: HistoryEntry[] = [];
export let transcribeHistory: HistoryEntry[] = [];
export let devices: AudioDevice[] = [];
export let outputDevices: AudioDevice[] = [];
export let models: ModelInfo[] = [];
export const modelProgress = new Map<string, DownloadProgress>();
export let currentCaptureStatus: RecordingState = "idle";
export let currentTranscribeStatus: RecordingState = "idle";
export let currentHistoryTab: HistoryTab = "mic";
export let dynamicSustainThreshold: number = 0.01;

// State setters
export function setSettings(newSettings: Settings | null) {
  settings = newSettings;
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
};
