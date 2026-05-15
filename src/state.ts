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

export const ASSISTANT_CORE_MODULE_ID = "assistant_core";
export const ASSISTANT_PRESENCE_MODULE_ID = "assistant_presence";
export const LEGACY_WORKFLOW_AGENT_MODULE_ID = "workflow_agent";

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

export function normalizeEnabledModuleIds(enabledModules: string[] | undefined): string[] {
  const normalized = new Set<string>();
  for (const rawId of enabledModules ?? []) {
    const moduleId = rawId === LEGACY_WORKFLOW_AGENT_MODULE_ID ? ASSISTANT_CORE_MODULE_ID : rawId;
    if (!moduleId) continue;
    normalized.add(moduleId);
  }
  return Array.from(normalized);
}

export function normalizeAssistantSettings(newSettings: Settings | null): Settings | null {
  if (!newSettings) return null;
  const moduleSettings = newSettings.module_settings;
  if (moduleSettings) {
    moduleSettings.enabled_modules = normalizeEnabledModuleIds(moduleSettings.enabled_modules);

    const remappedPermissions: Record<string, string[]> = {};
    for (const [moduleId, permissions] of Object.entries(moduleSettings.consented_permissions ?? {})) {
      const nextId = moduleId === LEGACY_WORKFLOW_AGENT_MODULE_ID ? ASSISTANT_CORE_MODULE_ID : moduleId;
      const current = remappedPermissions[nextId] ?? [];
      remappedPermissions[nextId] = Array.from(new Set([...current, ...(permissions ?? [])]));
    }
    moduleSettings.consented_permissions = remappedPermissions;

    const remappedOverrides: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(moduleSettings.module_overrides ?? {})) {
      const nextKey = key.startsWith(`${LEGACY_WORKFLOW_AGENT_MODULE_ID}.`)
        ? `${ASSISTANT_CORE_MODULE_ID}.${key.slice(LEGACY_WORKFLOW_AGENT_MODULE_ID.length + 1)}`
        : key;
      remappedOverrides[nextKey] = value;
    }
    moduleSettings.module_overrides = remappedOverrides;
  }

  newSettings.workflow_agent ??= {
    enabled: false,
    wakewords: ["trispr", "hey trispr", "trispr agent"],
    wakeword_aliases: [],
    intent_keywords: { gdd_generate_publish: ["gdd"] },
    model: "qwen3.5:4b",
    temperature: 0.2,
    max_tokens: 512,
    session_gap_minutes: 20,
    max_candidates: 3,
    hands_free_enabled: false,
    confirm_timeout_sec: 45,
    reply_mode: "rule_only",
    online_enabled: false,
    voice_feedback_enabled: false,
    activation_mode: "hotkey_first",
    trusted_action_allowlist: [],
    expert_yolo_enabled: false,
  };
  newSettings.workflow_agent.activation_mode ??= "hotkey_first";
  newSettings.workflow_agent.trusted_action_allowlist ??= [];
  newSettings.workflow_agent.expert_yolo_enabled ??= false;
  newSettings.assistant_presence_enabled ??= true;
  newSettings.assistant_presence_pinned ??= true;

  const assistantCoreEnabled = isAssistantCoreAvailable(newSettings);
  if (!assistantCoreEnabled && newSettings.product_mode === "assistant") {
    newSettings.product_mode = "transcribe";
  }

  return newSettings;
}

// State setters
export function setSettings(newSettings: Settings | null) {
  settings = normalizeAssistantSettings(newSettings);
}

export function isAssistantCoreModuleEnabled(current: Settings | null = settings): boolean {
  return current?.module_settings?.enabled_modules?.includes(ASSISTANT_CORE_MODULE_ID) ?? false;
}

export function isAssistantPresenceModuleEnabled(current: Settings | null = settings): boolean {
  return current?.module_settings?.enabled_modules?.includes(ASSISTANT_PRESENCE_MODULE_ID) ?? false;
}

export function isAssistantCoreAvailable(current: Settings | null = settings): boolean {
  return isAssistantCoreModuleEnabled(current) && Boolean(current?.workflow_agent?.enabled);
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
    normalizeAssistantSettings(settings);
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
