// DOM event listeners setup

import { invoke } from "@tauri-apps/api/core";
import type {
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthStatus,
  Settings,
  TtsSpeakResult,
} from "./types";
import {
  CLOUD_PROVIDER_IDS,
  CLOUD_PROVIDER_LABELS,
  normalizeCloudProvider,
  normalizeExecutionMode,
  normalizeAuthMethodPreference,
  isVerifiedAuthStatus,
  normalizeAIFallbackProvider,
} from "./ai-provider-utils";
import { settings, currentHistoryTab, history, transcribeHistory } from "./state";
import * as dom from "./dom-refs";
import {
  persistSettings,
  updateOverlayStyleVisibility,
  applyOverlaySharedUi,
  updateTranscribeVadVisibility,
  updateTranscribeThreshold,
  renderAIFallbackSettingsUi,
  renderTopicKeywords,
  ensureContinuousDumpDefaults,
  resolveEffectiveAsrLanguageHint,
  derivePostprocLanguageFromAsr,
  syncCaptureModeVisibility,
  syncDerivedLanguageSettings,
  refreshProviderAvailability,
  refreshProviderVoices,
  updateProviderMutualExclusion,
} from "./settings";
import { renderSettings } from "./settings";
import { renderHero, updateDeviceLineClamp, updateThresholdMarkers } from "./ui-state";
import { refreshModels, refreshModelsDir } from "./models";
import { setHistoryTab, buildConversationHistory, buildConversationText, setSearchQuery, setTopicKeywords, DEFAULT_TOPICS, scheduleHistoryRender, syncHistoryToolbarState } from "./history";
import { setHistoryAlias, setHistoryFontSize, syncHistoryAliasesIntoSettings } from "./history-preferences";
import { isPanelId, togglePanel } from "./panels";
import { setupHotkeyRecorder, initHotkeyStatusListener } from "./hotkeys";
import { updateRangeAria } from "./accessibility";
import { showToast } from "./toast";
import { dbToLevel, VAD_DB_FLOOR } from "./ui-helpers";
import { applyAccentColor, DEFAULT_ACCENT_COLOR } from "./utils";
import {
  DEFAULT_REFINEMENT_PROMPT_PRESET,
  findUserRefinementPromptPresetByOptionId,
  NEW_REFINEMENT_PROMPT_OPTION_ID,
  normalizeActiveRefinementPromptPresetId,
  normalizeRefinementPromptPreset,
  normalizeUserRefinementPromptPresets,
  resolveEffectiveRefinementPrompt,
  toUserRefinementPromptOptionId,
} from "./refinement-prompts";
import {
  autoStartLocalRuntimeIfNeeded,
  ensureLocalRuntimeReady,
  fetchOnlineVersionCatalog,
  getOllamaRuntimeCardState,
  importOllamaModelFromLocalFile,
  refreshOllamaInstalledModels,
  refreshOllamaRuntimeVersionCatalog,
  refreshOllamaRuntimeAndModels,
  refreshOllamaRuntimeState,
  renderOllamaModelManager,
  showOllamaRequiredModal,
  startOllamaRuntime,
  useManagedOllamaRuntime,
  useSystemOllamaRuntime,
  verifyOllamaRuntime,
} from "./ollama-models";
import { openExportDialog } from "./export-dialog";
import { openArchiveBrowser } from "./archive-browser";
import { normalizeModelTag } from "./ollama-tag-utils";
import { syncWorkflowAgentConsoleState } from "./workflow-agent-console";

// Cleanup registry for window-level listeners added by wireEvents()
const _windowCleanups: Array<() => void> = [];
export function cleanupWindowListeners(): void {
  _windowCleanups.forEach((fn) => fn());
  _windowCleanups.length = 0;
}

// RAF guard: ensures renderSettings() is called at most once per animation frame
// even when multiple settings toggles fire synchronously in one tick.
let _settingsRenderFrame: number | null = null;

export function scheduleSettingsRender(): void {
  if (_settingsRenderFrame !== null) return;
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    _settingsRenderFrame = window.requestAnimationFrame(() => {
      _settingsRenderFrame = null;
      renderSettings();
    });
  } else {
    _settingsRenderFrame = window.setTimeout(() => {
      _settingsRenderFrame = null;
      renderSettings();
    }, 16) as unknown as number;
  }
}

let authModalProvider: CloudAIFallbackProvider | null = null;

// Renders the three UI sections that always need to be refreshed together after
// a local/online runtime change or model-import action.
function refreshAIUi(): void {
  renderAIFallbackSettingsUi();
  renderOllamaModelManager();
  renderHero();
}

function formatTtsTestError(error: unknown): string {
  const raw = String(error ?? "Unknown error");
  const normalized = raw.replace(/^Error:\s*/i, "").trim();
  return normalized;
}

function getCredentialTargetProvider(): CloudAIFallbackProvider | null {
  if (authModalProvider) {
    return authModalProvider;
  }
  return normalizeCloudProvider(settings?.ai_fallback?.fallback_provider ?? null);
}

function getFallbackProvider(): CloudAIFallbackProvider | null {
  return normalizeCloudProvider(settings?.ai_fallback?.fallback_provider ?? null);
}

function isProviderVerified(provider: CloudAIFallbackProvider | null): boolean {
  if (!provider || !settings) return false;
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return false;
  return isVerifiedAuthStatus(providerSettings.auth_status);
}

const LOCAL_BACKENDS = ["ollama", "lm_studio", "oobabooga"] as const;
const AI_REFINEMENT_MODULE_ID = "ai_refinement";
const AI_REFINEMENT_MIGRATION_FLAG_KEY = "ai_refinement.migrated_legacy";

function isModuleEnabled(moduleId: string): boolean {
  return settings?.module_settings?.enabled_modules?.includes(moduleId) ?? false;
}

function isAiRefinementModuleEnabled(): boolean {
  return isModuleEnabled(AI_REFINEMENT_MODULE_ID);
}

function normalizeAiRefinementModuleBindingInSettings(): void {
  if (!settings) return;
  settings.module_settings ??= {
    enabled_modules: [],
    consented_permissions: {},
    module_overrides: {},
  };
  settings.module_settings.enabled_modules ??= [];
  settings.module_settings.consented_permissions ??= {};
  settings.module_settings.module_overrides ??= {};
  settings.module_settings.enabled_modules = Array.from(
    new Set(settings.module_settings.enabled_modules)
  );

  const overrides = settings.module_settings.module_overrides;
  const migrationDone = overrides[AI_REFINEMENT_MIGRATION_FLAG_KEY] === true;
  if (
    settings.ai_fallback.enabled
    && !settings.module_settings.enabled_modules.includes(AI_REFINEMENT_MODULE_ID)
    && !migrationDone
  ) {
    settings.module_settings.enabled_modules.push(AI_REFINEMENT_MODULE_ID);
  }

  const moduleEnabledNow = settings.module_settings.enabled_modules.includes(AI_REFINEMENT_MODULE_ID);
  if (!moduleEnabledNow) {
    settings.ai_fallback.enabled = false;
    settings.postproc_llm_enabled = false;
  }
  overrides[AI_REFINEMENT_MIGRATION_FLAG_KEY] = true;
}

function applyExecutionModeInSettings(mode: AIExecutionMode): void {
  if (!settings) return;
  settings.ai_fallback.execution_mode = "local_primary";
  // Preserve the currently selected local backend — do NOT reset to "ollama"
  if (!LOCAL_BACKENDS.includes(settings.ai_fallback.provider as typeof LOCAL_BACKENDS[number])) {
    settings.ai_fallback.provider = "ollama";
  }
  settings.postproc_llm_provider = settings.ai_fallback.provider;
  if (mode === "online_fallback") {
    settings.ai_fallback.execution_mode = "local_primary";
  }
}

function ensureOnlineModeConstraints(notify: boolean): boolean {
  if (!settings) return false;
  const fallbackProvider = getFallbackProvider();
  if (
    settings.ai_fallback.execution_mode === "online_fallback" &&
    (!fallbackProvider || !isProviderVerified(fallbackProvider))
  ) {
    applyExecutionModeInSettings("local_primary");
    if (notify) {
      showToast({
        type: "warning",
        title: "Fallback switched to local",
        message: "Fallback provider is locked/unverified. Switched back to local Ollama.",
        duration: 3800,
      });
    }
    return true;
  }
  return false;
}

function isLaneControlTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest("button,select,input,textarea,summary,details,label,a")
  );
}

function getAIFallbackProviderSettings(provider: AIFallbackProvider) {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  // Ollama uses OllamaSettings, handled separately
  return null;
}

function cloneDefaultTopicKeywords(): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  Object.entries(DEFAULT_TOPICS).forEach(([topic, words]) => {
    out[topic] = [...words];
  });
  return out;
}

function normalizeTopicKeywordsInput(
  input: Record<string, unknown> | null | undefined
): Record<string, string[]> {
  const fallback = cloneDefaultTopicKeywords();
  if (!input || typeof input !== "object") return fallback;

  const normalized: Record<string, string[]> = {};
  Object.entries(input).forEach(([topic, words]) => {
    const key = topic.trim().toLowerCase();
    if (!key || !Array.isArray(words)) return;
    const cleaned = words
      .map((word) => String(word).trim().toLowerCase())
      .filter((word) => word.length > 0);
    if (cleaned.length === 0) return;
    normalized[key] = Array.from(new Set(cleaned));
  });

  if (Object.keys(normalized).length === 0) return fallback;
  Object.entries(DEFAULT_TOPICS).forEach(([topic, defaults]) => {
    if (!normalized[topic] || normalized[topic].length === 0) {
      normalized[topic] = [...defaults];
    }
  });
  return normalized;
}

function refreshResolvedRefinementPromptInSettings() {
  if (!settings) return;
  settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
    settings.ai_fallback.prompt_profile,
    resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
    settings.ai_fallback.custom_prompt,
    settings.ai_fallback.preserve_source_language
  );
}

