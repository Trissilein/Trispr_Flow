// DOM event listeners setup

import { invoke } from "@tauri-apps/api/core";
import type {
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthStatus,
  Settings,
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
} from "./settings";
import { renderSettings } from "./settings";
import { renderHero, updateDeviceLineClamp, updateThresholdMarkers } from "./ui-state";
import { refreshModels, refreshModelsDir } from "./models";
import { setHistoryTab, buildConversationHistory, buildConversationText, setSearchQuery, setTopicKeywords, DEFAULT_TOPICS, scheduleHistoryRender, syncHistoryToolbarState } from "./history";
import { setHistoryAlias, setHistoryFontSize, syncHistoryAliasesIntoSettings } from "./history-preferences";
import { isPanelId, togglePanel } from "./panels";
import { setupHotkeyRecorder } from "./hotkeys";
import { updateRangeAria } from "./accessibility";
import { showToast } from "./toast";
import { dbToLevel, VAD_DB_FLOOR } from "./ui-helpers";
import { applyAccentColor, DEFAULT_ACCENT_COLOR } from "./utils";
import {
  DEFAULT_REFINEMENT_PROMPT_PRESET,
  normalizeRefinementPromptPreset,
  resolveEffectiveRefinementPrompt,
} from "./refinement-prompts";
import {
  autoStartLocalRuntimeIfNeeded,
  ensureLocalRuntimeReady,
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

function applyExecutionModeInSettings(mode: AIExecutionMode): void {
  if (!settings) return;
  settings.ai_fallback.execution_mode = "local_primary";
  settings.ai_fallback.provider = "ollama";
  settings.postproc_llm_provider = "ollama";
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
        endpoint: "http://localhost:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "manual",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.17.5",
        last_health_check: null,
      },
    };
  }
  if (!settings.providers.ollama) {
    settings.providers.ollama = {
      endpoint: "http://localhost:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      runtime_target_version: "0.17.5",
      last_health_check: null,
    };
  }
  settings.ai_fallback.strict_local_mode ??= true;
  settings.ai_fallback.preserve_source_language ??= true;
  settings.ai_fallback.prompt_profile = normalizeRefinementPromptPreset(
    settings.ai_fallback.prompt_profile
  );
  settings.ai_fallback.low_latency_mode ??= false;
  settings.ai_fallback.custom_prompt_enabled = settings.ai_fallback.prompt_profile === "custom";
  settings.ai_fallback.use_default_prompt = false;
  settings.ai_fallback.fallback_provider = normalizeCloudProvider(
    settings.ai_fallback.fallback_provider ?? null
  );
  settings.ai_fallback.execution_mode = normalizeExecutionMode(settings.ai_fallback.execution_mode);
  if (!settings.ai_fallback.fallback_provider && settings.ai_fallback.provider !== "ollama") {
    settings.ai_fallback.fallback_provider = normalizeCloudProvider(settings.ai_fallback.provider);
  }
  // Online fallback lane is intentionally roadmap-only for now.
  settings.ai_fallback.execution_mode = "local_primary";
  settings.ai_fallback.provider = "ollama";
  settings.postproc_llm_provider = "ollama";
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
  settings.providers.ollama.runtime_target_version ??= "0.17.5";
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
  settings.module_settings ??= {
    enabled_modules: [],
    consented_permissions: {},
    module_overrides: {},
  };
  settings.module_settings.enabled_modules ??= [];
  settings.module_settings.consented_permissions ??= {};
  settings.module_settings.module_overrides ??= {};
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
    settings.providers.ollama.available_models = models;
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
type MainTab = "transcription" | "settings" | "ai-refinement" | "modules";
let aiRefinementTabRefreshInFlight: Promise<void> | null = null;

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
  const isTranscription = tab === "transcription";
  const isSettings = tab === "settings";
  const isAiRefinement = tab === "ai-refinement";
  const isModules = tab === "modules";

  dom.tabBtnTranscription?.classList.toggle("active", isTranscription);
  dom.tabBtnSettings?.classList.toggle("active", isSettings);
  dom.tabBtnAiRefinement?.classList.toggle("active", isAiRefinement);
  dom.tabBtnModules?.classList.toggle("active", isModules);

  dom.tabBtnTranscription?.setAttribute("aria-selected", isTranscription.toString());
  dom.tabBtnSettings?.setAttribute("aria-selected", isSettings.toString());
  dom.tabBtnAiRefinement?.setAttribute("aria-selected", isAiRefinement.toString());
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
  if (dom.tabModules) {
    dom.tabModules.style.removeProperty("display");
    dom.tabModules.classList.toggle("active", isModules);
  }

  // Persist to localStorage
  try {
    localStorage.setItem("trispr-active-tab", tab);
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
  try {
    const savedTab = localStorage.getItem("trispr-active-tab") as MainTab | null;
    if (
      savedTab === "settings" ||
      savedTab === "transcription" ||
      savedTab === "ai-refinement" ||
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

  // Hotkey recording functionality
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
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      normalizeRefinementPromptPreset(settings.ai_fallback.prompt_profile),
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
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
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      normalizeRefinementPromptPreset(settings.ai_fallback.prompt_profile),
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
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
    const enabling = dom.aiFallbackEnabled!.checked;

    if (enabling) {
      // Check if Ollama is available; if not, show modal
      const runtimeInfo = await invoke<any>("detect_ollama_runtime").catch(() => null);
      const ollamaDetected = runtimeInfo?.found === true;

      if (!ollamaDetected) {
        const userWantsInstall = await showOllamaRequiredModal();
        if (!userWantsInstall) {
          // User clicked "Später" — don't enable AI fallback yet
          dom.aiFallbackEnabled!.checked = false;
          return;
        }
      }
    }

    settings.ai_fallback.enabled = enabling;
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
    if (settings.ai_fallback.enabled) {
      void autoStartLocalRuntimeIfNeeded("enable_toggle").finally(() => {
        renderAIFallbackSettingsUi();
        renderOllamaModelManager();
      });
    } else {
      // Stop Ollama runtime when AI refinement is disabled
      try {
        await invoke("stop_ollama_runtime");
      } catch (error) {
        console.warn("Failed to stop Ollama runtime:", error);
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
    const switched =
      settings.ai_fallback.execution_mode !== "local_primary" ||
      settings.ai_fallback.provider !== "ollama";
    applyExecutionModeInSettings("local_primary");
    await refreshOllamaRuntimeState({ force: true });
    if (getOllamaRuntimeCardState().healthy) {
      await refreshOllamaInstalledModels();
    }
    await persistSettings();
    refreshAIUi();
    if (notify && switched) {
      showToast({
        type: "success",
        title: "Local runtime active",
        message: "Using local Ollama as primary runtime.",
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

  // Combined refresh: runtime state + models + version catalog + detect
  dom.aiFallbackLocalRefreshAction?.addEventListener("click", async () => {
    renderAIFallbackSettingsUi();
    await Promise.all([
      refreshOllamaRuntimeAndModels(),
      refreshOllamaRuntimeVersionCatalog(true),
    ]);
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
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      normalizeRefinementPromptPreset(settings.ai_fallback.prompt_profile),
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
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

  dom.aiFallbackPromptPreset?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackPromptPreset) return;
    ensureAIFallbackSettingsDefaults();
    const profile = normalizeRefinementPromptPreset(dom.aiFallbackPromptPreset.value);
    settings.ai_fallback.prompt_profile = profile;
    settings.ai_fallback.custom_prompt_enabled = profile === "custom";
    settings.ai_fallback.use_default_prompt = false;
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      profile,
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackCustomPrompt?.addEventListener("input", () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    if (settings.ai_fallback.prompt_profile !== "custom") return;
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value;
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      "custom",
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
  });

  dom.aiFallbackCustomPrompt?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    if (settings.ai_fallback.prompt_profile !== "custom") {
      renderAIFallbackSettingsUi();
      return;
    }
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value.trim();
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
      "custom",
      resolveEffectiveAsrLanguageHint(settings.language_mode, settings.language_pinned),
      settings.ai_fallback.custom_prompt,
      settings.ai_fallback.preserve_source_language
    );
    await persistSettings();
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
      .value as "windows_native" | "local_custom";
    await persistSettings();
  });

  dom.voiceOutputFallbackProvider?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.fallback_provider = dom.voiceOutputFallbackProvider!
      .value as "windows_native" | "local_custom";
    await persistSettings();
  });

  dom.voiceOutputPolicy?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.output_policy = dom.voiceOutputPolicy!
      .value as "agent_replies_only" | "replies_and_events" | "explicit_only";
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
      await invoke("test_tts_provider", { provider });
      dom.voiceOutputTestStatus.textContent = "✓ Provider responded.";
    } catch (e) {
      dom.voiceOutputTestStatus.textContent = `Error: ${e}`;
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

  // GPU VRAM Purge Button
  dom.purgeVramBtn?.addEventListener("click", async () => {
    if (!dom.purgeVramBtn) return;
    const originalText = dom.purgeVramBtn.textContent;
    dom.purgeVramBtn.disabled = true;
    dom.purgeVramBtn.textContent = "Purging...";
    try {
      await invoke("purge_gpu_memory");
      dom.purgeVramBtn.textContent = "VRAM Purged ✓";
      setTimeout(() => {
        dom.purgeVramBtn!.textContent = originalText;
        dom.purgeVramBtn!.disabled = false;
      }, 2000);
    } catch (error) {
      dom.purgeVramBtn.textContent = `Error: ${error}`;
      dom.purgeVramBtn.disabled = false;
      setTimeout(() => {
        dom.purgeVramBtn!.textContent = originalText;
      }, 3000);
    }
  });
}
