// AI Refinement wiring (R2 slice 5).
//
// Covers: AI Refinement + Post-Processing + topic-keywords + provider-chain +
// prompt-editor + local-runtime per CONTEXT.md. Language controls
// (languageSelect, languagePinnedToggle, whisperInputLanguageSelect) are folded
// in because they mutate AI-Refinement prompt state. See OQ-3 clause 8
// (project-spec/decisions/2026-05-15/refactoring-plan.md).

import { invoke } from "@tauri-apps/api/core";
import type {
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthStatus,
  Settings,
} from "../types";
import {
  CLOUD_PROVIDER_IDS,
  CLOUD_PROVIDER_LABELS,
  normalizeCloudProvider,
  normalizeExecutionMode,
  normalizeAuthMethodPreference,
  isVerifiedAuthStatus,
  normalizeAIFallbackProvider,
} from "../ai-provider-utils";
import { settings } from "../state";
import * as dom from "../dom-refs";
import {
  persistSettings,
  renderAIFallbackSettingsUi,
  renderSettings,
  renderTopicKeywords,
  resolveEffectiveAsrLanguageHint,
  derivePostprocLanguageFromAsr,
  syncDerivedLanguageSettings,
  addVocabRow,
} from "../settings";
import { renderHero } from "../ui-state";
import { setTopicKeywords, DEFAULT_TOPICS } from "../history";
import { showToast } from "../toast";
import { updateRangeAria } from "../accessibility";
import {
  BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS,
  DEFAULT_REFINEMENT_PROMPT_PRESET,
  findUserRefinementPromptPresetByOptionId,
  NEW_REFINEMENT_PROMPT_OPTION_ID,
  normalizeActiveRefinementPromptPresetId,
  normalizeRefinementPromptPreset,
  normalizeUserRefinementPromptPresets,
  removePresetOverride,
  resolveEffectiveRefinementPrompt,
  setPresetOverride,
  toUserRefinementPromptOptionId,
  type BuiltInRefinementPromptPreset,
} from "../refinement-prompts";
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
} from "../ollama-models";
import { normalizeModelTag } from "../ollama-tag-utils";
import { onChangePersist, scheduleSettingsRender } from "./wire-helpers";

// ── Module-level constants ──────────────────────────────────────────────────

const LOCAL_BACKENDS = ["ollama", "lm_studio", "oobabooga"] as const;
const AI_REFINEMENT_MODULE_ID = "ai_refinement";
const AI_REFINEMENT_MIGRATION_FLAG_KEY = "ai_refinement.migrated_legacy";

// ── Module-level state ──────────────────────────────────────────────────────

let authModalProvider: CloudAIFallbackProvider | null = null;

// ── Private helpers ─────────────────────────────────────────────────────────

// Duplicated one-liners per OQ-3 clause 3 (private, not exported; matching
// the pattern established in refinement-pipeline-graph.ts and
// voice-output-console.ts).
function isModuleEnabled(moduleId: string): boolean {
  return (
    settings?.module_settings?.enabled_modules?.includes(moduleId) ?? false
  );
}

function isAiRefinementModuleEnabled(): boolean {
  return isModuleEnabled(AI_REFINEMENT_MODULE_ID);
}

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
  return normalizeCloudProvider(
    settings?.ai_fallback?.fallback_provider ?? null,
  );
}

function getFallbackProvider(): CloudAIFallbackProvider | null {
  return normalizeCloudProvider(
    settings?.ai_fallback?.fallback_provider ?? null,
  );
}

function isProviderVerified(provider: CloudAIFallbackProvider | null): boolean {
  if (!provider || !settings) return false;
  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return false;
  return isVerifiedAuthStatus(providerSettings.auth_status);
}

function getAIFallbackProviderSettings(provider: AIFallbackProvider) {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  // Ollama uses OllamaSettings, handled separately
  return null;
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
    new Set(settings.module_settings.enabled_modules),
  );

  const overrides = settings.module_settings.module_overrides;
  const migrationDone = overrides[AI_REFINEMENT_MIGRATION_FLAG_KEY] === true;
  if (
    settings.ai_fallback.enabled &&
    !settings.module_settings.enabled_modules.includes(
      AI_REFINEMENT_MODULE_ID,
    ) &&
    !migrationDone
  ) {
    settings.module_settings.enabled_modules.push(AI_REFINEMENT_MODULE_ID);
  }

  const moduleEnabledNow = settings.module_settings.enabled_modules.includes(
    AI_REFINEMENT_MODULE_ID,
  );
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
  if (
    !LOCAL_BACKENDS.includes(
      settings.ai_fallback.provider as (typeof LOCAL_BACKENDS)[number],
    )
  ) {
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
        message:
          "Fallback provider is locked/unverified. Switched back to local Ollama.",
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
    target.closest("button,select,input,textarea,summary,details,label,a"),
  );
}