function syncActivePromptPresetSelection() {
  if (!settings) return;
  const ai = settings.ai_fallback;
  ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
  ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    ai.active_prompt_preset_id,
    ai.prompt_profile,
    ai.prompt_presets
  );
  if (ai.active_prompt_preset_id === NEW_REFINEMENT_PROMPT_OPTION_ID) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
    ai.use_default_prompt = false;
    return;
  }
  const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
    ai.prompt_presets,
    ai.active_prompt_preset_id
  );
  if (selectedUserPreset) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
    ai.custom_prompt = selectedUserPreset.prompt;
  } else {
    ai.prompt_profile = normalizeRefinementPromptPreset(ai.active_prompt_preset_id);
    ai.custom_prompt_enabled = ai.prompt_profile === "custom";
  }
  ai.use_default_prompt = false;
}

function createUserPromptPresetId(baseName: string, existingIds: Set<string>): string {
  const slug = baseName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "preset";
  let candidate = slug;
  let suffix = 1;
  while (existingIds.has(candidate)) {
    candidate = `${slug}-${suffix}`;
    suffix += 1;
  }
  return candidate;
}

function applyPendingUserPresetEditsFromEditor(): boolean {
  if (!settings || !dom.aiFallbackCustomPrompt || !dom.aiFallbackPromptPresetName) {
    return false;
  }
  const ai = settings.ai_fallback;
  ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
  ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    ai.active_prompt_preset_id,
    ai.prompt_profile,
    ai.prompt_presets
  );
  const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
    ai.prompt_presets,
    ai.active_prompt_preset_id
  );
  if (!selectedUserPreset) return false;

  const nextPrompt = dom.aiFallbackCustomPrompt.value.trim();
  const nextName = dom.aiFallbackPromptPresetName.value.trim() || selectedUserPreset.name;
  if (!nextPrompt) {
    return false;
  }
  if (nextPrompt === selectedUserPreset.prompt && nextName === selectedUserPreset.name) {
    return false;
  }

  ai.prompt_presets = ai.prompt_presets.map((preset) =>
    preset.id === selectedUserPreset.id
      ? { ...preset, name: nextName, prompt: nextPrompt }
      : preset
  );
  ai.active_prompt_preset_id = toUserRefinementPromptOptionId(selectedUserPreset.id);
  syncActivePromptPresetSelection();
  refreshResolvedRefinementPromptInSettings();
  return true;
}

function ensureAIFallbackSettingsDefaults() {
  if (!settings) return;
  if (!settings.ai_fallback) {
    settings.ai_fallback = {
      enabled: false,
      provider: "ollama",
      fallback_provider: null,
      execution_mode: "local_primary",
      strict_local_mode: true,
      preserve_source_language: true,
      model: "",
      temperature: 0.3,
      max_tokens: 4000,
      low_latency_mode: false,
      prompt_profile: DEFAULT_REFINEMENT_PROMPT_PRESET,
      custom_prompt_enabled: false,
      custom_prompt:
        "Fix this transcribed text: correct punctuation, capitalization, and obvious errors. Keep the meaning unchanged. Return only the corrected text.",
      use_default_prompt: true,
      prompt_presets: [],
      active_prompt_preset_id: DEFAULT_REFINEMENT_PROMPT_PRESET,
    };
  }
  if (!settings.providers) {
    settings.providers = {
      claude: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022", "claude-3-opus-20240229"],
        preferred_model: "claude-3-5-sonnet-20241022",
      },
      openai: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini"],
        preferred_model: "gpt-4o-mini",
      },
      gemini: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
        preferred_model: "gemini-2.0-flash",
      },
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "manual",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.17.7",
        last_health_check: null,
      },
    };
  }
  if (!settings.providers.ollama) {
    settings.providers.ollama = {
      endpoint: "http://127.0.0.1:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      runtime_target_version: "0.17.7",
      last_health_check: null,
    };
  }
  settings.ai_fallback.strict_local_mode ??= true;
  settings.ai_fallback.preserve_source_language ??= true;
  settings.ai_fallback.prompt_profile = normalizeRefinementPromptPreset(settings.ai_fallback.prompt_profile);
  settings.ai_fallback.prompt_presets = normalizeUserRefinementPromptPresets(
    settings.ai_fallback.prompt_presets
  );
  settings.ai_fallback.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    settings.ai_fallback.active_prompt_preset_id,
    settings.ai_fallback.prompt_profile,
    settings.ai_fallback.prompt_presets
  );
  settings.ai_fallback.low_latency_mode ??= false;
  syncActivePromptPresetSelection();
  settings.ai_fallback.fallback_provider = normalizeCloudProvider(
    settings.ai_fallback.fallback_provider ?? null
  );
  settings.ai_fallback.execution_mode = normalizeExecutionMode(settings.ai_fallback.execution_mode);
  if (!settings.ai_fallback.fallback_provider && !LOCAL_BACKENDS.includes(settings.ai_fallback.provider as typeof LOCAL_BACKENDS[number])) {
    settings.ai_fallback.fallback_provider = normalizeCloudProvider(settings.ai_fallback.provider);
  }
  // Online fallback lane is intentionally roadmap-only for now.
  settings.ai_fallback.execution_mode = "local_primary";
  // Preserve the selected local backend — only reset if something invalid crept in
  if (!LOCAL_BACKENDS.includes(settings.ai_fallback.provider as typeof LOCAL_BACKENDS[number])) {
    settings.ai_fallback.provider = "ollama";
  }
  settings.postproc_llm_provider = settings.ai_fallback.provider;
  settings.postproc_language = derivePostprocLanguageFromAsr(
    settings.language_mode,
    settings.language_pinned
  );
  const effectiveLanguageHint = resolveEffectiveAsrLanguageHint(
    settings.language_mode,
    settings.language_pinned
  );
  settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
    settings.ai_fallback.prompt_profile,
    effectiveLanguageHint,
    settings.ai_fallback.custom_prompt,
    settings.ai_fallback.preserve_source_language
  );
  settings.topic_keywords = normalizeTopicKeywordsInput(settings.topic_keywords);
  setTopicKeywords(settings.topic_keywords);
  settings.providers.ollama.runtime_source ??= "manual";
  settings.providers.ollama.runtime_path ??= "";
  settings.providers.ollama.runtime_version ??= "";
  settings.providers.ollama.runtime_target_version ??= "0.17.7";
  settings.providers.ollama.last_health_check ??= null;
  CLOUD_PROVIDER_IDS.forEach((provider) => {
    const providerSettings = getAIFallbackProviderSettings(provider);
    if (!providerSettings) return;
    providerSettings.auth_method_preference = normalizeAuthMethodPreference(
      providerSettings.auth_method_preference
    );
    providerSettings.auth_status = isVerifiedAuthStatus(providerSettings.auth_status)
      ? (providerSettings.auth_status as AIProviderAuthStatus)
      : "locked";
    providerSettings.auth_verified_at ??= null;
    if (!providerSettings.api_key_stored && providerSettings.auth_status !== "verified_oauth") {
      providerSettings.auth_status = "locked";
      providerSettings.auth_verified_at = null;
    }
  });
  settings.setup ??= {
    local_ai_wizard_completed: false,
    local_ai_wizard_pending: true,
    ollama_remote_expert_opt_in: false,
  };
  settings.setup.ollama_remote_expert_opt_in ??= false;
  settings.product_mode = settings.product_mode === "assistant" ? "assistant" : "transcribe";
  normalizeAiRefinementModuleBindingInSettings();
  settings.gdd_module_settings ??= {
    enabled: false,
    default_preset_id: "universal_strict",
    detect_preset_automatically: true,
    prefer_one_click_publish: false,
    workflow_mode_default: "standard",
    transcript_source_default: "runtime_session",
    target_routing_strategy: "hybrid_memory",
    one_click_confidence_threshold: 0.75,
    preset_clones: [],
  };
  settings.gdd_module_settings.default_preset_id ??= "universal_strict";
  settings.gdd_module_settings.detect_preset_automatically ??= true;
  settings.gdd_module_settings.prefer_one_click_publish ??= false;
  settings.gdd_module_settings.workflow_mode_default =
    settings.gdd_module_settings.workflow_mode_default === "advanced"
      ? "advanced"
      : "standard";
  settings.gdd_module_settings.transcript_source_default = "runtime_session";
  settings.gdd_module_settings.target_routing_strategy =
    settings.gdd_module_settings.target_routing_strategy === "fixed"
      ? "fixed"
      : settings.gdd_module_settings.target_routing_strategy === "fresh_suggest"
        ? "fresh_suggest"
        : "hybrid_memory";
  {
    const threshold = Number(settings.gdd_module_settings.one_click_confidence_threshold);
    settings.gdd_module_settings.one_click_confidence_threshold =
      Number.isFinite(threshold) && threshold >= 0 && threshold <= 1 ? threshold : 0.75;
  }
  settings.gdd_module_settings.preset_clones ??= [];
  settings.confluence_settings ??= {
    enabled: false,
    site_base_url: "",
    oauth_cloud_id: "",
    default_space_key: "",
    api_user_email: "",
    default_parent_page_id: "",
    auth_mode: "oauth",
    routing_memory: {},
  };
  settings.confluence_settings.enabled ??= false;
  settings.confluence_settings.site_base_url ??= "";
  settings.confluence_settings.oauth_cloud_id ??= "";
  settings.confluence_settings.default_space_key ??= "";
  settings.confluence_settings.api_user_email ??= "";
  settings.confluence_settings.default_parent_page_id ??= "";
  settings.confluence_settings.auth_mode =
    settings.confluence_settings.auth_mode === "api_token" ? "api_token" : "oauth";
  settings.confluence_settings.routing_memory ??= {};
  syncDerivedLanguageSettings();
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

  if (provider === "ollama") {
    ensureAIFallbackSettingsDefaults();
    const mergedModels = Array.from(
      new Set(
        [...(settings.providers.ollama.available_models ?? []), ...models]
          .map((name) => normalizeModelTag(name))
          .filter((name) => name.length > 0)
      )
    );
    settings.providers.ollama.available_models = mergedModels;
    if (!models.includes(settings.providers.ollama.preferred_model)) {
      settings.providers.ollama.preferred_model = models[0] ?? "";
    }
    if (settings.ai_fallback.provider === "ollama" && !models.includes(settings.ai_fallback.model)) {
      settings.ai_fallback.model = settings.providers.ollama.preferred_model || models[0] || "";
    }
    return;
  }

  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return;
  providerSettings.available_models = models;
  if (!providerSettings.preferred_model || !models.includes(providerSettings.preferred_model)) {
    providerSettings.preferred_model = models[0] ?? "";
  }
  const mode = normalizeExecutionMode(settings.ai_fallback.execution_mode);
  const fallbackProvider = getFallbackProvider();
  if (
    settings.ai_fallback.provider === provider ||
    (mode === "online_fallback" && fallbackProvider === provider)
  ) {
    if (!models.includes(settings.ai_fallback.model)) {
      settings.ai_fallback.model = providerSettings.preferred_model || models[0] || "";
    }
  }
}

