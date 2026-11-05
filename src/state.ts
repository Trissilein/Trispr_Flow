// Global application state
import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  QuantizeProgress,
  OllamaPullProgress,
  OverlayHealthEvent,
  RuntimeDiagnostics,
  StartupStatus,
} from "./types";

export let settings: Settings | null = null;
export let history: HistoryEntry[] = [];
// Runtime session anchor used by export range "session" (resets on app restart).
export const appRuntimeStartedMs = Date.now();
export let devices: AudioDevice[] = [];
export let models: ModelInfo[] = [];
export const modelProgress = new Map<string, DownloadProgress>();
export const quantizeProgress = new Map<string, QuantizeProgress>();
export const ollamaPullProgress = new Map<string, OllamaPullProgress>();
export let dynamicSustainThreshold: number = 0.01;
export let startupStatus: StartupStatus | null = null;
export let runtimeDiagnostics: RuntimeDiagnostics | null = null;
export let overlayHealth: OverlayHealthEvent | null = null;

export function normalizeSettings(newSettings: Settings | null): Settings | null {
  if (!newSettings) return null;
  newSettings.diagnostic_logging_enabled ??= false;
  return newSettings;
}

// State setters
export function setSettings(newSettings: Settings | null) {
  settings = normalizeSettings(newSettings);
}

/** Returns true only when AI refinement is explicitly enabled in settings. */
export function isRefinementEnabled(): boolean {
  return settings?.ai_fallback?.enabled === true;
}

export function isDiagnosticLoggingEnabled(current: Settings | null = settings): boolean {
  return current?.diagnostic_logging_enabled === true;
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
    normalizeSettings(settings);
  }
}

export function setHistory(newHistory: HistoryEntry[]) {
  history = newHistory;
}

export function setDevices(newDevices: AudioDevice[]) {
  devices = newDevices;
}

export function setModels(newModels: ModelInfo[]) {
  models = newModels;
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