function cloneDefaultTopicKeywords(): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  Object.entries(DEFAULT_TOPICS).forEach(([topic, words]) => {
    out[topic] = [...words];
  });
  return out;
}

function normalizeTopicKeywordsInput(
  input: Record<string, unknown> | null | undefined,
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
    resolveEffectiveAsrLanguageHint(
      settings.language_mode,
      settings.language_pinned,
    ),
    settings.ai_fallback.custom_prompt,
    settings.ai_fallback.preserve_source_language,
    settings.ai_fallback.model,
    settings.ai_fallback.prompt_preset_overrides,
  );
}

function syncActivePromptPresetSelection() {
  if (!settings) return;
  const ai = settings.ai_fallback;
  ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
  ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    ai.active_prompt_preset_id,
    ai.prompt_profile,
    ai.prompt_presets,
  );
  if (ai.active_prompt_preset_id === NEW_REFINEMENT_PROMPT_OPTION_ID) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
    ai.use_default_prompt = false;
    return;
  }
  const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
    ai.prompt_presets,
    ai.active_prompt_preset_id,
  );
  if (selectedUserPreset) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
    ai.custom_prompt = selectedUserPreset.prompt;
  } else {
    ai.prompt_profile = normalizeRefinementPromptPreset(
      ai.active_prompt_preset_id,
    );
    ai.custom_prompt_enabled = ai.prompt_profile === "custom";
  }
  ai.use_default_prompt = false;
}

function createUserPromptPresetId(
  baseName: string,
  existingIds: Set<string>,
): string {
  const slug =
    baseName
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
  if (
    !settings ||
    !dom.aiFallbackCustomPrompt ||
    !dom.aiFallbackPromptPresetName
  ) {
    return false;
  }
  const ai = settings.ai_fallback;
  ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
  ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    ai.active_prompt_preset_id,
    ai.prompt_profile,
    ai.prompt_presets,
  );
  const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
    ai.prompt_presets,
    ai.active_prompt_preset_id,
  );
  if (!selectedUserPreset) return false;

  const nextPrompt = dom.aiFallbackCustomPrompt.value.trim();
  const nextName =
    dom.aiFallbackPromptPresetName.value.trim() || selectedUserPreset.name;
  if (!nextPrompt) {
    return false;
  }
  if (
    nextPrompt === selectedUserPreset.prompt &&
    nextName === selectedUserPreset.name
  ) {
    return false;
  }

  const nextPrevious =
    nextPrompt !== selectedUserPreset.prompt
      ? selectedUserPreset.prompt
      : selectedUserPreset.previous_prompt;

  ai.prompt_presets = ai.prompt_presets.map((preset) =>
    preset.id === selectedUserPreset.id
      ? {
          ...preset,
          name: nextName,
          prompt: nextPrompt,
          previous_prompt: nextPrevious,
        }
      : preset,
  );
  ai.active_prompt_preset_id = toUserRefinementPromptOptionId(
    selectedUserPreset.id,
  );
  syncActivePromptPresetSelection();
  refreshResolvedRefinementPromptInSettings();
  return true;
}