function setAuthModalOpen(open: boolean): void {
  if (!dom.aiAuthModal) return;
  dom.aiAuthModal.hidden = !open;
  dom.aiAuthModal.classList.toggle("is-open", open);
}

function refreshAuthModalContent(): void {
  const provider = authModalProvider;
  if (!provider) return;
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return;

  const providerLabel = CLOUD_PROVIDER_LABELS[provider];
  if (dom.aiAuthProviderName) {
    dom.aiAuthProviderName.textContent = `${providerLabel} credentials`;
  }
  if (dom.aiAuthMethod) {
    dom.aiAuthMethod.value = normalizeAuthMethodPreference(providerSettings.auth_method_preference);
  }
  if (dom.aiAuthVerifyKey) {
    dom.aiAuthVerifyKey.disabled =
      normalizeAuthMethodPreference(providerSettings.auth_method_preference) === "oauth";
  }
  if (dom.aiAuthStatus) {
    if (normalizeAuthMethodPreference(providerSettings.auth_method_preference) === "oauth") {
      dom.aiAuthStatus.textContent =
        `OAuth for ${providerLabel} is coming soon. Use API key verification for now.`;
    } else {
      dom.aiAuthStatus.textContent = providerSettings.api_key_stored
        ? `${providerLabel} key stored. Verify to unlock online usage.`
        : `No API key stored for ${providerLabel} yet.`;
    }
  }
}

function closeAuthModal(): void {
  setAuthModalOpen(false);
  authModalProvider = null;
  if (dom.aiAuthApiKeyInput) {
    dom.aiAuthApiKeyInput.value = "";
  }
}

async function saveProviderApiKey(provider: CloudAIFallbackProvider, apiKey: string): Promise<void> {
  if (!settings) return;
  await invoke("save_provider_api_key", { provider, apiKey });
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (providerSettings) {
    providerSettings.api_key_stored = true;
    providerSettings.auth_status = "locked";
    providerSettings.auth_verified_at = null;
  }
  ensureOnlineModeConstraints(true);
  await persistSettings();
  renderAIFallbackSettingsUi();
  refreshAuthModalContent();
}

async function clearProviderApiKey(provider: CloudAIFallbackProvider): Promise<void> {
  if (!settings) return;
  await invoke("clear_provider_api_key", { provider });
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (providerSettings) {
    providerSettings.api_key_stored = false;
    providerSettings.auth_status = "locked";
    providerSettings.auth_verified_at = null;
  }
  ensureOnlineModeConstraints(true);
  await persistSettings();
  renderAIFallbackSettingsUi();
  refreshAuthModalContent();
}

