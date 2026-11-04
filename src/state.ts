// Global application state
import type {
  Settings,
  HistoryEntry,
  AudioDevice,
  ModelInfo,
  DownloadProgress,
  QuantizeProgress,
  HistoryTab,
  OllamaPullProgress,
  OverlayHealthEvent,
  RuntimeDiagnostics,
  StartupStatus,
} from "./types";

// Module ids that no longer exist. Old settings.json files may still mention them.
export const REMOVED_MODULE_IDS = new Set([
  "analysis",
  "assistant_core",
  "assistant_presence",
  "gdd",
  "input_vision",
  "integrations_confluence",
  "output_video_generation",
  "output_voice_tts",
  "piper_tts",
  "task_capture",
  "workflow_agent",
]);

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
export let currentHistoryTab: HistoryTab = "mic";
export let dynamicSustainThreshold: number = 0.01;
export let startupStatus: StartupStatus | null = null;
export let runtimeDiagnostics: RuntimeDiagnostics | null = null;
export let overlayHealth: OverlayHealthEvent | null = null;

export function normalizeEnabledModuleIds(enabledModules: string[] | undefined): string[] {
  const normalized = new Set<string>();
  for (const moduleId of enabledModules ?? []) {
    if (!moduleId || REMOVED_MODULE_IDS.has(moduleId)) continue;
    normalized.add(moduleId);
  }
  return Array.from(normalized);
}

function isRemovedModuleKey(key: string): boolean {
  for (const moduleId of REMOVED_MODULE_IDS) {
    if (key === moduleId || key.startsWith(`${moduleId}.`)) return true;
  }
  return false;
}

export function normalizeSettings(newSettings: Settings | null): Settings | null {
  if (!newSettings) return null;
  const moduleSettings = newSettings.module_settings;
  if (moduleSettings) {
    moduleSettings.enabled_modules = normalizeEnabledModuleIds(moduleSettings.enabled_modules);
    moduleSettings.consented_permissions = Object.fromEntries(
      Object.entries(moduleSettings.consented_permissions ?? {}).filter(([key]) => !isRemovedModuleKey(key)),
    );
    moduleSettings.module_overrides = Object.fromEntries(
      Object.entries(moduleSettings.module_overrides ?? {}).filter(([key]) => !isRemovedModuleKey(key)),
    );
  }
  newSettings.diagnostic_logging_enabled ??= false;
  return newSettings;
}

// State setters
export function setSettings(newSettings: Settings | null) {
  settings = normalizeSettings(newSettings);
}

/** Returns true only when AI refinement is explicitly enabled in settings. */
export function isRefinementEnabled(): boolean {
  const moduleEnabled = settings?.module_settings?.enabled_modules?.includes("ai_refinement") ?? false;
  return moduleEnabled && settings?.ai_fallback?.enabled === true;
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