/** Returns true if the user confirmed (or no confirmation was needed), false if cancelled. */
function confirmDiscardBuiltInEdits(): boolean {
  if (!dom.aiFallbackCustomPrompt) return true;
  // `.has-unsaved-edits` is the authoritative dirty flag: the `input` listener
  // adds it on keystroke, the renderer removes it when the textarea is re-sourced
  // from the effective prompt. No flag → no unsaved edits → no confirmation.
  if (!dom.aiFallbackCustomPrompt.classList.contains("has-unsaved-edits"))
    return true;
  return window.confirm("Discard unsaved changes to this preset?");
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
        available_models: [
          "claude-3-5-sonnet-20241022",
          "claude-3-5-haiku-20241022",
          "claude-3-opus-20240229",
        ],
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
        available_models: [
          "gemini-2.0-flash",
          "gemini-1.5-pro",
          "gemini-1.5-flash",
        ],
        preferred_model: "gemini-2.0-flash",
      },
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "manual",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.20.2",
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
      runtime_target_version: "0.20.2",
      last_health_check: null,
    };
  }
  settings.ai_fallback.strict_local_mode ??= true;
  settings.ai_fallback.preserve_source_language ??= true;
  settings.ai_fallback.prompt_profile = normalizeRefinementPromptPreset(
    settings.ai_fallback.prompt_profile,
  );
  settings.ai_fallback.prompt_presets = normalizeUserRefinementPromptPresets(
    settings.ai_fallback.prompt_presets,
  );
  settings.ai_fallback.active_prompt_preset_id =
    normalizeActiveRefinementPromptPresetId(
      settings.ai_fallback.active_prompt_preset_id,
      settings.ai_fallback.prompt_profile,
      settings.ai_fallback.prompt_presets,
    );
  settings.ai_fallback.low_latency_mode ??= false;
  syncActivePromptPresetSelection();
  settings.ai_fallback.fallback_provider = normalizeCloudProvider(
    settings.ai_fallback.fallback_provider ?? null,
  );
  settings.ai_fallback.execution_mode = normalizeExecutionMode(
    settings.ai_fallback.execution_mode,
  );
  if (
    !settings.ai_fallback.fallback_provider &&
    !LOCAL_BACKENDS.includes(
      settings.ai_fallback.provider as (typeof LOCAL_BACKENDS)[number],
    )
  ) {
    settings.ai_fallback.fallback_provider = normalizeCloudProvider(
      settings.ai_fallback.provider,
    );
  }
  // Online fallback lane is intentionally roadmap-only for now.
  settings.ai_fallback.execution_mode = "local_primary";
  // Preserve the selected local backend — only reset if something invalid crept in
  if (
    !LOCAL_BACKENDS.includes(
      settings.ai_fallback.provider as (typeof LOCAL_BACKENDS)[number],
    )
  ) {
    settings.ai_fallback.provider = "ollama";
  }
  settings.postproc_llm_provider = settings.ai_fallback.provider;
  settings.postproc_language = derivePostprocLanguageFromAsr(
    settings.language_mode,
    settings.language_pinned,
  );
  const effectiveLanguageHint = resolveEffectiveAsrLanguageHint(
    settings.language_mode,
    settings.language_pinned,
  );
  settings.postproc_llm_prompt = resolveEffectiveRefinementPrompt(
    settings.ai_fallback.prompt_profile,
    effectiveLanguageHint,
    settings.ai_fallback.custom_prompt,
    settings.ai_fallback.preserve_source_language,
    settings.ai_fallback.model,
    settings.ai_fallback.prompt_preset_overrides,
  );
  settings.topic_keywords = normalizeTopicKeywordsInput(
    settings.topic_keywords,
  );
  setTopicKeywords(settings.topic_keywords);
  settings.providers.ollama.runtime_source ??= "manual";
  settings.providers.ollama.runtime_path ??= "";
  settings.providers.ollama.runtime_version ??= "";
  settings.providers.ollama.runtime_target_version ??= "0.20.2";
  settings.providers.ollama.last_health_check ??= null;
  CLOUD_PROVIDER_IDS.forEach((provider) => {
    const providerSettings = getAIFallbackProviderSettings(provider);
    if (!providerSettings) return;
    providerSettings.auth_method_preference = normalizeAuthMethodPreference(
      providerSettings.auth_method_preference,
    );
    providerSettings.auth_status = isVerifiedAuthStatus(
      providerSettings.auth_status,
    )
      ? (providerSettings.auth_status as AIProviderAuthStatus)
      : "locked";
    providerSettings.auth_verified_at ??= null;
    if (
      !providerSettings.api_key_stored &&
      providerSettings.auth_status !== "verified_oauth"
    ) {
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
  settings.product_mode =
    settings.product_mode === "assistant" ? "assistant" : "transcribe";
  settings.hotkey_product_mode_toggle ??= "CommandOrControl+Shift+P";
  settings.hotkey_tts_stop ??= "CommandOrControl+Shift+F12";
  settings.overlay_tts_stop_enabled ??= true;
  settings.overlay_tts_stop_shape =
    settings.overlay_tts_stop_shape === "round" ? "round" : "compact";
  settings.overlay_tts_stop_color ??= "#4be0d4";
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
    const threshold = Number(
      settings.gdd_module_settings.one_click_confidence_threshold,
    );
    settings.gdd_module_settings.one_click_confidence_threshold =
      Number.isFinite(threshold) && threshold >= 0 && threshold <= 1
        ? threshold
        : 0.75;
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
    settings.confluence_settings.auth_mode === "api_token"
      ? "api_token"
      : "oauth";
  settings.confluence_settings.routing_memory ??= {};
  syncDerivedLanguageSettings();
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
          .filter((name) => name.length > 0),
      ),
    );
    settings.providers.ollama.available_models = mergedModels;
    if (!models.includes(settings.providers.ollama.preferred_model)) {
      settings.providers.ollama.preferred_model = models[0] ?? "";
    }
    if (
      settings.ai_fallback.provider === "ollama" &&
      !models.includes(settings.ai_fallback.model)
    ) {
      settings.ai_fallback.model =
        settings.providers.ollama.preferred_model || models[0] || "";
    }
    return;
  }

  const providerSettings = getAIFallbackProviderSettings(provider);
  if (!providerSettings) return;
  providerSettings.available_models = models;
  if (
    !providerSettings.preferred_model ||
    !models.includes(providerSettings.preferred_model)
  ) {
    providerSettings.preferred_model = models[0] ?? "";
  }
  const mode = normalizeExecutionMode(settings.ai_fallback.execution_mode);
  const fallbackProvider = getFallbackProvider();
  if (
    settings.ai_fallback.provider === provider ||
    (mode === "online_fallback" && fallbackProvider === provider)
  ) {
    if (!models.includes(settings.ai_fallback.model)) {
      settings.ai_fallback.model =
        providerSettings.preferred_model || models[0] || "";
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
    dom.aiAuthMethod.value = normalizeAuthMethodPreference(
      providerSettings.auth_method_preference,
    );
  }
  if (dom.aiAuthVerifyKey) {
    dom.aiAuthVerifyKey.disabled =
      normalizeAuthMethodPreference(providerSettings.auth_method_preference) ===
      "oauth";
  }
  if (dom.aiAuthStatus) {
    if (
      normalizeAuthMethodPreference(providerSettings.auth_method_preference) ===
      "oauth"
    ) {
      dom.aiAuthStatus.textContent = `OAuth for ${providerLabel} is coming soon. Use API key verification for now.`;
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

async function saveProviderApiKey(
  provider: CloudAIFallbackProvider,
  apiKey: string,
): Promise<void> {
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

async function clearProviderApiKey(
  provider: CloudAIFallbackProvider,
): Promise<void> {
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

async function verifyProviderCredentials(
  provider: CloudAIFallbackProvider,
): Promise<void> {
  if (!settings) return;
  const providerSettings = getAIFallbackProviderSettings(provider);
  const authMethod = normalizeAuthMethodPreference(
    providerSettings?.auth_method_preference,
  );
  if (authMethod === "oauth") {
    showToast({
      type: "info",
      title: "OAuth coming soon",
      message:
        "OAuth verification is not available yet. Use API key verification for now.",
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
    const result = await invoke<{
      message?: string;
      method?: string;
      verified_at?: string;
    }>("verify_provider_auth", { provider, method: authMethod });
    if (providerSettings) {
      providerSettings.auth_status =
        (result?.method as "verified_api_key" | "verified_oauth") ||
        "verified_api_key";
      providerSettings.auth_verified_at =
        result?.verified_at ?? new Date().toISOString();
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
      message:
        result?.message ?? `${provider} is unlocked for online fallback.`,
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

// ── Local-closure lifts (per OQ-3 clause 3) ────────────────────────────────

function getCompatSettings() {
  if (!settings) return null;
  const p = settings.ai_fallback.provider;
  if (p === "lm_studio")
    return (settings.providers.lm_studio ??= {
      endpoint: "http://127.0.0.1:1234",
      api_key: "",
      preferred_model: "",
      available_models: [],
    });
  if (p === "oobabooga")
    return (settings.providers.oobabooga ??= {
      endpoint: "http://127.0.0.1:5000",
      api_key: "",
      preferred_model: "",
      available_models: [],
    });
  return null;
}

async function setWhisperInputLanguage(mode: Settings["language_mode"]) {
  if (!settings) return;
  if (mode === "auto") {
    settings.language_mode = "auto";
    settings.language_pinned = false;
  } else {
    settings.language_mode = mode;
    settings.language_pinned = true;
  }
  settings.postproc_language = derivePostprocLanguageFromAsr(
    settings.language_mode,
    settings.language_pinned,
  );
  syncActivePromptPresetSelection();
  refreshResolvedRefinementPromptInSettings();
  await persistSettings();
  renderSettings();
}

async function activateLocalLane(notify = false) {
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
}

async function activateOnlineLane() {
  applyExecutionModeInSettings("local_primary");
  renderAIFallbackSettingsUi();
  renderOllamaModelManager();
  renderHero();
  showToast({
    type: "info",
    title: "Roadmap-only",
    message:
      "Online fallback is currently read-only and not active in production.",
    duration: 3600,
  });
}

async function handleSaveCredentialsClick() {
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
}

async function handleClearCredentialsClick() {
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
}

async function handleVerifyCredentialsClick() {
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
}

// ── Public entry point ──────────────────────────────────────────────────────

export function wireAiRefinement(): void {
  ensureAIFallbackSettingsDefaults();

  // Language controls — folded into this slice because all three listeners
  // mutate AI-Refinement prompt state via syncActivePromptPresetSelection()
  // and refreshResolvedRefinementPromptInSettings().

  dom.languageSelect?.addEventListener("change", async () => {
    if (!settings) return;
    settings.language_mode = dom.languageSelect!
      .value as Settings["language_mode"];
    settings.postproc_language = derivePostprocLanguageFromAsr(
      settings.language_mode,
      settings.language_pinned,
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
      settings.language_pinned,
    );
    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderSettings();
  });

  dom.whisperInputLanguageSelect?.addEventListener("change", () => {
    const value = dom.whisperInputLanguageSelect!
      .value as Settings["language_mode"];
    void setWhisperInputLanguage(value);
  });

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
    settings.postproc_capitalization_enabled =
      dom.postprocCapitalization!.checked;
    await persistSettings();
  });

  dom.postprocNumbers?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_numbers_enabled = dom.postprocNumbers!.checked;
    await persistSettings();
  });

  dom.postprocCustomVocabEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    settings.postproc_custom_vocab_enabled =
      dom.postprocCustomVocabEnabled!.checked;
    if (dom.postprocCustomVocabConfig) {
      dom.postprocCustomVocabConfig.style.display =
        settings.postproc_custom_vocab_enabled ? "flex" : "none";
    }
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
        const runtimeInfo = await invoke<any>("detect_ollama_runtime").catch(
          () => null,
        );
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
    if (!settings.ai_fallback.enabled) {
      // Stop Ollama runtime when AI refinement is disabled — only if Ollama is the active provider
      if (settings.ai_fallback.provider === "ollama") {
        try {
          await invoke("stop_ollama_runtime");
        } catch (error) {
          console.warn("Failed to stop Ollama runtime:", error);
        }
      }
    } else if (settings.ai_fallback.provider === "ollama") {
      void autoStartLocalRuntimeIfNeeded("enable_toggle").finally(() => {
        renderAIFallbackSettingsUi();
        renderOllamaModelManager();
      });
      renderAIFallbackSettingsUi();
      renderOllamaModelManager();
    }
  });

  dom.aiFallbackCloudProviderList?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const actionBtn = target?.closest<HTMLButtonElement>(
      "[data-ai-provider-action]",
    );
    if (!actionBtn) return;
    event.preventDefault();
    showToast({
      type: "info",
      title: "Roadmap-only",
      message:
        "Online fallback controls are visible for roadmap transparency and are currently read-only.",
      duration: 3200,
    });
  });

  dom.aiAuthModalClose?.addEventListener("click", () => {
    closeAuthModal();
  });
  dom.aiAuthModalBackdrop?.addEventListener("click", () => {
    closeAuthModal();
  });
  // Auth-modal Escape handler — added directly to window per OQ-3 clause 1
  // (no cleanup; listeners persist for app lifetime). Note: if wireAiRefinement()
  // is ever called more than once (e.g. in a future hot-reload path) this handler
  // will accumulate, but each instance is idempotent (checks dom.aiAuthModal.hidden).
  window.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key === "Escape" && dom.aiAuthModal && !dom.aiAuthModal.hidden) {
      closeAuthModal();
    }
  });

  dom.aiAuthMethod?.addEventListener("change", async () => {
    if (!settings || !dom.aiAuthMethod || !authModalProvider) return;
    ensureAIFallbackSettingsDefaults();
    const providerSettings = getAIFallbackProviderSettings(authModalProvider);
    if (!providerSettings) return;
    providerSettings.auth_method_preference = normalizeAuthMethodPreference(
      dom.aiAuthMethod.value,
    );
    await persistSettings();
    renderAIFallbackSettingsUi();
    refreshAuthModalContent();
    if (providerSettings.auth_method_preference === "oauth") {
      showToast({
        type: "info",
        title: "OAuth coming soon",
        message:
          "OAuth verification is not available yet. Use API key for now.",
        duration: 3600,
      });
    }
  });

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
    const action =
      dom.aiFallbackLocalPrimaryAction?.dataset.runtimeAction ?? "install";
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
    const switchPromise =
      source === "system"
        ? useSystemOllamaRuntime()
        : useManagedOllamaRuntime();
    renderAIFallbackSettingsUi();
    await switchPromise;
    refreshAIUi();
  });

  dom.aiFallbackLocalBackendSelect?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackLocalBackendSelect) return;
    const backend = dom.aiFallbackLocalBackendSelect.value as
      | "ollama"
      | "lm_studio"
      | "oobabooga";
    settings.ai_fallback.provider = backend;
    // Render immediately so the UI reflects the new backend before the slow save.
    renderAIFallbackSettingsUi();
    await persistSettings();
    // After save, re-render with the normalized settings from Rust.
    renderAIFallbackSettingsUi();
    // Trigger Ollama runtime refresh only when Ollama is selected.
    if (backend === "ollama") {
      void refreshOllamaRuntimeState({ force: false }).finally(() =>
        renderAIFallbackSettingsUi(),
      );
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
    if (dom.aiFallbackCompatStatus)
      dom.aiFallbackCompatStatus.textContent = "Connecting to server…";
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
      if (dom.aiFallbackCompatStatus)
        dom.aiFallbackCompatStatus.textContent = `${models.length} model(s) found.`;
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
    if (dom.aiFallbackCompatStatus)
      dom.aiFallbackCompatStatus.textContent = "Verifying…";
    try {
      const result = await invoke<{ ok: boolean; message: string }>(
        "test_provider_connection",
        {
          provider: settings.ai_fallback.provider,
          apiKey: s.api_key || "",
        },
      );
      if (dom.aiFallbackCompatStatus) {
        dom.aiFallbackCompatStatus.textContent = result.ok
          ? `✓ ${result.message}`
          : `✗ ${result.message}`;
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
        message:
          "A PowerShell window has opened. Follow the prompts to complete installation, then restart Trispr Flow.",
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
    const value = Math.max(
      0,
      Math.min(1, Number(dom.aiFallbackTemperature.value)),
    );
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
    settings.ai_fallback.preserve_source_language =
      dom.aiFallbackPreserveLanguage.checked;
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
    settings.ai_fallback.max_tokens = Math.max(
      128,
      Math.min(8192, Number(dom.aiFallbackMaxTokens.value)),
    );
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
      if (!confirmDiscardBuiltInEdits()) return;
      const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
      if (hasPendingUserChanges) await persistSettings();
      settings.ai_fallback.active_prompt_preset_id = presetId;
      syncActivePromptPresetSelection();
      refreshResolvedRefinementPromptInSettings();
      await persistSettings();
      renderAIFallbackSettingsUi();
    } else if (action === "new-preset") {
      if (!confirmDiscardBuiltInEdits()) return;
      const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
      if (hasPendingUserChanges) await persistSettings();
      settings.ai_fallback.active_prompt_preset_id =
        NEW_REFINEMENT_PROMPT_OPTION_ID;
      settings.ai_fallback.prompt_profile = "custom";
      settings.ai_fallback.custom_prompt_enabled = true;
      settings.ai_fallback.use_default_prompt = false;
      settings.ai_fallback.custom_prompt = "";
      if (dom.aiFallbackPromptPresetName)
        dom.aiFallbackPromptPresetName.value = "";
      if (dom.aiFallbackCustomPrompt) dom.aiFallbackCustomPrompt.value = "";
      refreshResolvedRefinementPromptInSettings();
      renderAIFallbackSettingsUi();
    } else if (action === "delete-chip-preset") {
      e.stopPropagation();
      const ai = settings.ai_fallback;
      ai.prompt_presets = normalizeUserRefinementPromptPresets(
        ai.prompt_presets,
      );
      const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
        ai.prompt_presets,
        presetId,
      );
      if (!selectedUserPreset) return;
      ai.prompt_presets = ai.prompt_presets.filter(
        (p) => p.id !== selectedUserPreset.id,
      );
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
    if (
      !settings ||
      !dom.aiFallbackCustomPrompt ||
      !dom.aiFallbackPromptPresetName
    )
      return;
    ensureAIFallbackSettingsDefaults();
    const prompt = dom.aiFallbackCustomPrompt.value.trim();
    if (!prompt) {
      showToast({
        type: "warning",
        title: "Prompt is empty",
        message: "Enter a prompt, or use Reset to restore the factory default.",
        duration: 3000,
      });
      return;
    }

    const ai = settings.ai_fallback;
    ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
    ai.prompt_preset_overrides ??= {};
    ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
      ai.active_prompt_preset_id,
      ai.prompt_profile,
      ai.prompt_presets,
    );

    const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
      ai.prompt_presets,
      ai.active_prompt_preset_id,
    );
    const activePresetSelection = ai.active_prompt_preset_id;
    const isBuiltIn =
      activePresetSelection !== "custom" &&
      activePresetSelection !== NEW_REFINEMENT_PROMPT_OPTION_ID &&
      !activePresetSelection.startsWith("user:");
    const requestedName = dom.aiFallbackPromptPresetName.value.trim();

    if (selectedUserPreset) {
      const nextName = requestedName || selectedUserPreset.name;
      const nextPrevious =
        prompt !== selectedUserPreset.prompt
          ? selectedUserPreset.prompt
          : selectedUserPreset.previous_prompt;
      ai.prompt_presets = ai.prompt_presets.map((preset) =>
        preset.id === selectedUserPreset.id
          ? { ...preset, name: nextName, prompt, previous_prompt: nextPrevious }
          : preset,
      );
      ai.active_prompt_preset_id = toUserRefinementPromptOptionId(
        selectedUserPreset.id,
      );
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
    } else if (isBuiltIn) {
      const builtInId = normalizeRefinementPromptPreset(
        activePresetSelection,
      ) as BuiltInRefinementPromptPreset;
      setPresetOverride(ai.prompt_preset_overrides, builtInId, prompt);
      const label =
        BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS.find(
          (p) => p.id === builtInId,
        )?.label ?? builtInId;
      showToast({
        type: "success",
        title: "Override saved",
        message: `Customized "${label}".`,
        duration: 2600,
      });
    } else {
      showToast({
        type: "info",
        title: "Nothing to save",
        message: "Select a preset or '+ New' to save changes.",
        duration: 3000,
      });
      return;
    }

    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackPromptPresetReset?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const ai = settings.ai_fallback;
    ai.prompt_preset_overrides ??= {};
    const activeId = normalizeActiveRefinementPromptPresetId(
      ai.active_prompt_preset_id,
      ai.prompt_profile,
      ai.prompt_presets,
    );
    const isBuiltIn =
      activeId !== "custom" &&
      activeId !== NEW_REFINEMENT_PROMPT_OPTION_ID &&
      !activeId.startsWith("user:");
    if (!isBuiltIn) return;
    const builtInId = normalizeRefinementPromptPreset(
      activeId,
    ) as BuiltInRefinementPromptPreset;
    const label =
      BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS.find((p) => p.id === builtInId)
        ?.label ?? builtInId;
    const hasOverride =
      typeof ai.prompt_preset_overrides[builtInId] === "string" &&
      ai.prompt_preset_overrides[builtInId]!.trim().length > 0;
    if (!hasOverride) return;
    if (
      !window.confirm(
        `Remove your customization for "${label}" and restore the factory default?`,
      )
    ) {
      return;
    }
    removePresetOverride(ai.prompt_preset_overrides, builtInId);
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
    showToast({
      type: "info",
      title: "Restored to default",
      message: `"${label}" reset to factory default.`,
      duration: 2600,
    });
  });

  dom.aiFallbackPromptPresetRevert?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
    const ai = settings.ai_fallback;
    ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
    ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
      ai.active_prompt_preset_id,
      ai.prompt_profile,
      ai.prompt_presets,
    );
    const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
      ai.prompt_presets,
      ai.active_prompt_preset_id,
    );
    if (!selectedUserPreset) return;
    const previous = selectedUserPreset.previous_prompt?.trim();
    if (!previous) return;
    if (
      !window.confirm(
        `Restore previous saved version of "${selectedUserPreset.name}"?`,
      )
    ) {
      return;
    }
    ai.prompt_presets = ai.prompt_presets.map((preset) =>
      preset.id === selectedUserPreset.id
        ? { ...preset, prompt: previous, previous_prompt: preset.prompt }
        : preset,
    );
    ai.active_prompt_preset_id = toUserRefinementPromptOptionId(
      selectedUserPreset.id,
    );
    syncActivePromptPresetSelection();
    refreshResolvedRefinementPromptInSettings();
    await persistSettings();
    renderAIFallbackSettingsUi();
    showToast({
      type: "info",
      title: "Reverted",
      message: `"${selectedUserPreset.name}" restored to previous version.`,
      duration: 2600,
    });
  });

  dom.aiFallbackPromptPresetDiscard?.addEventListener("click", () => {
    if (!settings) return;
    // Simply re-render to re-source the textarea from the effective prompt,
    // which discards any unsaved edits.
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
      ai.prompt_presets,
    );
    const selectedUserPreset = findUserRefinementPromptPresetByOptionId(
      ai.prompt_presets,
      ai.active_prompt_preset_id,
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

    ai.prompt_presets = ai.prompt_presets.filter(
      (preset) => preset.id !== selectedUserPreset.id,
    );
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
      settings.ai_fallback.prompt_presets,
    );
    const isUserPreset = activePresetId.startsWith("user:");
    const isBuiltIn =
      activePresetId !== "custom" &&
      activePresetId !== NEW_REFINEMENT_PROMPT_OPTION_ID &&
      !isUserPreset;
    if (isBuiltIn) {
      // Built-in edits are only persisted on Save. Just re-render for button-state updates.
      dom.aiFallbackCustomPrompt.classList.add("has-unsaved-edits");
      renderAIFallbackSettingsUi();
      return;
    }
    settings.ai_fallback.custom_prompt = dom.aiFallbackCustomPrompt.value;
    settings.ai_fallback.active_prompt_preset_id = activePresetId;
    settings.ai_fallback.prompt_profile = "custom";
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    refreshResolvedRefinementPromptInSettings();
    dom.aiFallbackCustomPrompt.classList.add("has-unsaved-edits");
    renderAIFallbackSettingsUi();
  });

  dom.aiFallbackCustomPrompt?.addEventListener("keydown", (e) => {
    if (!dom.aiFallbackCustomPrompt) return;
    const isSave = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s";
    if (isSave) {
      e.preventDefault();
      if (
        dom.aiFallbackPromptPresetSave &&
        !dom.aiFallbackPromptPresetSave.disabled
      ) {
        dom.aiFallbackPromptPresetSave.click();
      }
      return;
    }
    if (e.key === "Escape") {
      const discard = dom.aiFallbackPromptPresetDiscard;
      if (discard && !discard.hidden && !discard.disabled) {
        e.preventDefault();
        discard.click();
      }
    }
  });

  dom.aiFallbackCustomPrompt?.addEventListener("change", async () => {
    if (!settings || !dom.aiFallbackCustomPrompt) return;
    ensureAIFallbackSettingsDefaults();
    const activePresetId = normalizeActiveRefinementPromptPresetId(
      settings.ai_fallback.active_prompt_preset_id,
      settings.ai_fallback.prompt_profile,
      settings.ai_fallback.prompt_presets,
    );
    const isUserPreset = activePresetId.startsWith("user:");
    const isEditablePreset =
      activePresetId === "custom" ||
      activePresetId === NEW_REFINEMENT_PROMPT_OPTION_ID ||
      isUserPreset;
    if (!isEditablePreset) {
      renderAIFallbackSettingsUi();
      return;
    }
    settings.ai_fallback.custom_prompt =
      dom.aiFallbackCustomPrompt.value.trim();
    settings.ai_fallback.active_prompt_preset_id = activePresetId;
    settings.ai_fallback.prompt_profile = "custom";
    settings.ai_fallback.custom_prompt_enabled = true;
    settings.ai_fallback.use_default_prompt = false;
    refreshResolvedRefinementPromptInSettings();
    if (activePresetId === "custom") {
      await persistSettings();
    }
  });

  // Topic keywords reset
  dom.topicKeywordsReset?.addEventListener("click", async () => {
    if (!settings) return;
    settings.topic_keywords = cloneDefaultTopicKeywords();
    setTopicKeywords(settings.topic_keywords);
    await renderTopicKeywords();
    await persistSettings();
  });

  // ── Header quick-picker for refinement preset ─────────────────────────────
  dom.engineRefinePreset?.addEventListener("click", (e) => {
    e.stopPropagation();
    const menu = dom.engineRefinePresetMenu;
    if (!menu) return;
    const nowOpen = menu.hidden;
    menu.hidden = !nowOpen;
    dom.engineRefinePreset?.setAttribute("aria-expanded", nowOpen ? "true" : "false");
  });

  dom.engineRefinePresetMenu?.addEventListener("click", async (e) => {
    const btn = (e.target as HTMLElement).closest(
      "button[data-preset-id]",
    ) as HTMLButtonElement | null;
    if (!btn || !settings) return;
    const presetId = btn.dataset.presetId;
    if (!presetId) return;

    dom.engineRefinePresetMenu!.hidden = true;
    dom.engineRefinePreset?.setAttribute("aria-expanded", "false");

    if (presetId === "__off__") {
      settings.ai_fallback.enabled = false;
    } else {
      settings.ai_fallback.enabled = true;
      settings.ai_fallback.active_prompt_preset_id = presetId;
      syncActivePromptPresetSelection();
      refreshResolvedRefinementPromptInSettings();
    }
    await persistSettings();
    renderHero();
    renderAIFallbackSettingsUi();
  });

  document.addEventListener("click", () => {
    if (!dom.engineRefinePresetMenu?.hidden) {
      dom.engineRefinePresetMenu!.hidden = true;
      dom.engineRefinePreset?.setAttribute("aria-expanded", "false");
    }
  });
}