async function verifyProviderCredentials(provider: CloudAIFallbackProvider): Promise<void> {
  if (!settings) return;
  const providerSettings = getAIFallbackProviderSettings(provider);
  const authMethod = normalizeAuthMethodPreference(providerSettings?.auth_method_preference);
  if (authMethod === "oauth") {
    showToast({
      type: "info",
      title: "OAuth coming soon",
      message: "OAuth verification is not available yet. Use API key verification for now.",
      duration: 4200,
    });
    return;
  }
  if (!providerSettings?.api_key_stored) {
    showToast({
      type: "warning",
      title: "Missing API key",
      message: "Save an API key first, then click Verify.",
      duration: 3000,
    });
    return;
  }

  try {
    const result = await invoke<{ message?: string; method?: string; verified_at?: string }>(
      "verify_provider_auth",
      { provider, method: authMethod }
    );
    if (providerSettings) {
      providerSettings.auth_status =
        (result?.method as "verified_api_key" | "verified_oauth") || "verified_api_key";
      providerSettings.auth_verified_at = result?.verified_at ?? new Date().toISOString();
    }
    if (!settings.ai_fallback.fallback_provider) {
      settings.ai_fallback.fallback_provider = provider;
    }
    await refreshAIFallbackModels(provider);
    await persistSettings();
    renderAIFallbackSettingsUi();
    refreshAuthModalContent();
    showToast({
      type: "success",
      title: "Provider verified",
      message: result?.message ?? `${provider} is unlocked for online fallback.`,
      duration: 3500,
    });
  } catch (error) {
    if (providerSettings) {
      providerSettings.auth_status = "locked";
      providerSettings.auth_verified_at = null;
    }
    ensureOnlineModeConstraints(true);
    await persistSettings();
    renderAIFallbackSettingsUi();
    refreshAuthModalContent();
    showToast({
      type: "error",
      title: "Verification failed",
      message: String(error),
      duration: 5000,
    });
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
  originalInput.title = "Word or phrase to find in transcripts";

  const replacementInput = document.createElement("input");
  replacementInput.type = "text";
  replacementInput.value = replacement;
  replacementInput.placeholder = "API";
  replacementInput.className = "vocab-input";
  replacementInput.title = "Text to substitute for the matched word or phrase";

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
type MainTab = "transcription" | "settings" | "ai-refinement" | "voice-output" | "modules";
let aiRefinementTabRefreshInFlight: Promise<void> | null = null;

function aiRefinementTabAvailable(): boolean {
  return isAiRefinementModuleEnabled();
}

function voiceOutputTabAvailable(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("output_voice_tts") ?? false;
}

function syncMainTabAvailability(): void {
  const aiAvailable = aiRefinementTabAvailable();
  const voiceAvailable = voiceOutputTabAvailable();
  if (dom.tabBtnAiRefinement) {
    dom.tabBtnAiRefinement.hidden = !aiAvailable;
    dom.tabBtnAiRefinement.setAttribute("aria-hidden", (!aiAvailable).toString());
    if (aiAvailable) {
      dom.tabBtnAiRefinement.removeAttribute("tabindex");
    } else {
      dom.tabBtnAiRefinement.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabAiRefinement) {
    dom.tabAiRefinement.hidden = !aiAvailable;
    if (!aiAvailable) {
      dom.tabAiRefinement.classList.remove("active");
    }
  }
  if (dom.tabBtnVoiceOutput) {
    dom.tabBtnVoiceOutput.hidden = !voiceAvailable;
    dom.tabBtnVoiceOutput.setAttribute("aria-hidden", (!voiceAvailable).toString());
    if (voiceAvailable) {
      dom.tabBtnVoiceOutput.removeAttribute("tabindex");
    } else {
      dom.tabBtnVoiceOutput.setAttribute("tabindex", "-1");
    }
  }
  if (dom.tabVoiceOutput) {
    dom.tabVoiceOutput.hidden = !voiceAvailable;
    if (!voiceAvailable) {
      dom.tabVoiceOutput.classList.remove("active");
    }
  }
}

function getActiveMainTabFromDom(): MainTab {
  if (dom.tabBtnSettings?.classList.contains("active")) return "settings";
  if (dom.tabBtnAiRefinement?.classList.contains("active")) return "ai-refinement";
  if (dom.tabBtnVoiceOutput?.classList.contains("active")) return "voice-output";
  if (dom.tabBtnModules?.classList.contains("active")) return "modules";
  return "transcription";
}

export function reconcileMainTabVisibility(): void {
  syncMainTabAvailability();
  const activeTab = getActiveMainTabFromDom();
  if (!aiRefinementTabAvailable() && activeTab === "ai-refinement") {
    switchMainTab("transcription");
    return;
  }
  if (!voiceOutputTabAvailable() && activeTab === "voice-output") {
    switchMainTab("transcription");
  }
}

async function refreshAiRefinementTabState(): Promise<void> {
  if (aiRefinementTabRefreshInFlight) {
    await aiRefinementTabRefreshInFlight;
    return;
  }

  const refreshTask = (async () => {
    await refreshOllamaRuntimeState({ force: true });
    if (getOllamaRuntimeCardState().healthy) {
      await refreshOllamaInstalledModels();
    }
    renderAIFallbackSettingsUi();
    renderOllamaModelManager();
  })();

  aiRefinementTabRefreshInFlight = refreshTask;
  try {
    await refreshTask;
  } finally {
    if (aiRefinementTabRefreshInFlight === refreshTask) {
      aiRefinementTabRefreshInFlight = null;
    }
  }
}

export function openMainTab(tab: MainTab) {
  switchMainTab(tab);
}

function switchMainTab(tab: MainTab) {
  syncMainTabAvailability();
  let resolvedTab: MainTab = tab;
  if (resolvedTab === "ai-refinement" && !aiRefinementTabAvailable()) {
    resolvedTab = "transcription";
  }
  if (resolvedTab === "voice-output" && !voiceOutputTabAvailable()) {
    resolvedTab = "transcription";
  }

  const isTranscription = resolvedTab === "transcription";
  const isSettings = resolvedTab === "settings";
  const isAiRefinement = resolvedTab === "ai-refinement";
  const isVoiceOutput = resolvedTab === "voice-output";
  const isModules = resolvedTab === "modules";

  dom.tabBtnTranscription?.classList.toggle("active", isTranscription);
  dom.tabBtnSettings?.classList.toggle("active", isSettings);
  dom.tabBtnAiRefinement?.classList.toggle("active", isAiRefinement);
  dom.tabBtnVoiceOutput?.classList.toggle("active", isVoiceOutput);
  dom.tabBtnModules?.classList.toggle("active", isModules);

  dom.tabBtnTranscription?.setAttribute("aria-selected", isTranscription.toString());
  dom.tabBtnSettings?.setAttribute("aria-selected", isSettings.toString());
  dom.tabBtnAiRefinement?.setAttribute("aria-selected", isAiRefinement.toString());
  dom.tabBtnVoiceOutput?.setAttribute("aria-selected", isVoiceOutput.toString());
  dom.tabBtnModules?.setAttribute("aria-selected", isModules.toString());

  // Update tab content visibility — clear any inline display styles first
  if (dom.tabTranscription) {
    dom.tabTranscription.style.removeProperty("display");
    dom.tabTranscription.classList.toggle("active", isTranscription);
  }
  if (dom.tabSettings) {
    dom.tabSettings.style.removeProperty("display");
    dom.tabSettings.classList.toggle("active", isSettings);
  }
  if (dom.tabAiRefinement) {
    dom.tabAiRefinement.style.removeProperty("display");
    dom.tabAiRefinement.classList.toggle("active", isAiRefinement);
  }
  if (dom.tabVoiceOutput) {
    dom.tabVoiceOutput.style.removeProperty("display");
    dom.tabVoiceOutput.classList.toggle("active", isVoiceOutput);
  }
  if (dom.tabModules) {
    dom.tabModules.style.removeProperty("display");
    dom.tabModules.classList.toggle("active", isModules);
  }

  // Persist to localStorage
  try {
    localStorage.setItem("trispr-active-tab", resolvedTab);
  } catch (error) {
    console.error("Failed to persist active tab", error);
  }

  if (isAiRefinement) {
    void (async () => {
      try {
        await refreshAiRefinementTabState();
      } catch (error) {
        console.warn("Failed to refresh Ollama runtime on tab switch:", error);
      }
    })();
  }
}

// Initialize tab state from localStorage
export function initMainTab() {
  syncMainTabAvailability();
  try {
    const savedTab = localStorage.getItem("trispr-active-tab") as MainTab | null;
    if (
      savedTab === "settings" ||
      savedTab === "transcription" ||
      savedTab === "ai-refinement" ||
      savedTab === "voice-output" ||
      savedTab === "modules"
    ) {
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

// Registers a "change" event listener that only saves settings.
// Use for sliders / toggles whose value was already written to the settings
// object by the companion "input" listener; this just persists the final value.
function onChangePersist(el: Element | null | undefined): void {
  el?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });
}

export function wireEvents() {
  ensureAIFallbackSettingsDefaults();
  ensureContinuousDumpDefaults();
  if (syncHistoryAliasesIntoSettings()) {
    void persistSettings();
  }

  // Main tab switching
  dom.tabBtnTranscription?.addEventListener("click", () => {
    switchMainTab("transcription");
  });

  dom.tabBtnSettings?.addEventListener("click", () => {
    switchMainTab("settings");
  });

  dom.tabBtnAiRefinement?.addEventListener("click", () => {
    switchMainTab("ai-refinement");
  });
  dom.tabBtnVoiceOutput?.addEventListener("click", () => {
    switchMainTab("voice-output");
  });
  dom.tabBtnModules?.addEventListener("click", () => {
    switchMainTab("modules");
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
    syncCaptureModeVisibility(settings.mode, settings.ptt_use_vad);
    renderSettings();
    await persistSettings();
    renderHero();
  });

  dom.productModeSelect?.addEventListener("change", async () => {
    if (!settings || !dom.productModeSelect) return;
    settings.product_mode = dom.productModeSelect.value === "assistant" ? "assistant" : "transcribe";
    renderSettings();
    syncWorkflowAgentConsoleState();
    await persistSettings();
    renderHero();
  });

  dom.modelSourceSelect?.addEventListener("change", async () => {
    if (!settings || !dom.modelSourceSelect) return;
    settings.model_source = dom.modelSourceSelect.value as Settings["model_source"];
    await persistSettings();
    scheduleSettingsRender();
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
    try {
      await navigator.clipboard.writeText(transcript);
    } catch {
      showToast({ type: "error", title: "Kopieren fehlgeschlagen", message: "Clipboard-Zugriff verweigert." });
    }
  });

  dom.historyDeleteConversation?.addEventListener("click", async () => {
    const totalEntries = history.length + transcribeHistory.length;
    if (totalEntries === 0) {
      showToast({
        type: "info",
        title: "Nichts zu löschen",
        message: "Der aktuelle Verlauf ist bereits leer.",
      });
      return;
    }

    const confirmed = window.confirm(
      "Gesamtes Transkript (Input + System) aus dem aktuellen Verlauf löschen?\n\nDiese Aktion kann nicht rückgängig gemacht werden."
    );
    if (!confirmed) return;

    try {
      const deletedCount = await invoke<number>("clear_active_transcript_history");
      showToast({
        type: "success",
        title: "Transkript gelöscht",
        message: `${deletedCount} Einträge wurden dauerhaft entfernt.`,
      });
    } catch (error) {
      showToast({
        type: "error",
        title: "Löschen fehlgeschlagen",
        message: String(error),
      });
    }
  });

  dom.analyseButton?.addEventListener("click", () => {
    switchMainTab("modules");
    window.dispatchEvent(new CustomEvent("modules:focus", { detail: "analysis" }));
  });
  dom.openModulesBtn?.addEventListener("click", () => {
    switchMainTab("modules");
  });

  dom.historyExport?.addEventListener("click", () => {
    void openExportDialog();
  });

  dom.openRecordingsBtn?.addEventListener("click", () => {
    void invoke("open_recordings_directory");
  });

  dom.archiveBrowseBtn?.addEventListener("click", () => {
    void openArchiveBrowser();
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
    const size = setHistoryFontSize(currentHistoryTab, Number(dom.conversationFontSize.value));
    document.documentElement.style.setProperty("--history-active-font-size", `${size}px`);
    if (dom.conversationFontSizeValue) {
      dom.conversationFontSizeValue.textContent = `${size}px`;
    }
    updateRangeAria("conversation-font-size", size);
    scheduleHistoryRender();
  });

  const commitAlias = (key: "mic" | "system", input: HTMLInputElement | null): void => {
    if (!input) return;
    input.value = setHistoryAlias(key, input.value);
    syncHistoryToolbarState();
    scheduleHistoryRender();
    void persistSettings();
  };

  dom.historyAliasMicInput?.addEventListener("change", () =>
    commitAlias("mic", dom.historyAliasMicInput));
  dom.historyAliasMicInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commitAlias("mic", dom.historyAliasMicInput); }
  });

  dom.historyAliasSystemInput?.addEventListener("change", () =>
    commitAlias("system", dom.historyAliasSystemInput));
  dom.historyAliasSystemInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commitAlias("system", dom.historyAliasSystemInput); }
  });

  // Hotkey recording functionality + registration status listener
  initHotkeyStatusListener();
  setupHotkeyRecorder("ptt", dom.pttHotkey, dom.pttHotkeyRecord, dom.pttHotkeyStatus);
  setupHotkeyRecorder("toggle", dom.toggleHotkey, dom.toggleHotkeyRecord, dom.toggleHotkeyStatus);
  setupHotkeyRecorder("transcribe", dom.transcribeHotkey, dom.transcribeHotkeyRecord, dom.transcribeHotkeyStatus);
  setupHotkeyRecorder("toggleActivationWords", dom.toggleActivationWordsHotkey, dom.toggleActivationWordsHotkeyRecord, dom.toggleActivationWordsHotkeyStatus);

  const _onResize = () => updateDeviceLineClamp();
  window.addEventListener("resize", _onResize);
  _windowCleanups.push(() => window.removeEventListener("resize", _onResize));

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

  onChangePersist(dom.transcribeVadThreshold);

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

  onChangePersist(dom.transcribeVadSilence);

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

  onChangePersist(dom.transcribeBatchInterval);

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

  onChangePersist(dom.transcribeChunkOverlap);

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

  onChangePersist(dom.transcribeGain);

  dom.languageSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_mode = dom.languageSelect!.value as Settings["language_mode"];
    settings.postproc_language = derivePostprocLanguageFromAsr(
      settings.language_mode,
      settings.language_pinned
    );
    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderSettings();
  });

  dom.languagePinnedToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_pinned = dom.languagePinnedToggle!.checked;
    settings.postproc_language = derivePostprocLanguageFromAsr(
      settings.language_mode,
      settings.language_pinned
    );
    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderSettings();
  });



  dom.audioCuesToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.audio_cues = dom.audioCuesToggle!.checked;
    await persistSettings();
  });

  dom.pttUseVadToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.ptt_use_vad = dom.pttUseVadToggle!.checked;
    syncCaptureModeVisibility(settings.mode, settings.ptt_use_vad);
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

  onChangePersist(dom.audioCuesVolume);

  dom.hallucinationFilterToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.hallucination_filter_enabled = dom.hallucinationFilterToggle!.checked;
    await persistSettings();
  });

  dom.activationWordsToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.activation_words_enabled = dom.activationWordsToggle!.checked;
    await persistSettings();
    scheduleSettingsRender();
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

  // Quality & Encoding event listeners — sync both opus toggles
  const syncOpusToggles = async (source: HTMLInputElement) => {
    if (!settings) return;
    settings.opus_enabled = source.checked;
    if (dom.opusEnabledToggle && dom.opusEnabledToggle !== source) dom.opusEnabledToggle.checked = source.checked;
    if (dom.opusArchiveToggle && dom.opusArchiveToggle !== source) dom.opusArchiveToggle.checked = source.checked;
    await persistSettings();
  };
  dom.opusEnabledToggle?.addEventListener("change", () => void syncOpusToggles(dom.opusEnabledToggle!));
  dom.opusArchiveToggle?.addEventListener("change", () => void syncOpusToggles(dom.opusArchiveToggle!));

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
    scheduleSettingsRender();
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
  onChangePersist(dom.continuousHardCut);

  dom.continuousMinChunk?.addEventListener("input", () => {
    if (!settings || !dom.continuousMinChunk) return;
    const value = Math.max(250, Math.min(5000, Number(dom.continuousMinChunk.value)));
    settings.continuous_min_chunk_ms = value;
    if (dom.continuousMinChunkValue) dom.continuousMinChunkValue.textContent = `${(value / 1000).toFixed(1)}s`;
    updateRangeAria("continuous-min-chunk", value);
  });
  onChangePersist(dom.continuousMinChunk);

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
  onChangePersist(dom.continuousPreRoll);

  dom.continuousPostRoll?.addEventListener("input", () => {
    if (!settings || !dom.continuousPostRoll) return;
    const value = Math.max(0, Math.min(1500, Number(dom.continuousPostRoll.value)));
    settings.continuous_post_roll_ms = value;
    if (dom.continuousPostRollValue) dom.continuousPostRollValue.textContent = `${(value / 1000).toFixed(2)}s`;
    updateRangeAria("continuous-post-roll", value);
  });
  onChangePersist(dom.continuousPostRoll);

  dom.continuousKeepalive?.addEventListener("input", () => {
    if (!settings || !dom.continuousKeepalive) return;
    const value = Math.max(10000, Math.min(120000, Number(dom.continuousKeepalive.value)));
    settings.continuous_idle_keepalive_ms = value;
    if (dom.continuousKeepaliveValue) dom.continuousKeepaliveValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-keepalive", value);
  });
  onChangePersist(dom.continuousKeepalive);

  dom.continuousSystemOverrideToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.continuous_system_override_enabled = dom.continuousSystemOverrideToggle!.checked;
    if (!settings.continuous_system_override_enabled) {
      settings.continuous_system_soft_flush_ms = settings.continuous_soft_flush_ms!;
      settings.continuous_system_silence_flush_ms = settings.continuous_silence_flush_ms!;
      settings.continuous_system_hard_cut_ms = settings.continuous_hard_cut_ms!;
    }
    scheduleSettingsRender();
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
  onChangePersist(dom.continuousSystemSoftFlush);

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
  onChangePersist(dom.continuousSystemSilenceFlush);

  dom.continuousSystemHardCut?.addEventListener("input", () => {
    if (!settings || !dom.continuousSystemHardCut) return;
    const value = Math.max(15000, Math.min(120000, Number(dom.continuousSystemHardCut.value)));
    settings.continuous_system_hard_cut_ms = value;
    if (dom.continuousSystemHardCutValue) dom.continuousSystemHardCutValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-system-hard-cut", value);
  });
  onChangePersist(dom.continuousSystemHardCut);

  dom.continuousMicOverrideToggle?.addEventListener("change", async () => {
    if (!settings) return;
    settings.continuous_mic_override_enabled = dom.continuousMicOverrideToggle!.checked;
    if (!settings.continuous_mic_override_enabled) {
      settings.continuous_mic_soft_flush_ms = settings.continuous_soft_flush_ms!;
      settings.continuous_mic_silence_flush_ms = settings.continuous_silence_flush_ms!;
      settings.continuous_mic_hard_cut_ms = settings.continuous_hard_cut_ms!;
    }
    scheduleSettingsRender();
    await persistSettings();
  });

  dom.continuousMicSoftFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicSoftFlush) return;
    const value = Math.max(4000, Math.min(30000, Number(dom.continuousMicSoftFlush.value)));
    settings.continuous_mic_soft_flush_ms = value;
    if (dom.continuousMicSoftFlushValue) dom.continuousMicSoftFlushValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-mic-soft-flush", value);
  });
  onChangePersist(dom.continuousMicSoftFlush);

  dom.continuousMicSilenceFlush?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicSilenceFlush) return;
    const value = Math.max(300, Math.min(5000, Number(dom.continuousMicSilenceFlush.value)));
    settings.continuous_mic_silence_flush_ms = value;
    if (dom.continuousMicSilenceFlushValue) dom.continuousMicSilenceFlushValue.textContent = `${(value / 1000).toFixed(1)}s`;
    updateRangeAria("continuous-mic-silence-flush", value);
  });
  onChangePersist(dom.continuousMicSilenceFlush);

  dom.continuousMicHardCut?.addEventListener("input", () => {
    if (!settings || !dom.continuousMicHardCut) return;
    const value = Math.max(15000, Math.min(120000, Number(dom.continuousMicHardCut.value)));
    settings.continuous_mic_hard_cut_ms = value;
    if (dom.continuousMicHardCutValue) dom.continuousMicHardCutValue.textContent = `${Math.round(value / 1000)}s`;
    updateRangeAria("continuous-mic-hard-cut", value);
  });
  onChangePersist(dom.continuousMicHardCut);

  // Post-processing event listeners
  dom.postprocEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_enabled = dom.postprocEnabled!.checked;
    await persistSettings();
    scheduleSettingsRender();
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
    scheduleSettingsRender();
  });

  dom.postprocVocabAdd?.addEventListener("click", () => {
    addVocabRow("", "");
  });

  // AI fallback event listeners
  dom.aiFallbackEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    if (!isAiRefinementModuleEnabled()) {
      dom.aiFallbackEnabled!.checked = false;
      settings.ai_fallback.enabled = false;
      settings.postproc_llm_enabled = false;
      await persistSettings();
      renderAIFallbackSettingsUi();
      renderHero();
      showToast({
        type: "warning",
        title: "Module disabled",
        message: "Enable module 'ai_refinement' first.",
        duration: 3200,
      });
      return;
    }
    const enabling = dom.aiFallbackEnabled!.checked;

    if (enabling) {
      const provider = settings.ai_fallback.provider;
      if (provider === "ollama") {
        // For Ollama: check if runtime is available before enabling
        const runtimeInfo = await invoke<any>("detect_ollama_runtime").catch(() => null);
        const ollamaDetected = runtimeInfo?.found === true;
        if (!ollamaDetected) {
          const userWantsInstall = await showOllamaRequiredModal();
          if (!userWantsInstall) {
            dom.aiFallbackEnabled!.checked = false;
            return;
          }
        }
      }
      // LM Studio / Oobabooga: no runtime detection needed, server is managed externally
    }

    settings.ai_fallback.enabled = enabling;
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
    if (settings.ai_fallback.enabled) {
      if (settings.ai_fallback.provider === "ollama") {
        void autoStartLocalRuntimeIfNeeded("enable_toggle").finally(() => {
          renderAIFallbackSettingsUi();
          renderOllamaModelManager();
        });
      }
    } else {
      // Stop Ollama runtime when AI refinement is disabled — only if Ollama is the active provider
      if (settings.ai_fallback.provider === "ollama") {
        try {
          await invoke("stop_ollama_runtime");
        } catch (error) {
          console.warn("Failed to stop Ollama runtime:", error);
        }
      }
    }
  });

  dom.aiFallbackCloudProviderList?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const actionBtn = target?.closest<HTMLButtonElement>("[data-ai-provider-action]");
    if (!actionBtn) return;
    event.preventDefault();
    showToast({
      type: "info",
      title: "Roadmap-only",
      message: "Online fallback controls are visible for roadmap transparency and are currently read-only.",
      duration: 3200,
    });
  });

  dom.aiAuthModalClose?.addEventListener("click", () => {
    closeAuthModal();
  });
  dom.aiAuthModalBackdrop?.addEventListener("click", () => {
    closeAuthModal();
  });
  const _onKeydown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && dom.aiAuthModal && !dom.aiAuthModal.hidden) {
      closeAuthModal();
    }
  };
  window.addEventListener("keydown", _onKeydown);
  _windowCleanups.push(() => window.removeEventListener("keydown", _onKeydown));
  dom.aiAuthMethod?.addEventListener("change", async () => {
    if (!settings || !dom.aiAuthMethod || !authModalProvider) return;
    ensureAIFallbackSettingsDefaults();
    const providerSettings = getAIFallbackProviderSettings(authModalProvider);
    if (!providerSettings) return;
    providerSettings.auth_method_preference = normalizeAuthMethodPreference(
      dom.aiAuthMethod.value
    );
    await persistSettings();
    renderAIFallbackSettingsUi();
    refreshAuthModalContent();
    if (providerSettings.auth_method_preference === "oauth") {
      showToast({
        type: "info",
        title: "OAuth coming soon",
        message: "OAuth verification is not available yet. Use API key for now.",
        duration: 3600,
      });
    }
  });

  const activateLocalLane = async (notify = false) => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const switched = settings.ai_fallback.execution_mode !== "local_primary";
    applyExecutionModeInSettings("local_primary");
    // Only refresh Ollama runtime state when Ollama is actually the active backend.
    // Calling this for LM Studio / Oobabooga causes a 2–3 s freeze due to the
    // Ollama endpoint ping in detect_ollama_runtime.
    if (settings.ai_fallback.provider === "ollama") {
      await refreshOllamaRuntimeState({ force: true });
      if (getOllamaRuntimeCardState().healthy) {
        await refreshOllamaInstalledModels();
      }
    }
    await persistSettings();
    refreshAIUi();
    if (notify && switched) {
      showToast({
        type: "success",
        title: "Local runtime active",
        message: "Local AI backend active.",
        duration: 2600,
      });
    }
  };

  const activateOnlineLane = async () => {
    applyExecutionModeInSettings("local_primary");
    renderAIFallbackSettingsUi();
    renderOllamaModelManager();
    renderHero();
    showToast({
      type: "info",
      title: "Roadmap-only",
      message: "Online fallback is currently read-only and not active in production.",
      duration: 3600,
    });
  };

  dom.aiFallbackLocalLane?.addEventListener("click", (event) => {
    if (isLaneControlTarget(event.target)) return;
    void activateLocalLane(false);
  });
  dom.aiFallbackLocalLane?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (isLaneControlTarget(event.target)) return;
    event.preventDefault();
    void activateLocalLane(false);
  });

  dom.aiFallbackOnlineLane?.addEventListener("click", (event) => {
    if (isLaneControlTarget(event.target)) return;
    void activateOnlineLane();
  });
  dom.aiFallbackOnlineLane?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (isLaneControlTarget(event.target)) return;
    event.preventDefault();
    void activateOnlineLane();
  });

  dom.aiFallbackModel?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackModel) return;
    ensureAIFallbackSettingsDefaults();
    const mode = normalizeExecutionMode(settings.ai_fallback.execution_mode);
    const provider = normalizeAIFallbackProvider(settings.ai_fallback.provider);
    if (provider === "ollama" || mode !== "online_fallback") {
      // Ollama model selection is handled only in the Local AI Runtime manager.
      return;
    }
    settings.ai_fallback.model = dom.aiFallbackModel.value;
    settings.postproc_llm_model = settings.ai_fallback.model;
    const providerSettings = getAIFallbackProviderSettings(provider);
    if (providerSettings) {
      providerSettings.preferred_model = settings.ai_fallback.model;
    }
    await persistSettings();
  });

  // Local runtime action handlers in Local Primary card
  dom.aiFallbackLocalPrimaryAction?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    applyExecutionModeInSettings("local_primary");
    await persistSettings();
    const action = dom.aiFallbackLocalPrimaryAction?.dataset.runtimeAction ?? "install";
    if (action === "ready") {
      renderAIFallbackSettingsUi();
      return;
    }
    if (action === "start") {
      const startPromise = startOllamaRuntime();
      renderAIFallbackSettingsUi();
      await startPromise;
    } else {
      const ensurePromise = ensureLocalRuntimeReady();
      renderAIFallbackSettingsUi();
      await ensurePromise;
    }
    refreshAIUi();
  });

  dom.aiFallbackLocalImportAction?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    applyExecutionModeInSettings("local_primary");
    await persistSettings();
    const importPromise = importOllamaModelFromLocalFile();
    renderAIFallbackSettingsUi();
    await importPromise;
    refreshAIUi();
  });

  dom.aiFallbackLocalVerifyAction?.addEventListener("click", async () => {
    const verifyPromise = verifyOllamaRuntime();
    renderAIFallbackSettingsUi();
    await verifyPromise;
    renderAIFallbackSettingsUi();
  });

  // Combined refresh: runtime state + models (no GitHub call)
  dom.aiFallbackLocalRefreshAction?.addEventListener("click", async () => {
    renderAIFallbackSettingsUi();
    await Promise.all([
      refreshOllamaRuntimeAndModels(),
      refreshOllamaRuntimeVersionCatalog(true),
    ]);
    renderAIFallbackSettingsUi();
  });

  // Explicit GitHub fetch for version list — only on user request
  dom.aiFallbackFetchVersionsAction?.addEventListener("click", async () => {
    if (dom.aiFallbackFetchVersionsAction) {
      dom.aiFallbackFetchVersionsAction.disabled = true;
    }
    await fetchOnlineVersionCatalog((msg) => {
      if (dom.aiFallbackFetchVersionsStatus) {
        if (msg) {
          dom.aiFallbackFetchVersionsStatus.textContent = msg;
          dom.aiFallbackFetchVersionsStatus.hidden = false;
        } else {
          dom.aiFallbackFetchVersionsStatus.hidden = true;
          dom.aiFallbackFetchVersionsStatus.textContent = "";
        }
      }
    });
    if (dom.aiFallbackFetchVersionsAction) {
      dom.aiFallbackFetchVersionsAction.disabled = false;
    }
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackLocalRuntimeVersion?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLocalRuntimeVersion) return;
    const selected = dom.aiFallbackLocalRuntimeVersion.value.trim();
    if (!selected) return;
    settings.providers.ollama.runtime_target_version = selected;
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  // Runtime source toggle: managed ↔ system
  dom.aiFallbackLocalRuntimeSource?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLocalRuntimeSource) return;
    const source = dom.aiFallbackLocalRuntimeSource.value;
    ensureAIFallbackSettingsDefaults();
    applyExecutionModeInSettings("local_primary");
    await persistSettings();
    const switchPromise = source === "system"
      ? useSystemOllamaRuntime()
      : useManagedOllamaRuntime();
    renderAIFallbackSettingsUi();
    await switchPromise;
    refreshAIUi();
  });

  dom.aiFallbackLocalBackendSelect?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLocalBackendSelect) return;
    const backend = dom.aiFallbackLocalBackendSelect.value as "ollama" | "lm_studio" | "oobabooga";
    settings.ai_fallback.provider = backend;
    // Render immediately so the UI reflects the new backend before the slow save.
    renderAIFallbackSettingsUi();
    await persistSettings();
    // After save, re-render with the normalized settings from Rust.
    renderAIFallbackSettingsUi();
    // Trigger Ollama runtime refresh only when Ollama is selected.
    if (backend === "ollama") {
      void refreshOllamaRuntimeState({ force: false }).finally(() => renderAIFallbackSettingsUi());
    }
  });

  dom.aiFallbackLocalFallbackEndpoints?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLocalFallbackEndpoints) return;
    const raw = dom.aiFallbackLocalFallbackEndpoints.value;
    settings.providers.ollama.fallback_endpoints = raw
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
    await persistSettings();
  });

  // ── OpenAI-compat backend (LM Studio / Oobabooga) config listeners ─────────

  function getCompatSettings() {
    if (!settings) return null;
    const p = settings.ai_fallback.provider;
    if (p === "lm_studio") return settings.providers.lm_studio ??= { endpoint: "http://127.0.0.1:1234", api_key: "", preferred_model: "", available_models: [] };
    if (p === "oobabooga") return settings.providers.oobabooga ??= { endpoint: "http://127.0.0.1:5000", api_key: "", preferred_model: "", available_models: [] };
    return null;
  }

  dom.aiFallbackCompatEndpoint?.addEventListener("change", async () => {
    const s = getCompatSettings();
    if (!s || !dom.aiFallbackCompatEndpoint) return;
    s.endpoint = dom.aiFallbackCompatEndpoint.value.trim() || s.endpoint;
    await persistSettings();
  });

  dom.aiFallbackCompatApiKey?.addEventListener("change", async () => {
    const s = getCompatSettings();
    if (!s || !dom.aiFallbackCompatApiKey) return;
    s.api_key = dom.aiFallbackCompatApiKey.value;
    await persistSettings();
  });

  // Model selection is now handled by card "Set active" buttons in settings.ts
  // (renderCompatModelCards). The old <select> dropdown has been removed.

  dom.aiFallbackCompatFetchModels?.addEventListener("click", async () => {
    const s = getCompatSettings();
    if (!s || !dom.aiFallbackCompatFetchModels || !settings) return;
    dom.aiFallbackCompatFetchModels.disabled = true;
    dom.aiFallbackCompatFetchModels.textContent = "Fetching…";
    if (dom.aiFallbackCompatStatus) dom.aiFallbackCompatStatus.textContent = "Connecting to server…";
    try {
      const models = await invoke<string[]>("fetch_available_models", {
        provider: settings.ai_fallback.provider,
      });
      s.available_models = models;
      if (!s.preferred_model && models.length > 0) {
        s.preferred_model = models[0];
        settings.ai_fallback.model = models[0];
      }
      await persistSettings();
      if (dom.aiFallbackCompatStatus) dom.aiFallbackCompatStatus.textContent = `${models.length} model(s) found.`;
    } catch (err) {
      if (dom.aiFallbackCompatStatus) {
        dom.aiFallbackCompatStatus.textContent = `Connection failed: ${err instanceof Error ? err.message : String(err)}`;
      }
    } finally {
      dom.aiFallbackCompatFetchModels!.disabled = false;
      dom.aiFallbackCompatFetchModels!.textContent = "Fetch models";
      renderAIFallbackSettingsUi();
    }
  });

  dom.aiFallbackCompatVerifyAction?.addEventListener("click", async () => {
    const s = getCompatSettings();
    if (!s || !dom.aiFallbackCompatVerifyAction || !settings) return;
    dom.aiFallbackCompatVerifyAction.disabled = true;
    if (dom.aiFallbackCompatStatus) dom.aiFallbackCompatStatus.textContent = "Verifying…";
    try {
      const result = await invoke<{ ok: boolean; message: string }>("test_provider_connection", {
        provider: settings.ai_fallback.provider,
        apiKey: s.api_key || "",
      });
      if (dom.aiFallbackCompatStatus) {
        dom.aiFallbackCompatStatus.textContent = result.ok ? `✓ ${result.message}` : `✗ ${result.message}`;
      }
    } catch (err) {
      if (dom.aiFallbackCompatStatus) {
        dom.aiFallbackCompatStatus.textContent = `✗ ${err instanceof Error ? err.message : String(err)}`;
      }
    } finally {
      dom.aiFallbackCompatVerifyAction!.disabled = false;
    }
  });

  dom.aiFallbackLmStudioInstallAction?.addEventListener("click", async () => {
    const btn = dom.aiFallbackLmStudioInstallAction!;
    btn.disabled = true;
    btn.textContent = "Opening installer…";
    try {
      await invoke("install_lm_studio");
      showToast({
        type: "info",
        title: "LM Studio installer launched",
        message: "A PowerShell window has opened. Follow the prompts to complete installation, then restart Trispr Flow.",
        duration: 7000,
      });
    } catch (err) {
      showToast({
        type: "error",
        title: "Installer failed to launch",
        message: err instanceof Error ? err.message : String(err),
        duration: 5000,
      });
    } finally {
      btn.disabled = false;
      btn.textContent = "Install LM Studio";
    }
  });

  const handleSaveCredentialsClick = async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = getCredentialTargetProvider();
    if (!provider) {
      showToast({
        type: "warning",
        title: "Select provider",
        message: "Choose a cloud provider before saving credentials.",
        duration: 3000,
      });
      return;
    }
    const apiKey = (dom.aiAuthApiKeyInput?.value ?? "").trim();
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
      await saveProviderApiKey(provider, apiKey);
      if (dom.aiAuthApiKeyInput) dom.aiAuthApiKeyInput.value = "";
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
  };

  const handleClearCredentialsClick = async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = getCredentialTargetProvider();
    if (!provider) {
      showToast({
        type: "warning",
        title: "Select provider",
        message: "Choose a cloud provider before clearing credentials.",
        duration: 3000,
      });
      return;
    }
    try {
      await clearProviderApiKey(provider);
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
  };

  const handleVerifyCredentialsClick = async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const provider = getCredentialTargetProvider();
    if (!provider) {
      showToast({
        type: "warning",
        title: "Select provider",
        message: "Choose a cloud provider before verification.",
        duration: 3000,
      });
      return;
    }
    await verifyProviderCredentials(provider);
  };

  dom.aiAuthSaveKey?.addEventListener("click", () => {
    void handleSaveCredentialsClick();
  });
  dom.aiAuthClearKey?.addEventListener("click", () => {
    void handleClearCredentialsClick();
  });
  dom.aiAuthVerifyKey?.addEventListener("click", () => {
    void handleVerifyCredentialsClick();
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

  onChangePersist(dom.aiFallbackTemperature);

  dom.aiFallbackPreserveLanguage?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackPreserveLanguage) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.preserve_source_language = dom.aiFallbackPreserveLanguage.checked;
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackLowLatencyMode?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLowLatencyMode) return;
    ensureAIFallbackSettingsDefaults();
    const enabled = dom.aiFallbackLowLatencyMode.checked;
    settings.ai_fallback.low_latency_mode = enabled;

    if (enabled) {
      if (settings.ai_fallback.max_tokens > 512) {
        settings.ai_fallback.max_tokens = 512;
      }
      if (settings.ai_fallback.temperature > 0.2) {
        settings.ai_fallback.temperature = 0.15;
      }
    }

    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackMaxTokens?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackMaxTokens) return;
    ensureAIFallbackSettingsDefaults();
    settings.ai_fallback.max_tokens = Math.max(128, Math.min(8192, Number(dom.aiFallbackMaxTokens.value)));
    await persistSettings();
  });

  dom.promptPresetList?.addEventListener("click", async (e) => {
    if (!settings) return;
    const target = e.target as HTMLElement;
    const btn = target.closest("[data-action]") as HTMLElement | null;
    if (!btn) return;
    const action = btn.dataset.action;
    const chip = target.closest("[data-preset-id]") as HTMLElement | null;
    const presetId = chip?.dataset.presetId;
    if (!presetId) return;

    ensureAIFallbackSettingsDefaults();

    if (action === "use-preset") {
      const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
      if (hasPendingUserChanges) await persistSettings();
      settings.ai_fallback.active_prompt_preset_id = presetId;
      syncActivePromptPresetSelection();
      refreshResolvedRefinementPromptInSettings();
      await persistSettings();
      renderAIFallbackSettingsUi();
    } else if (action === "new-preset") {
      const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
      if (hasPendingUserChanges) await persistSettings();
      settings.ai_fallback.active_prompt_preset_id = NEW_REFINEMENT_PROMPT_OPTION_ID;
      settings.ai_fallback.prompt_profile = "custom";
      settings.ai_fallback.custom_prompt_enabled = true;
      settings.ai_fallback.use_default_prompt = false;
      settings.ai_fallback.custom_prompt = "";
      if (dom.aiFallbackPromptPresetName) dom.aiFallbackPromptPresetName.value = "";
      if (dom.aiFallbackCustomPrompt) dom.aiFallbackCustomPrompt.value = "";
      refreshResolvedRefinementPromptInSettings();
      renderAIFallbackSettingsUi();
    } else if (action === "delete-chip-preset") {
      e.stopPropagation();
      const ai = settings.ai_fallback;
      ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
      const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
        ai.prompt_presets,
        presetId
      );
      if (!selectedUserPreset) return;
      ai.prompt_presets = ai.prompt_presets.filter((p) => p.id !== selectedUserPreset.id);
      ai.custom_prompt = selectedUserPreset.prompt;
      ai.active_prompt_preset_id = DEFAULT_REFINEMENT_PROMPT_PRESET;
      syncActivePromptPresetSelection();
      refreshResolvedRefinementPromptInSettings();
      await persistSettings();
      renderAIFallbackSettingsUi();
      showToast({
        type: "info",
        title: "Preset deleted",
        message: `Deleted "${selectedUserPreset.name}".`,
        duration: 2600,
      });
    }
  });

  dom.aiFallbackPromptPresetSave?.addEventListener("click", async () => {
    if (!settings || !dom.aiFallbackCustomPrompt || !dom.aiFallbackPromptPresetName) return;
    ensureAIFallbackSettingsDefaults();
    const prompt = dom.aiFallbackCustomPrompt.value.trim();
    if (!prompt) {
      showToast({
        type: "warning",
        title: "Prompt is empty",
        message: "Enter a prompt before saving a preset.",
        duration: 3000,
      });
      return;
    }

    const ai = settings.ai_fallback;
    ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
    ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
      ai.active_prompt_preset_id,
      ai.prompt_profile,
      ai.prompt_presets
    );

    const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
      ai.prompt_presets,
      ai.active_prompt_preset_id
    );
    const activePresetSelection = dom.aiFallbackPromptPreset?.value || ai.active_prompt_preset_id;
    const requestedName = dom.aiFallbackPromptPresetName.value.trim();

    if (selectedUserPreset) {
      const nextName = requestedName || selectedUserPreset.name;
      ai.prompt_presets = ai.prompt_presets.map((preset) =>
        preset.id === selectedUserPreset.id
          ? { ...preset, name: nextName, prompt }
          : preset
      );
      ai.active_prompt_preset_id = toUserRefinementPromptOptionId(selectedUserPreset.id);
      showToast({
        type: "success",
        title: "Preset updated",
        message: `Updated "${nextName}".`,
        duration: 2600,
      });
    } else if (activePresetSelection === NEW_REFINEMENT_PROMPT_OPTION_ID) {
      if (!requestedName) {
        showToast({
          type: "warning",
          title: "Name required",
          message: "Enter a preset name to create a new preset.",
          duration: 3000,
        });
        return;
      }
      const existingIds = new Set(ai.prompt_presets.map((preset) => preset.id));
      const newId = createUserPromptPresetId(requestedName, existingIds);
      ai.prompt_presets.push({
        id: newId,
        name: requestedName,
        prompt,
      });
      ai.active_prompt_preset_id = toUserRefinementPromptOptionId(newId);
      showToast({
        type: "success",
        title: "Preset saved",
        message: `Saved "${requestedName}".`,
        duration: 2600,
      });
    } else {
      showToast({
        type: "info",
        title: "Select New Preset",
        message: "Choose 'New Preset…' in the dropdown to create a new user preset.",
        duration: 3000,
      });
      return;
    }

    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackPromptPresetDelete?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const ai = settings.ai_fallback;
    ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
    ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
      ai.active_prompt_preset_id,
      ai.prompt_profile,
      ai.prompt_presets
    );
    const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
      ai.prompt_presets,
      ai.active_prompt_preset_id
    );
    if (!selectedUserPreset) {
      showToast({
        type: "warning",
        title: "Select user preset",
        message: "Only user presets can be deleted.",
        duration: 2600,
      });
      return;
    }

    ai.prompt_presets = ai.prompt_presets.filter((preset) => preset.id !== selectedUserPreset.id);
    ai.custom_prompt = selectedUserPreset.prompt;
    ai.active_prompt_preset_id = DEFAULT_REFINEMENT_PROMPT_PRESET;
    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
    showToast({
      type: "info",
      title: "Preset deleted",
      message: `Deleted "${selectedUserPreset.name}".`,
      duration: 2600,
    });
  });

  dom.aiFallbackCustomPrompt?.addEventListener("input", () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    const activePresetId = normalizeActiveRefinementPromptPresetId(
      settings.ai_fallback.active_prompt_preset_id,
      settings.ai_fallback.prompt_profile,
      settings.ai_fallback.prompt_presets
    );
    const isUserPreset = activePresetId.startsWith("user:");
    const isEditablePreset =
      activePresetId === "custom"
      || activePresetId === NEW_REFINEMENT_PROMPT_OPTION_ID
      || isUserPreset;
    if (!isEditablePreset) return;
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value;
    settings.ai_fallback.active_prompt_preset_id = activePresetId;
    settings.ai_fallback.prompt_profile = "custom";
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    refreshResolvedRefinementPromptInSettings();
  });

  dom.aiFallbackCustomPrompt?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    const activePresetId = normalizeActiveRefinementPromptPresetId(
      settings.ai_fallback.active_prompt_preset_id,
      settings.ai_fallback.prompt_profile,
      settings.ai_fallback.prompt_presets
    );
    const isUserPreset = activePresetId.startsWith("user:");
    const isEditablePreset =
      activePresetId === "custom"
      || activePresetId === NEW_REFINEMENT_PROMPT_OPTION_ID
      || isUserPreset;
    if (!isEditablePreset) {
      renderAIFallbackSettingsUi();
      return;
    }
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value.trim();
    settings.ai_fallback.active_prompt_preset_id = activePresetId;
    settings.ai_fallback.prompt_profile = "custom";
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    refreshResolvedRefinementPromptInSettings();
    if (activePresetId === "custom") {
      await persistSettings();
    }
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

  onChangePersist(dom.micGain);

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

  onChangePersist(dom.vadThreshold);

  dom.vadSilence?.addEventListener("input", () => {
    if (!settings || !dom.vadSilence) return;
    const value = Math.max(200, Math.min(4000, Number(dom.vadSilence.value)));
    settings.vad_silence_ms = value;
    if (dom.vadSilenceValue) {
      dom.vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
    }
    updateRangeAria("vad-silence", value);
  });

  onChangePersist(dom.vadSilence);

  dom.overlayColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayColor) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_color = dom.overlayColor.value;
    } else {
      settings.overlay_color = dom.overlayColor.value;
    }
  });

  onChangePersist(dom.overlayColor);

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

  onChangePersist(dom.overlayMinRadius);

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

  onChangePersist(dom.overlayMaxRadius);

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

  onChangePersist(dom.overlayRise);

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

  onChangePersist(dom.overlayFall);

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

  onChangePersist(dom.overlayOpacityInactive);

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

  onChangePersist(dom.overlayOpacityActive);

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

  dom.overlayRefiningIndicatorEnabled?.addEventListener("change", async () => {
    if (!settings || !dom.overlayRefiningIndicatorEnabled) return;
    settings.overlay_refining_indicator_enabled = dom.overlayRefiningIndicatorEnabled.checked;
    await persistSettings();
  });

  dom.overlayRefiningIndicatorPreset?.addEventListener("change", async () => {
    if (!settings || !dom.overlayRefiningIndicatorPreset) return;
    const value = dom.overlayRefiningIndicatorPreset.value;
    settings.overlay_refining_indicator_preset =
      value === "subtle" || value === "intense" ? value : "standard";
    await persistSettings();
  });

  dom.overlayRefiningIndicatorColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorColor) return;
    settings.overlay_refining_indicator_color = dom.overlayRefiningIndicatorColor.value;
  });

  onChangePersist(dom.overlayRefiningIndicatorColor);

  // Accent color picker — live preview while dragging
  dom.accentColor?.addEventListener("input", () => {
    if (!settings || !dom.accentColor) return;
    settings.accent_color = dom.accentColor.value;
    applyAccentColor(dom.accentColor.value);
  });

  // Accent color picker — persist when picker closes
  dom.accentColor?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  // Reset accent color to default teal
  dom.accentColorReset?.addEventListener("click", async () => {
    if (!settings) return;
    settings.accent_color = DEFAULT_ACCENT_COLOR;
    if (dom.accentColor) dom.accentColor.value = DEFAULT_ACCENT_COLOR;
    applyAccentColor(DEFAULT_ACCENT_COLOR);
    await persistSettings();
  });

  dom.overlayRefiningIndicatorSpeed?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorSpeed) return;
    const value = Math.max(450, Math.min(3000, Number(dom.overlayRefiningIndicatorSpeed.value)));
    settings.overlay_refining_indicator_speed_ms = value;
    if (dom.overlayRefiningIndicatorSpeedValue) {
      dom.overlayRefiningIndicatorSpeedValue.textContent = `${value} ms`;
    }
    updateRangeAria("overlay-refining-indicator-speed", value);
  });

  dom.overlayRefiningIndicatorSpeed?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });

  dom.overlayRefiningIndicatorRange?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorRange) return;
    const value = Math.max(60, Math.min(180, Number(dom.overlayRefiningIndicatorRange.value)));
    settings.overlay_refining_indicator_range = value;
    if (dom.overlayRefiningIndicatorRangeValue) {
      dom.overlayRefiningIndicatorRangeValue.textContent = `${value}%`;
    }
    updateRangeAria("overlay-refining-indicator-range", value);
  });

  dom.overlayRefiningIndicatorRange?.addEventListener("change", async () => {
    if (!settings) return;
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
  dom.applyOverlayBtn?.addEventListener("click", async () => {
    if (!settings) return;
    await persistSettings();
    showToast({ title: "Applied", message: "Overlay settings applied", type: "success" });
  });

  // Topic keywords reset
  dom.topicKeywordsReset?.addEventListener("click", async () => {
    if (!settings) return;
    settings.topic_keywords = cloneDefaultTopicKeywords();
    setTopicKeywords(settings.topic_keywords);
    await renderTopicKeywords();
    await persistSettings();
  });

  // Voice Output Settings
  dom.voiceOutputDefaultProvider?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.default_provider = dom.voiceOutputDefaultProvider!
      .value as "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
    await refreshProviderVoices("default");
    updateProviderMutualExclusion();
    await refreshProviderAvailability();
    await persistSettings();
  });

  dom.voiceOutputFallbackProvider?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.fallback_provider = dom.voiceOutputFallbackProvider!
      .value as "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
    await refreshProviderVoices("fallback");
    updateProviderMutualExclusion();
    await refreshProviderAvailability();
    await persistSettings();
  });

  dom.voiceOutputPolicy?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.output_policy = dom.voiceOutputPolicy!
      .value as "agent_replies_only" | "replies_and_events" | "explicit_only";
    await persistSettings();
  });

  dom.voiceOutputDeviceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputDeviceSelect) return;
    settings.voice_output_settings.output_device = dom.voiceOutputDeviceSelect.value || "default";
    await persistSettings();
  });

  dom.voiceOutputWindowsVoiceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputWindowsVoiceSelect) return;
    settings.voice_output_settings.voice_id_windows = dom.voiceOutputWindowsVoiceSelect.value.trim();
    await persistSettings();
  });

  dom.voiceOutputFallbackVoiceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputFallbackVoiceSelect) return;
    settings.voice_output_settings.voice_id_windows_fallback = dom.voiceOutputFallbackVoiceSelect.value.trim();
    await persistSettings();
  });

  dom.voiceOutputAutoLanguageVoice?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputAutoLanguageVoice) return;
    settings.voice_output_settings.auto_voice_by_detected_language = dom.voiceOutputAutoLanguageVoice.checked;
    await persistSettings();
  });

  // Voice Output Rate slider (live update)
  dom.voiceOutputRate?.addEventListener("input", () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputRate) return;
    const rate = parseFloat(dom.voiceOutputRate.value);
    settings.voice_output_settings.rate = rate;
    if (dom.voiceOutputRateValue) {
      dom.voiceOutputRateValue.textContent = rate.toFixed(2);
    }
  });

  // Voice Output Rate slider (persist on change)
  dom.voiceOutputRate?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputRate) return;
    settings.voice_output_settings.rate = parseFloat(dom.voiceOutputRate.value);
    await persistSettings();
  });

  // Voice Output Volume slider (live update)
  dom.voiceOutputVolume?.addEventListener("input", () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputVolume) return;
    const volume = parseFloat(dom.voiceOutputVolume.value);
    settings.voice_output_settings.volume = volume;
    if (dom.voiceOutputVolumeValue) {
      dom.voiceOutputVolumeValue.textContent = volume.toFixed(2);
    }
  });

  // Voice Output Volume slider (persist on change)
  dom.voiceOutputVolume?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputVolume) return;
    settings.voice_output_settings.volume = parseFloat(dom.voiceOutputVolume.value);
    await persistSettings();
  });

  // Voice Output Test Button
  dom.voiceOutputTestBtn?.addEventListener("click", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputTestBtn || !dom.voiceOutputTestStatus) return;
    const provider = settings.voice_output_settings.default_provider;
    dom.voiceOutputTestStatus.textContent = "Testing…";
    try {
      const result = await invoke<TtsSpeakResult>("test_tts_provider", { provider });
      if (result.used_fallback) {
        const preferred = result.preferred_provider || provider;
        dom.voiceOutputTestStatus.textContent = `⚠ Fallback used: ${preferred} -> ${result.provider_used}`;
      } else {
        dom.voiceOutputTestStatus.textContent = `✓ ${result.provider_used} responded.`;
      }
    } catch (e) {
      const formatted = formatTtsTestError(e);
      dom.voiceOutputTestStatus.textContent = `Error: ${formatted}`;
      dom.voiceOutputTestStatus.title = formatted;
      console.error("Voice output test failed:", formatted);
    }
  });

  // Voice Output Piper Paths (text inputs)
  dom.voiceOutputPiperBinary?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperBinary) return;
    settings.voice_output_settings.piper_binary_path = dom.voiceOutputPiperBinary.value;
    await persistSettings();
  });

  dom.voiceOutputPiperModel?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperModel) return;
    settings.voice_output_settings.piper_model_path = dom.voiceOutputPiperModel.value;
    await persistSettings();
  });

  dom.voiceOutputPiperModelDir?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperModelDir) return;
    settings.voice_output_settings.piper_model_dir = dom.voiceOutputPiperModelDir.value;
    await persistSettings();
  });

  // Voice Output Qwen3 endpoint/model settings
  dom.voiceOutputQwenEndpoint?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenEndpoint) return;
    settings.voice_output_settings.qwen3_tts_endpoint = dom.voiceOutputQwenEndpoint.value;
    await persistSettings();
  });

  dom.voiceOutputQwenModel?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenModel) return;
    settings.voice_output_settings.qwen3_tts_model = dom.voiceOutputQwenModel.value;
    await persistSettings();
  });

  dom.voiceOutputQwenVoice?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenVoice) return;
    settings.voice_output_settings.qwen3_tts_voice = dom.voiceOutputQwenVoice.value;
    await persistSettings();
  });

  dom.voiceOutputQwenApiKey?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenApiKey) return;
    settings.voice_output_settings.qwen3_tts_api_key = dom.voiceOutputQwenApiKey.value;
    await persistSettings();
  });

  dom.voiceOutputQwenTimeoutSec?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenTimeoutSec) return;
    const parsed = Number.parseInt(dom.voiceOutputQwenTimeoutSec.value, 10);
    settings.voice_output_settings.qwen3_tts_timeout_sec = Number.isFinite(parsed)
      ? Math.max(3, Math.min(180, parsed))
      : 45;
    dom.voiceOutputQwenTimeoutSec.value = String(settings.voice_output_settings.qwen3_tts_timeout_sec);
    await persistSettings();
  });

  let backendSwitchBusy = false;
  const setWhisperBackendPreference = async (backend: "cuda" | "vulkan") => {
    if (!settings || backendSwitchBusy) return;
    const normalizedCurrent = (settings.local_backend_preference ?? "auto").trim().toLowerCase();
    if (normalizedCurrent === backend) return;
    backendSwitchBusy = true;
    if (dom.gpuBackendCudaBtn) dom.gpuBackendCudaBtn.disabled = true;
    if (dom.gpuBackendVulkanBtn) dom.gpuBackendVulkanBtn.disabled = true;
    try {
      settings.local_backend_preference = backend;
      renderHero();
      await persistSettings();
      renderHero();
      showToast({
        type: "success",
        title: "Backend updated",
        message: `Whisper backend preference set to ${backend.toUpperCase()}.`,
        duration: 2200,
      });
    } finally {
      backendSwitchBusy = false;
      if (dom.gpuBackendCudaBtn) dom.gpuBackendCudaBtn.disabled = false;
      if (dom.gpuBackendVulkanBtn) dom.gpuBackendVulkanBtn.disabled = false;
    }
  };

  dom.gpuBackendCudaBtn?.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    void setWhisperBackendPreference("cuda");
  });

  dom.gpuBackendVulkanBtn?.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    void setWhisperBackendPreference("vulkan");
  });

  // GPU VRAM Purge on click
  const purgeTrigger = dom.gpuPurgeBtn ?? dom.gpuStatusItem;
  purgeTrigger?.addEventListener("click", async () => {
    if (!dom.gpuVramLabel) return;
    const originalVramText = dom.gpuVramLabel.textContent;
    if (dom.gpuPurgeBtn) {
      dom.gpuPurgeBtn.disabled = true;
    } else if (dom.gpuStatusItem) {
      dom.gpuStatusItem.style.pointerEvents = "none";
    }
    dom.gpuVramLabel.textContent = "Purging...";
    try {
      await invoke("purge_gpu_memory");
      dom.gpuVramLabel.textContent = "Purged ✓";
      setTimeout(() => {
        dom.gpuVramLabel!.textContent = originalVramText;
        if (dom.gpuPurgeBtn) {
          dom.gpuPurgeBtn.disabled = false;
        } else if (dom.gpuStatusItem) {
          dom.gpuStatusItem.style.pointerEvents = "auto";
        }
      }, 2000);
    } catch (error) {
      dom.gpuVramLabel.textContent = "Error";
      if (dom.gpuPurgeBtn) {
        dom.gpuPurgeBtn.disabled = false;
      } else if (dom.gpuStatusItem) {
        dom.gpuStatusItem.style.pointerEvents = "auto";
      }
      setTimeout(() => {
        dom.gpuVramLabel!.textContent = originalVramText;
      }, 3000);
    }
  });

  // Keyboard fallback for legacy clickable GPU item only.
  if (!dom.gpuPurgeBtn) {
    dom.gpuStatusItem?.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        dom.gpuStatusItem?.click();
      }
    });
  }
}
