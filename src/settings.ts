// Settings persistence and UI rendering
import { invoke } from "@tauri-apps/api/core";
import { overlayHealth, runtimeDiagnostics, settings, startupStatus } from "./state";
import * as dom from "./dom-refs";
import { thresholdToDb, VAD_DB_FLOOR } from "./ui-helpers";
import { applyAccentColor, DEFAULT_ACCENT_COLOR, normalizeColorHex } from "./utils";
import { renderVocabulary } from "./event-listeners";
import { DEFAULT_TOPICS, setTopicKeywords, type TopicKeywords } from "./history";
import { renderAIRefinementStaticHelp } from "./ai-refinement-help";
import {
  getOllamaRuntimeCardState,
  getOllamaRuntimeVersionCatalog,
  isOnlineVersionFetchInProgress,
} from "./ollama-models";
import { traceFrontendWarn } from "./frontend-trace";
import { syncRefinementPipelineGraphFromSettings } from "./refinement-pipeline-graph";
import {
  BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS,
  DEFAULT_REFINEMENT_PROMPT_PRESET,
  findUserRefinementPromptPresetByOptionId,
  NEW_REFINEMENT_PROMPT_OPTION_ID,
  normalizeActiveRefinementPromptPresetId,
  normalizePersistedRefinementPromptPresetId,
  normalizeRefinementPromptPreset,
  normalizeUserRefinementPromptPresets,
  resolveEffectiveRefinementPrompt,
  toUserRefinementPromptOptionId,
} from "./refinement-prompts";
import type {
  AIProviderSettings,
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthMethodPreference,
  OverlayRefiningIndicatorPreset,
  UserRefinementPromptPreset,
} from "./types";
import {
  CLOUD_PROVIDER_IDS,
  CLOUD_PROVIDER_LABELS,
  normalizeCloudProvider,
  normalizeExecutionMode,
  normalizeAuthMethodPreference,
  isVerifiedAuthStatus,
} from "./ai-provider-utils";

export function ensureContinuousDumpDefaults() {
  if (!settings) return;
  settings.auto_save_mic_audio ??= false;
  settings.continuous_dump_enabled ??= true;
  settings.continuous_dump_profile ??= "balanced";
  settings.continuous_soft_flush_ms ??= 10000;
  settings.continuous_silence_flush_ms ??= 1200;
  settings.continuous_hard_cut_ms ??= 45000;
  settings.continuous_min_chunk_ms ??= 1000;
  settings.continuous_pre_roll_ms ??= 300;
  settings.continuous_post_roll_ms ??= 200;
  settings.continuous_idle_keepalive_ms ??= 60000;
  settings.continuous_mic_override_enabled ??= false;
  settings.continuous_mic_soft_flush_ms ??= settings.continuous_soft_flush_ms;
  settings.continuous_mic_silence_flush_ms ??= settings.continuous_silence_flush_ms;
  settings.continuous_mic_hard_cut_ms ??= settings.continuous_hard_cut_ms;
  settings.continuous_system_override_enabled ??= false;
  settings.continuous_system_soft_flush_ms ??= settings.continuous_soft_flush_ms;
  settings.continuous_system_silence_flush_ms ??= settings.continuous_silence_flush_ms;
  settings.continuous_system_hard_cut_ms ??= settings.continuous_hard_cut_ms;
}

export async function persistSettings() {
  if (!settings) return;
  const aiFallback = settings.ai_fallback;
  const settingsForSave = {
    ...settings,
    ai_fallback: aiFallback ? { ...aiFallback } : aiFallback,
  };
  if (settingsForSave.ai_fallback) {
    settingsForSave.ai_fallback.prompt_presets = normalizeUserRefinementPromptPresets(
      settingsForSave.ai_fallback.prompt_presets
    );
    settingsForSave.ai_fallback.active_prompt_preset_id = normalizePersistedRefinementPromptPresetId(
      settingsForSave.ai_fallback.active_prompt_preset_id,
      settingsForSave.ai_fallback.prompt_profile,
      settingsForSave.ai_fallback.prompt_presets
    );
  }
  try {
    await Promise.race([
      invoke("save_settings", { settings: settingsForSave }),
      new Promise<void>((_, reject) =>
        setTimeout(() => reject(new Error("save_settings timed out")), 3_000)
      ),
    ]);
  } catch (error) {
    console.error("save_settings failed", error);
  }
}

function detectOverlayViewport(): { width: number; height: number } {
  const screenWidth = Number(
    (typeof window !== "undefined"
      ? window.screen?.availWidth ?? window.screen?.width
      : 0) ?? 0
  );
  const screenHeight = Number(
    (typeof window !== "undefined"
      ? window.screen?.availHeight ?? window.screen?.height
      : 0) ?? 0
  );
  const width = Number.isFinite(screenWidth) && screenWidth > 0 ? screenWidth : 1920;
  const height = Number.isFinite(screenHeight) && screenHeight > 0 ? screenHeight : 1080;
  return { width, height };
}

function applyOverlayDimensionSliderBounds() {
  const { width, height } = detectOverlayViewport();
  const kittMaxWidthCap = Math.max(50, Math.round(width * 0.5));
  const dotMaxRadiusCap = Math.max(8, Math.round(Math.min(width, height) * 0.25)); // 50% diameter

  if (dom.overlayKittMaxWidth) {
    dom.overlayKittMaxWidth.max = String(kittMaxWidthCap);
    dom.overlayKittMaxWidth.setAttribute("aria-valuemax", String(kittMaxWidthCap));
  }
  if (dom.overlayMaxRadius) {
    dom.overlayMaxRadius.max = String(dotMaxRadiusCap);
    dom.overlayMaxRadius.setAttribute("aria-valuemax", String(dotMaxRadiusCap));
  }
  if (dom.overlayMinRadius) {
    const minRadiusCap = Math.max(4, dotMaxRadiusCap);
    dom.overlayMinRadius.max = String(minRadiusCap);
    dom.overlayMinRadius.setAttribute("aria-valuemax", String(minRadiusCap));
  }
}

function clampToSliderBounds(input: HTMLInputElement, value: number): number {
  const parsedMin = Number(input.min);
  const parsedMax = Number(input.max);
  let out = value;
  if (Number.isFinite(parsedMin)) out = Math.max(parsedMin, out);
  if (Number.isFinite(parsedMax)) out = Math.min(parsedMax, out);
  return out;
}

export function updateOverlayStyleVisibility(style: string) {
  const isKitt = style === "kitt";
  if (dom.overlayDotSettings) dom.overlayDotSettings.style.display = isKitt ? "none" : "block";
  if (dom.overlayKittSettings) dom.overlayKittSettings.style.display = isKitt ? "block" : "none";
}

function getOverlaySharedSettings(style: string, current: typeof settings) {
  if (!current) return null;
  if (style === "kitt") {
    return {
      color: current.overlay_kitt_color,
      rise_ms: current.overlay_kitt_rise_ms,
      fall_ms: current.overlay_kitt_fall_ms,
      opacity_inactive: current.overlay_kitt_opacity_inactive,
      opacity_active: current.overlay_kitt_opacity_active,
    };
  }
  return {
    color: current.overlay_color,
    rise_ms: current.overlay_rise_ms,
    fall_ms: current.overlay_fall_ms,
    opacity_inactive: current.overlay_opacity_inactive,
    opacity_active: current.overlay_opacity_active,
  };
}

export function applyOverlaySharedUi(style: string) {
  if (!settings) return;
  const shared = getOverlaySharedSettings(style, settings);
  if (!shared) return;

  if (dom.overlayColor) dom.overlayColor.value = shared.color;
  let effectiveRise = shared.rise_ms;
  if (dom.overlayRise) dom.overlayRise.value = shared.rise_ms.toString();
  if (dom.overlayRise) {
    const maxRise = Number(dom.overlayRise.max || "200");
    if (Number.isFinite(maxRise) && maxRise > 0 && shared.rise_ms > maxRise) {
      dom.overlayRise.value = String(maxRise);
      effectiveRise = maxRise;
    }
  }
  if (dom.overlayRiseValue) dom.overlayRiseValue.textContent = `${effectiveRise}`;
  let effectiveFall = shared.fall_ms;
  if (dom.overlayFall) dom.overlayFall.value = shared.fall_ms.toString();
  if (dom.overlayFall) {
    const maxFall = Number(dom.overlayFall.max || "200");
    if (Number.isFinite(maxFall) && maxFall > 0 && shared.fall_ms > maxFall) {
      dom.overlayFall.value = String(maxFall);
      effectiveFall = maxFall;
    }
  }
  if (dom.overlayFallValue) dom.overlayFallValue.textContent = `${effectiveFall}`;
  if (dom.overlayOpacityInactive) {
    dom.overlayOpacityInactive.value = Math.round(shared.opacity_inactive * 100).toString();
  }
  if (dom.overlayOpacityInactiveValue) {
    dom.overlayOpacityInactiveValue.textContent = `${Math.round(shared.opacity_inactive * 100)}%`;
  }
  if (dom.overlayOpacityActive) {
    dom.overlayOpacityActive.value = Math.round(shared.opacity_active * 100).toString();
  }
  if (dom.overlayOpacityActiveValue) {
    dom.overlayOpacityActiveValue.textContent = `${Math.round(shared.opacity_active * 100)}%`;
  }
  if (dom.overlayPosX) {
    dom.overlayPosX.value = Math.round(
      style === "kitt" ? settings.overlay_kitt_pos_x : settings.overlay_pos_x
    ).toString();
  }
  if (dom.overlayPosY) {
    dom.overlayPosY.value = Math.round(
      style === "kitt" ? settings.overlay_kitt_pos_y : settings.overlay_pos_y
    ).toString();
  }
}

export function updateTranscribeVadVisibility(enabled: boolean) {
  if (dom.transcribeMeterThreshold) {
    dom.transcribeMeterThreshold.style.display = enabled ? "block" : "none";
  }
  if (dom.transcribeThresholdLabel) {
    dom.transcribeThresholdLabel.style.display = enabled ? "block" : "none";
  }
}

export function updateTranscribeThreshold(threshold: number) {
  const db = thresholdToDb(threshold, VAD_DB_FLOOR);
  if (dom.transcribeThresholdDb) {
    dom.transcribeThresholdDb.textContent = `${db.toFixed(1)} dB`;
  }
  if (dom.transcribeMeterThreshold) {
    const pos = (db - VAD_DB_FLOOR) / (0 - VAD_DB_FLOOR);
    dom.transcribeMeterThreshold.style.left = `${Math.round(pos * 100)}%`;
  }
}

function normalizeLanguageModeValue(languageMode: string | null | undefined): string {
  const normalized = (languageMode || "auto").trim().toLowerCase();
  if (!normalized) return "auto";
  return normalized;
}

export function resolveEffectiveAsrLanguageHint(
  languageMode: string | null | undefined,
  languagePinned: boolean | null | undefined
): string {
  const normalized = normalizeLanguageModeValue(languageMode);
  return languagePinned ? normalized : "auto";
}

export function derivePostprocLanguageFromAsr(
  languageMode: string | null | undefined,
  languagePinned: boolean | null | undefined
): "en" | "de" | "multi" {
  if (!languagePinned) return "multi";
  const normalized = normalizeLanguageModeValue(languageMode);
  if (normalized === "en") return "en";
  if (normalized === "de") return "de";
  return "multi";
}

function derivedPostprocLanguageLabel(postprocLanguage: "en" | "de" | "multi"): string {
  if (postprocLanguage === "en") {
    return "Derived: English rules (ASR language pinned to English).";
  }
  if (postprocLanguage === "de") {
    return "Derived: German rules (ASR language pinned to German).";
  }
  return "Derived: Multilingual rules (ASR auto-detect or non EN/DE language).";
}

export function syncCaptureModeVisibility(mode: string, pttUseVad = false): void {
  const hotkeysEnabled = mode === "ptt";
  const vadEnabled = mode === "vad" || (mode === "ptt" && pttUseVad);
  if (dom.hotkeysBlock) dom.hotkeysBlock.classList.toggle("hidden", !hotkeysEnabled);
  if (dom.vadBlock) dom.vadBlock.classList.toggle("hidden", !vadEnabled);
  // In PTT+VAD mode we only use threshold gating while the key is held.
  // Silence grace is VAD-mode specific and should not appear for PTT.
  const vadSilenceField = dom.vadSilence?.closest(".field");
  if (vadSilenceField) {
    vadSilenceField.classList.toggle("hidden", mode === "ptt");
  }
}

export function syncDerivedLanguageSettings(): void {
  if (!settings) return;
  settings.postproc_language = derivePostprocLanguageFromAsr(
    settings.language_mode,
    settings.language_pinned
  );
}

function syncAsrLanguageHintUi(): void {
  if (!settings) return;
  const pinned = Boolean(settings.language_pinned);
  if (dom.languageSelect) {
    dom.languageSelect.disabled = !pinned;
    dom.languageSelect.setAttribute("aria-disabled", String(!pinned));
  }
  if (dom.asrLanguageField) {
    dom.asrLanguageField.classList.toggle("is-disabled", !pinned);
  }
  if (dom.asrLanguageHintNote) {
    dom.asrLanguageHintNote.textContent = pinned
      ? "Pinned: ASR is locked to the selected language."
      : "Auto-detect is active. Enable pinning to lock a specific ASR language.";
  }
}

function normalizeRefiningIndicatorColor(value: string | undefined): string {
  return normalizeColorHex(value, "#6ec8ff");
}

function normalizeRefiningIndicatorSpeedMs(value: number | undefined): number {
  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) return 1150;
  return Math.max(450, Math.min(3000, Math.round(numberValue)));
}

function normalizeRefiningIndicatorRange(value: number | undefined): number {
  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) return 100;
  return Math.max(60, Math.min(180, Math.round(numberValue)));
}

function authStatusLabel(status?: string | null): string {
  if (status === "verified_api_key") return "Verified";
  if (status === "verified_oauth") return "Verified (OAuth)";
  return "Locked";
}

function authMethodLabel(method?: AIProviderAuthMethodPreference | null): string {
  return method === "oauth" ? "OAuth (coming soon)" : "API key";
}

function normalizeOverlayRefiningPreset(
  preset?: string | null
): OverlayRefiningIndicatorPreset {
  if (preset === "subtle" || preset === "intense") return preset;
  return "standard";
}

function getProviderSettings(provider: AIFallbackProvider): AIProviderSettings | null {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  // Ollama uses OllamaSettings, not AIProviderSettings
  return null;
}

function ensureSetupDefaults() {
  if (!settings) return;
  settings.setup ??= {
    local_ai_wizard_completed: false,
    local_ai_wizard_pending: true,
    ollama_remote_expert_opt_in: false,
  };
  settings.setup.ollama_remote_expert_opt_in ??= false;
}

function cloneTopicKeywords(input: TopicKeywords): TopicKeywords {
  const out: TopicKeywords = {};
  Object.entries(input).forEach(([topic, words]) => {
    out[topic] = [...words];
  });
  return out;
}

function normalizeTopicKeywords(
  input: Record<string, unknown> | null | undefined
): TopicKeywords {
  const fallback = cloneTopicKeywords(DEFAULT_TOPICS);
  if (!input || typeof input !== "object") return fallback;

  const normalized: TopicKeywords = {};
  Object.entries(input).forEach(([topic, words]) => {
    const key = topic.trim().toLowerCase();
    if (!key) return;
    if (!Array.isArray(words)) return;
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

function ensureTopicKeywordDefaults() {
  if (!settings) return;
  settings.topic_keywords = normalizeTopicKeywords(settings.topic_keywords);
  setTopicKeywords(settings.topic_keywords);
}

const AI_REFINEMENT_EXPANDER_STATE_KEY = "ai_refinement_expanders_v1";
const AI_REFINEMENT_EXPANDER_DEFAULTS: Record<string, boolean> = {
  "ai-refinement-runtime-expander": true,
  "ai-refinement-models-expander": true,
  "ai-refinement-topic-expander": true,
};

// In-memory cache: populated once from localStorage, then kept in sync via toggle listeners.
// null means "not yet loaded".
let _expanderStateCache: Record<string, boolean> | null = null;

function readAIRefinementExpanderState(): Record<string, boolean> {
  if (_expanderStateCache !== null) return _expanderStateCache;
  if (typeof window === "undefined") {
    _expanderStateCache = { ...AI_REFINEMENT_EXPANDER_DEFAULTS };
    return _expanderStateCache;
  }
  try {
    const raw = window.localStorage.getItem(AI_REFINEMENT_EXPANDER_STATE_KEY);
    if (!raw) {
      _expanderStateCache = { ...AI_REFINEMENT_EXPANDER_DEFAULTS };
      return _expanderStateCache;
    }
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const merged = { ...AI_REFINEMENT_EXPANDER_DEFAULTS };
    Object.keys(merged).forEach((key) => {
      if (typeof parsed?.[key] === "boolean") {
        merged[key] = parsed[key] as boolean;
      }
    });
    _expanderStateCache = merged;
    return _expanderStateCache;
  } catch {
    _expanderStateCache = { ...AI_REFINEMENT_EXPANDER_DEFAULTS };
    return _expanderStateCache;
  }
}

function writeAIRefinementExpanderState(next: Record<string, boolean>): void {
  _expanderStateCache = next;
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(AI_REFINEMENT_EXPANDER_STATE_KEY, JSON.stringify(next));
  } catch {
    // no-op
  }
}

function syncAIRefinementExpanders(): void {
  if (typeof document === "undefined") return;
  const state = readAIRefinementExpanderState();
  Object.keys(AI_REFINEMENT_EXPANDER_DEFAULTS).forEach((id) => {
    const expander = document.getElementById(id) as HTMLDetailsElement | null;
    if (!expander) return;
    expander.open = state[id] ?? AI_REFINEMENT_EXPANDER_DEFAULTS[id];
    if (expander.dataset.expanderBound === "true") return;
    expander.addEventListener("toggle", () => {
      const current = readAIRefinementExpanderState();
      current[id] = expander.open;
      writeAIRefinementExpanderState(current);
    });
    expander.dataset.expanderBound = "true";
  });
}

/**
 * Render model cards for OpenAI-compatible providers (LM Studio, Oobabooga).
 * Reuses the same .model-item CSS as the Ollama model manager.
 */
/**
 * Returns true if the model name matches known Chain-of-Thought reasoning patterns.
 * These models engage extended internal reasoning before output, causing 20-30s latency
 * for simple refinement tasks where a 3s instruct-model response is expected.
 */
function isReasoningModel(name: string): boolean {
  const n = name.toLowerCase();
  return (
    /deepseek-r\d/i.test(n) ||
    n.includes("qwq") ||
    n.includes("-think") ||
    n.includes("thinking") ||
    n.includes("-reason")
  );
}

function renderCompatModelCards(
  compatSettings: import("./types").OpenAICompatSettings | undefined,
  _provider?: string,
) {
  const container = dom.aiFallbackCompatModelList;
  if (!container) return;
  container.innerHTML = "";

  const models = compatSettings?.available_models ?? [];
  const preferred = compatSettings?.preferred_model || "";

  if (models.length === 0) {
    const hint = document.createElement("p");
    hint.className = "field-hint";
    hint.textContent = "Click \u2018Fetch models\u2019 to discover loaded models from the server.";
    container.appendChild(hint);
    return;
  }

  // Sort: active model first, then alphabetical
  const sorted = [...models].sort((a, b) => {
    if (a === preferred) return -1;
    if (b === preferred) return 1;
    return a.localeCompare(b);
  });

  sorted.forEach((modelName) => {
    const isActive = modelName === preferred;
    const card = document.createElement("article");
    card.className = `model-item${isActive ? " selected" : ""}`;

    const reasoningWarn = isReasoningModel(modelName)
      ? `<div class="model-reasoning-warn">⚠ Reasoning model — refinement may take 20-30s. Prefer an instruct model.</div>`
      : "";

    card.innerHTML = `
      <div class="model-header">
        <div class="model-name">${modelName}</div>
      </div>
      <div class="model-status ${isActive ? "active" : "downloaded"}">${isActive ? "Active" : "Available"}</div>
      ${reasoningWarn}
      <div class="model-actions"></div>
    `;

    const actionsEl = card.querySelector(".model-actions") as HTMLDivElement | null;
    if (actionsEl && !isActive) {
      const activateBtn = document.createElement("button");
      activateBtn.className = "btn-sm btn-primary";
      activateBtn.textContent = "Activate";
      activateBtn.title = `Use ${modelName} for AI refinement`;
      activateBtn.addEventListener("click", () => {
        if (!settings || !compatSettings) return;
        compatSettings.preferred_model = modelName;
        settings.ai_fallback.model = modelName;
        void persistSettings().then(() => renderAIFallbackSettingsUi());
      });
      actionsEl.appendChild(activateBtn);
    }

    container.appendChild(card);
  });
}

function applyProviderLaneVisibility(isOnlineMode: boolean) {
  if (dom.aiFallbackModelField)
    dom.aiFallbackModelField.style.display = isOnlineMode ? "block" : "none";
  if (dom.aiFallbackOllamaManagedNote)
    dom.aiFallbackOllamaManagedNote.style.display = isOnlineMode ? "none" : "block";
  if (dom.aiFallbackProviderLanes) {
    dom.aiFallbackProviderLanes.style.display = "grid";
  }
}

function renderCloudProviderList(fallbackProvider: CloudAIFallbackProvider | null) {
  if (!dom.aiFallbackCloudProviderList) return;

  dom.aiFallbackCloudProviderList.innerHTML = "";

  CLOUD_PROVIDER_IDS.forEach((providerId) => {
    const providerConfig = getProviderSettings(providerId);
    const verified = isVerifiedAuthStatus(providerConfig?.auth_status);
    const selectedFallback = fallbackProvider === providerId;
    const row = document.createElement("div");
    row.className = `cloud-provider-row is-disabled${selectedFallback ? " is-selected" : ""}`;
    row.setAttribute("aria-disabled", "true");

    const left = document.createElement("div");
    left.className = "cloud-provider-main";

    const label = document.createElement("div");
    label.className = "cloud-provider-title";
    label.textContent = CLOUD_PROVIDER_LABELS[providerId];
    left.appendChild(label);

    const meta = document.createElement("div");
    meta.className = "cloud-provider-meta";
    const authStatus = authStatusLabel(providerConfig?.auth_status);
    const method = authMethodLabel(providerConfig?.auth_method_preference);
    meta.textContent = `${authStatus} • ${method}`;
    left.appendChild(meta);

    const actions = document.createElement("div");
    actions.className = "cloud-provider-actions";

    const selectBtn = document.createElement("button");
    selectBtn.className = "hotkey-record-btn";
    selectBtn.type = "button";
    selectBtn.dataset.aiProviderAction = "select-fallback";
    selectBtn.dataset.provider = providerId;
    if (selectedFallback) {
      selectBtn.textContent = verified ? "Saved fallback" : "Saved (locked)";
    } else {
      selectBtn.textContent = "Roadmap";
    }
    selectBtn.disabled = true;
    actions.appendChild(selectBtn);

    const authBtn = document.createElement("button");
    authBtn.className = "hotkey-record-btn";
    authBtn.type = "button";
    authBtn.dataset.aiProviderAction = "authenticate";
    authBtn.dataset.provider = providerId;
    authBtn.textContent = "Read-only";
    authBtn.disabled = true;
    actions.appendChild(authBtn);

    row.appendChild(left);
    row.appendChild(actions);
    dom.aiFallbackCloudProviderList?.appendChild(row);
  });
}

function renderAIFallbackModelOptions(provider: AIFallbackProvider, selectedModel: string) {
  if (!dom.aiFallbackModel) return;

  if (provider === "ollama" || provider === "lm_studio" || provider === "oobabooga") {
    // Local backends manage their model in the Runtime section — hide this picker
    dom.aiFallbackModel.disabled = true;
    dom.aiFallbackModel.innerHTML = "";
    const option = document.createElement("option");
    option.value = selectedModel || "";
    option.textContent = selectedModel || "Managed in Local AI Runtime section";
    dom.aiFallbackModel.appendChild(option);
    dom.aiFallbackModel.value = option.value;
    return;
  }

  dom.aiFallbackModel.disabled = false;
  const models = getProviderSettings(provider)?.available_models ?? [];

  dom.aiFallbackModel.innerHTML = "";
  for (const modelId of models) {
    const option = document.createElement("option");
    option.value = modelId;
    option.textContent = modelId;
    dom.aiFallbackModel.appendChild(option);
  }
  if (models.length > 0) {
    dom.aiFallbackModel.value = models.includes(selectedModel) ? selectedModel : models[0];
  } else {
    const placeholder = "No models available";
    const option = document.createElement("option");
    option.value = selectedModel || "";
    option.textContent = selectedModel || placeholder;
    dom.aiFallbackModel.appendChild(option);
    dom.aiFallbackModel.value = option.value;
  }
}

function renderRefinementPipelineNote() {
  if (!settings || !dom.refinementPipelineNote) return;
  const aiEnabled = Boolean(settings.ai_fallback?.enabled);
  const rulesEnabled = Boolean(settings.postproc_enabled);
  const provider = settings.ai_fallback?.provider ?? "ollama";
  const isCompatLocal = provider === "lm_studio" || provider === "oobabooga";
  const ollamaReady = Boolean(startupStatus?.ollama_ready);
  const ollamaStarting = Boolean(startupStatus?.ollama_starting);
  // LM Studio / Oobabooga are external servers — treat as "ready" when AI is enabled
  const aiReady = isCompatLocal || ollamaReady;
  const aiStarting = !isCompatLocal && ollamaStarting;

  const providerLabels: Record<string, string> = {
    ollama: "Ollama", lm_studio: "LM Studio", oobabooga: "Oobabooga",
  };
  const label = providerLabels[provider] ?? provider;

  let note = "No refinement active: raw transcription output is used.";
  if (aiEnabled && rulesEnabled) {
    note = aiReady
      ? `Primary output: ${label} AI refinement. Rule-based refiner remains active as non-AI fallback (no token/API cost).`
      : aiStarting
        ? "Primary output: Rule-based refiner while local AI starts in background."
        : "Primary output: Rule-based refiner while local AI is unavailable.";
  } else if (aiEnabled) {
    note = aiReady
      ? `Primary output: ${label} AI refinement only. Rule-based non-AI fallback is disabled.`
      : "Primary output: Raw transcription while local AI is unavailable.";
  } else if (rulesEnabled) {
    note = "Primary output: Rule-based refiner only (non-AI, zero token/API cost).";
  }

  dom.refinementPipelineNote.textContent = note;
  dom.refinementPipelineNote.classList.toggle("is-warning", !rulesEnabled);
}

function renderOverlayHealthNote() {
  if (!dom.overlayHealthNote) return;
  const health = overlayHealth;
  if (!health) {
    dom.overlayHealthNote.hidden = true;
    dom.overlayHealthNote.textContent = "";
    return;
  }
  dom.overlayHealthNote.hidden = false;
  dom.overlayHealthNote.textContent =
    health.status === "failed"
      ? `Overlay degraded after ${health.attempt} recovery attempts: ${health.reason}`
      : `Overlay recovering (${health.attempt}): ${health.reason}`;
}

function renderPromptPresetCards(
  userPresets: UserRefinementPromptPreset[],
  activePresetId: string
): void {
  const container = dom.promptPresetList;
  if (!container) return;
  container.innerHTML = "";

  // Normalise "custom" fallback — no card for it, treat as default built-in
  const effectiveActiveId =
    activePresetId === "custom" ? DEFAULT_REFINEMENT_PROMPT_PRESET : activePresetId;

  const makeChip = (
    presetId: string,
    label: string,
    isActive: boolean,
    extraClass: string,
    action: string,
    deletable: boolean
  ): HTMLElement => {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = `preset-chip${isActive ? " preset-chip--active" : ""}${extraClass ? " " + extraClass : ""}`;
    chip.dataset.presetId = presetId;
    chip.dataset.action = action;
    chip.textContent = label;
    if (deletable) {
      const del = document.createElement("span");
      del.className = "preset-chip-del";
      del.dataset.action = "delete-chip-preset";
      del.title = "Delete preset";
      del.textContent = "×";
      chip.appendChild(del);
    }
    return chip;
  };

  // Built-in chips
  for (const preset of BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS) {
    const isActive = effectiveActiveId === preset.id;
    container.appendChild(makeChip(preset.id, preset.label, isActive, "", "use-preset", false));
  }

  // User preset chips
  for (const preset of userPresets) {
    const optionId = toUserRefinementPromptOptionId(preset.id);
    const isActive = effectiveActiveId === optionId;
    container.appendChild(
      makeChip(optionId, preset.name, isActive, "preset-chip--user", "use-preset", true)
    );
  }

  // "+ New" chip
  const isNewMode = effectiveActiveId === NEW_REFINEMENT_PROMPT_OPTION_ID;
  container.appendChild(
    makeChip(NEW_REFINEMENT_PROMPT_OPTION_ID, "+ New", isNewMode, "preset-chip--new", "new-preset", false)
  );
}

let aiRuntimeStateDriftLogged = false;

export function renderAIFallbackSettingsUi() {
  if (!settings) return;
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
  CLOUD_PROVIDER_IDS.forEach((providerId) => {
    const providerSettings = getProviderSettings(providerId);
    if (!providerSettings) return;
    providerSettings.auth_method_preference = normalizeAuthMethodPreference(
      providerSettings.auth_method_preference
    );
  });
  const ai = settings.ai_fallback;
  settings.providers.ollama.runtime_target_version ??= "0.17.7";
  ai.prompt_profile = normalizeRefinementPromptPreset(ai.prompt_profile);
  ai.prompt_presets = normalizeUserRefinementPromptPresets(ai.prompt_presets);
  ai.active_prompt_preset_id = normalizeActiveRefinementPromptPresetId(
    ai.active_prompt_preset_id,
    ai.prompt_profile,
    ai.prompt_presets
  );
  const selectedUserPromptPresetFromActive = findUserRefinementPromptPresetByOptionId(
    ai.prompt_presets,
    ai.active_prompt_preset_id
  );
  const isNewPromptPresetModeFromActive = ai.active_prompt_preset_id === NEW_REFINEMENT_PROMPT_OPTION_ID;
  if (selectedUserPromptPresetFromActive) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
    ai.custom_prompt = selectedUserPromptPresetFromActive.prompt;
  } else if (isNewPromptPresetModeFromActive) {
    ai.prompt_profile = "custom";
    ai.custom_prompt_enabled = true;
  } else {
    ai.prompt_profile = normalizeRefinementPromptPreset(ai.active_prompt_preset_id);
    ai.custom_prompt_enabled = ai.prompt_profile === "custom";
  }
  ai.use_default_prompt = false;
  ai.preserve_source_language ??= true;
  ai.fallback_provider = normalizeCloudProvider(ai?.fallback_provider ?? null);
  ai.execution_mode = normalizeExecutionMode(ai?.execution_mode);
  const LOCAL_BACKENDS: AIFallbackProvider[] = ["ollama", "lm_studio", "oobabooga"];
  if (!LOCAL_BACKENDS.includes(ai.provider as AIFallbackProvider)) {
    // Cloud provider somehow set as primary — migrate fallback and reset to ollama
    if (!ai.fallback_provider) {
      ai.fallback_provider = normalizeCloudProvider(ai.provider);
    }
    ai.provider = "ollama";
  }

  const fallbackProvider = normalizeCloudProvider(ai?.fallback_provider ?? null);
  const fallbackConfig = fallbackProvider ? getProviderSettings(fallbackProvider) : null;
  const executionMode: AIExecutionMode = "local_primary";
  const provider = ai.provider as AIFallbackProvider;
  ai.execution_mode = executionMode;
  settings.postproc_llm_provider = provider === "ollama" ? "ollama" : provider;

  const runtimeCardState = getOllamaRuntimeCardState();
  const runtimeVersionOptions = getOllamaRuntimeVersionCatalog();
  const runtimeStage = runtimeDiagnostics?.ollama?.spawn_stage?.trim() || "";
  const runtimeHealthy =
    runtimeCardState.healthy
    || Boolean(runtimeDiagnostics?.ollama?.reachable)
    || Boolean(startupStatus?.ollama_ready)
    || runtimeStage === "ready";
  const runtimeStarting =
    !runtimeHealthy
    && (
      runtimeCardState.busy
      || runtimeCardState.backgroundStarting
      || Boolean(startupStatus?.ollama_starting)
    );
  const aiRuntimeBannerVisible = Boolean(ai?.enabled) && provider === "ollama" && runtimeStarting;
  if (runtimeHealthy && startupStatus?.ollama_starting) {
    if (!aiRuntimeStateDriftLogged) {
      aiRuntimeStateDriftLogged = true;
      traceFrontendWarn("ai.runtime_ui", "runtime healthy while startup still reports starting", {
        startupStatus,
        runtimeDiagnostics: runtimeDiagnostics?.ollama ?? null,
      });
    }
  } else {
    aiRuntimeStateDriftLogged = false;
  }
  let selectedRuntimeEntry: (typeof runtimeVersionOptions)[number] | null = null;

  if (dom.aiFallbackEnabled) {
    dom.aiFallbackEnabled.checked = Boolean(ai?.enabled);
  }
  renderRefinementPipelineNote();
  syncRefinementPipelineGraphFromSettings();
  if (dom.aiFallbackSettings) {
    dom.aiFallbackSettings.style.display = "block";
    dom.aiFallbackSettings.classList.toggle("is-disabled", !ai?.enabled);
    dom.aiFallbackSettings.setAttribute("aria-busy", runtimeCardState.busy ? "true" : "false");
  }
  if (dom.aiFallbackLoadingScrim) {
    dom.aiFallbackLoadingScrim.hidden = !aiRuntimeBannerVisible;
  }
  if (dom.aiFallbackLoadingTitle) {
    dom.aiFallbackLoadingTitle.textContent = "Local AI runtime is starting";
  }
  if (dom.aiFallbackLoadingDetail) {
    const detail = runtimeCardState.detail?.trim();
    dom.aiFallbackLoadingDetail.textContent =
      detail && detail.length > 0
        ? detail
        : "Preparing Ollama in the background.";
  }
  renderCloudProviderList(fallbackProvider);

  if (dom.aiFallbackFallbackStatus) {
    const providerStatus = fallbackProvider
      ? `${CLOUD_PROVIDER_LABELS[fallbackProvider]} stored (${authStatusLabel(fallbackConfig?.auth_status)})`
      : "No provider selected.";
    dom.aiFallbackFallbackStatus.textContent =
      `${providerStatus} Online fallback is roadmap-only and not active in production.`;
  }

  if (dom.aiFallbackLocalLane) {
    dom.aiFallbackLocalLane.classList.toggle("is-active", true);
    dom.aiFallbackLocalLane.classList.toggle("is-runtime-busy", runtimeCardState.busy);
    dom.aiFallbackLocalLane.setAttribute("aria-pressed", "true");
  }
  if (dom.aiFallbackOnlineLane) {
    dom.aiFallbackOnlineLane.classList.remove("is-active");
    dom.aiFallbackOnlineLane.classList.add("is-roadmap-disabled");
    dom.aiFallbackOnlineLane.setAttribute("aria-pressed", "false");
    dom.aiFallbackOnlineLane.setAttribute("aria-disabled", "true");
  }

  if (dom.aiFallbackOnlineStatusBadge) {
    dom.aiFallbackOnlineStatusBadge.textContent = "Roadmap • Not active";
    dom.aiFallbackOnlineStatusBadge.classList.add("is-locked");
    dom.aiFallbackOnlineStatusBadge.classList.remove("is-verified");
    dom.aiFallbackOnlineStatusBadge.classList.remove("is-active");
  }
  if (dom.aiFallbackLocalPrimaryStatus) {
    const healthText = runtimeCardState.healthy
      ? "running"
      : runtimeCardState.detected
        ? "detected, not running"
        : "not detected";
    const processText = runtimeCardState.managedAlive
      ? `managed pid ${runtimeCardState.managedPid ?? "?"}`
      : runtimeCardState.healthy
        ? "running (external or unmanaged)"
        : "no managed process";
    dom.aiFallbackLocalPrimaryStatus.textContent = `Runtime ${healthText} • Source: ${runtimeCardState.source} • Version: ${runtimeCardState.version} • Process: ${processText}`;
  }
  if (dom.aiFallbackLocalRuntimeNote) {
    let baseNote = runtimeCardState.busy
      ? `${runtimeCardState.detail} Running in background.`
      : runtimeStarting
        ? "Starting in background. Controls remain available."
        : runtimeCardState.detail;
    if (!runtimeCardState.busy && !runtimeCardState.healthy) {
      if (runtimeStarting) {
        baseNote = "Starting in background. Controls remain available.";
      } else if (settings.postproc_enabled) {
        baseNote = "Unavailable, fallback active.";
      } else {
        baseNote = "Available later.";
      }
    }
    dom.aiFallbackLocalRuntimeNote.textContent = runtimeCardState.compatibilityWarning
      ? `${baseNote} ${runtimeCardState.compatibilityWarning}`
      : baseNote;
    dom.aiFallbackLocalRuntimeNote.classList.toggle(
      "ai-runtime-busy-note",
      runtimeCardState.busy || runtimeCardState.backgroundStarting
    );
    dom.aiFallbackLocalRuntimeNote.setAttribute("aria-live", "polite");
  }

  // Show/update Ollama runtime installation progress bar
  if (dom.aiFallbackRuntimeProgress && dom.aiFallbackRuntimeProgressFill && dom.aiFallbackRuntimeProgressText) {
    const progress = (window as any).runtimeInstallProgress;
    if (progress) {
      // Show progress bar
      dom.aiFallbackRuntimeProgress.removeAttribute("hidden");

      // Update progress fill
      let percent = 0;
      if (progress.downloaded !== undefined && progress.total !== undefined && progress.total > 0) {
        percent = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
      }
      dom.aiFallbackRuntimeProgressFill.style.width = `${percent}%`;

      // Update progress text
      let progressText = progress.message || "";
      if (progress.downloaded !== undefined && progress.total !== undefined && progress.total > 0) {
        const mbDone = Math.round(progress.downloaded / (1024 * 1024));
        const mbTotal = Math.round(progress.total / (1024 * 1024));
        progressText = `${progressText} (${mbDone}/${mbTotal} MB)`;
      }
      dom.aiFallbackRuntimeProgressText.textContent = progressText;
    } else {
      // Hide progress bar
      dom.aiFallbackRuntimeProgress.setAttribute("hidden", "");
    }
  }

  if (dom.aiFallbackLocalPrimaryAction) {
    dom.aiFallbackLocalPrimaryAction.textContent = runtimeCardState.primaryLabel;
    dom.aiFallbackLocalPrimaryAction.disabled = runtimeCardState.primaryDisabled;
    dom.aiFallbackLocalPrimaryAction.dataset.runtimeAction = runtimeCardState.primaryAction;
    dom.aiFallbackLocalPrimaryAction.classList.toggle("is-busy", runtimeCardState.busy);
    dom.aiFallbackLocalPrimaryAction.setAttribute(
      "aria-busy",
      runtimeCardState.busy ? "true" : "false"
    );
  }
  if (dom.aiFallbackLocalImportAction) {
    dom.aiFallbackLocalImportAction.disabled = runtimeCardState.busy;
  }
  if (dom.aiFallbackLocalRuntimeSource) {
    const currentSource = settings.providers.ollama.runtime_source || "per_user_zip";
    dom.aiFallbackLocalRuntimeSource.value = currentSource;
    dom.aiFallbackLocalRuntimeSource.disabled = runtimeCardState.busy;
  }
  if (dom.aiFallbackLocalVerifyAction) {
    dom.aiFallbackLocalVerifyAction.disabled = runtimeCardState.busy || !runtimeCardState.detected;
  }
  if (dom.aiFallbackLocalRefreshAction) {
    dom.aiFallbackLocalRefreshAction.disabled = runtimeCardState.busy;
  }
  if (dom.aiFallbackFetchVersionsAction) {
    dom.aiFallbackFetchVersionsAction.disabled = runtimeCardState.busy || isOnlineVersionFetchInProgress();
    dom.aiFallbackFetchVersionsAction.textContent = isOnlineVersionFetchInProgress()
      ? "Fetching..."
      : "Get versions";
  }
  if (dom.aiFallbackLocalRuntimeVersion) {
    const selectedVersion =
      settings.providers.ollama.runtime_target_version?.trim() || runtimeCardState.version || "0.17.7";
    const optionPool = [...runtimeVersionOptions];
    const appendIfMissing = (version: string) => {
      if (!version) return;
      if (optionPool.some((entry) => entry.version === version)) return;
      optionPool.push({
        version,
        source: "online",
        selected: version === selectedVersion,
        installed: version === runtimeCardState.version,
        recommended: version === "0.17.7",
        installable: false,
        installable_reason:
          "This version is not in the verified installable runtime catalog.",
      });
    };
    appendIfMissing(selectedVersion);
    appendIfMissing(runtimeCardState.version);
    const prioritized = optionPool
      .sort((a, b) => {
        const aScore = (a.selected ? 4 : 0) + (a.installed ? 2 : 0) + (a.recommended ? 1 : 0);
        const bScore = (b.selected ? 4 : 0) + (b.installed ? 2 : 0) + (b.recommended ? 1 : 0);
        return bScore - aScore || b.version.localeCompare(a.version, undefined, { numeric: true });
      })
      .filter((entry, idx, arr) => idx === arr.findIndex((e) => e.version === entry.version));
    const limited = prioritized;

    dom.aiFallbackLocalRuntimeVersion.innerHTML = "";
    limited.forEach((entry) => {
      const option = document.createElement("option");
      option.value = entry.version;
      option.textContent = entry.installable
        ? entry.version
        : `${entry.version} (not installable)`;
      option.disabled = !entry.installable && !entry.selected;
      dom.aiFallbackLocalRuntimeVersion?.appendChild(option);
    });
    dom.aiFallbackLocalRuntimeVersion.size = 1;
    dom.aiFallbackLocalRuntimeVersion.classList.remove("is-scroll-list");
    if (
      selectedVersion &&
      Array.from(dom.aiFallbackLocalRuntimeVersion.options).some((opt) => opt.value === selectedVersion)
    ) {
      dom.aiFallbackLocalRuntimeVersion.value = selectedVersion;
    }
    selectedRuntimeEntry =
      limited.find((entry) => entry.version === dom.aiFallbackLocalRuntimeVersion?.value) ?? null;
    dom.aiFallbackLocalRuntimeVersion.disabled = runtimeCardState.busy;
  }
  if (dom.aiFallbackLocalRuntimeVersionNote) {
    const selected = settings.providers.ollama.runtime_target_version || "0.17.7";
    dom.aiFallbackLocalRuntimeVersionNote.textContent = "";
    const lead = document.createElement("span");
    lead.textContent = `Selected target ${selected}. `;
    dom.aiFallbackLocalRuntimeVersionNote.appendChild(lead);

    const badges = document.createElement("span");
    badges.className = "runtime-version-badges";
    const addBadge = (label: string, cls: string) => {
      const el = document.createElement("span");
      el.className = `ollama-runtime-badge runtime-version-chip ${cls}`;
      el.textContent = label;
      badges.appendChild(el);
    };

    if (selectedRuntimeEntry?.selected) addBadge("Active", "runtime-version-chip--selected");
    if (selectedRuntimeEntry?.installed) addBadge("Installed", "runtime-version-chip--installed");
    if (selectedRuntimeEntry?.recommended) addBadge("Recommended", "runtime-version-chip--recommended");
    if (selectedRuntimeEntry) {
      if (selectedRuntimeEntry.installable) {
        addBadge("Installable", "runtime-version-chip--installable");
      } else {
        addBadge("Not installable", "runtime-version-chip--not-installable");
      }
      addBadge(
        selectedRuntimeEntry.source === "online" ? "Online" : "Pinned",
        "runtime-version-chip--source"
      );
    } else {
      addBadge("Not installable", "runtime-version-chip--not-installable");
      addBadge("Pinned", "runtime-version-chip--source");
    }

    dom.aiFallbackLocalRuntimeVersionNote.appendChild(badges);
    if (selectedRuntimeEntry?.installable_reason?.trim()) {
      const reason = document.createElement("span");
      reason.className = "runtime-version-reason";
      reason.textContent = selectedRuntimeEntry.installable_reason.trim();
      dom.aiFallbackLocalRuntimeVersionNote.appendChild(reason);
    }
  }

  if (dom.aiFallbackLocalBackendSelect) {
    const currentBackend = ai?.provider ?? "ollama";
    const validBackends = ["ollama", "lm_studio", "oobabooga"];
    dom.aiFallbackLocalBackendSelect.value = validBackends.includes(currentBackend) ? currentBackend : "ollama";
    dom.aiFallbackLocalBackendSelect.disabled = runtimeCardState.busy;
  }
  const backendTitleEl = document.getElementById("ai-fallback-local-lane-title-text");
  if (backendTitleEl) {
    const labels: Record<string, string> = {
      ollama: "Ollama (Local)",
      lm_studio: "LM Studio (Local)",
      oobabooga: "Oobabooga (Local)",
    };
    backendTitleEl.textContent = labels[ai?.provider ?? "ollama"] ?? "Local AI (Local)";
  }

  const isOllama = provider === "ollama";
  const isCompatBackend = provider === "lm_studio" || provider === "oobabooga";

  // Show/hide Ollama-specific controls vs OpenAI-compat config
  if (dom.aiFallbackLocalAdvanced) {
    dom.aiFallbackLocalAdvanced.hidden = !isOllama;
  }
  if (dom.aiFallbackCompatConfig) {
    dom.aiFallbackCompatConfig.hidden = !isCompatBackend;
  }

  // Update the "managed note" to reflect the active backend
  if (dom.aiFallbackOllamaManagedNote) {
    if (isOllama) {
      dom.aiFallbackOllamaManagedNote.textContent =
        "Ollama model selection is managed in the model cards below.";
    } else if (provider === "lm_studio") {
      dom.aiFallbackOllamaManagedNote.textContent =
        "LM Studio must be running with its local server enabled. Start LM Studio → load a model → enable Local Server → then click \u2018Fetch models\u2019 above.";
    } else if (provider === "oobabooga") {
      dom.aiFallbackOllamaManagedNote.textContent =
        "Oobabooga (text-generation-webui) must be running with the API extension enabled. Start the server, load a model, then click \u2018Fetch models\u2019 above.";
    }
  }

  // Primary action button: Ollama only (install/start), hide for compat backends
  if (dom.aiFallbackLocalPrimaryAction) {
    dom.aiFallbackLocalPrimaryAction.hidden = !isOllama;
    if (isOllama) {
      dom.aiFallbackLocalPrimaryAction.title = "Install or start local Ollama runtime";
    }
  }
  // Import model button: Ollama only
  if (dom.aiFallbackLocalImportAction) {
    dom.aiFallbackLocalImportAction.hidden = !isOllama;
    if (isOllama) {
      dom.aiFallbackLocalImportAction.title = "Import a local GGUF or Modelfile into Ollama";
    }
  }
  // Runtime status line: Ollama only
  if (dom.aiFallbackLocalPrimaryStatus) {
    dom.aiFallbackLocalPrimaryStatus.hidden = !isOllama;
  }
  // LM Studio install button: only for LM Studio backend
  if (dom.aiFallbackLmStudioInstallAction) {
    dom.aiFallbackLmStudioInstallAction.hidden = provider !== "lm_studio";
  }

  if (isOllama) {
    if (dom.aiFallbackLocalFallbackEndpoints && document.activeElement !== dom.aiFallbackLocalFallbackEndpoints) {
      dom.aiFallbackLocalFallbackEndpoints.value = (settings.providers.ollama.fallback_endpoints ?? []).join("\n");
      dom.aiFallbackLocalFallbackEndpoints.disabled = runtimeCardState.busy;
    }
  }

  if (isCompatBackend) {
    const compatSettings = provider === "lm_studio"
      ? settings.providers.lm_studio
      : settings.providers.oobabooga;
    const defaultEndpoint = provider === "lm_studio" ? "http://127.0.0.1:1234" : "http://127.0.0.1:5000";
    const endpointHint = provider === "lm_studio"
      ? "Default LM Studio port: 127.0.0.1:1234"
      : "Default Oobabooga port: 127.0.0.1:5000";

    if (dom.aiFallbackCompatGuide) {
      dom.aiFallbackCompatGuide.textContent = provider === "lm_studio"
        ? "Setup: Install LM Studio \u2192 load a model \u2192 open the \u201cLocal Server\u201d tab \u2192 click Start. Then click \u2018Fetch models\u2019."
        : "Setup: Start text-generation-webui with --api flag \u2192 load a model. Then click \u2018Fetch models\u2019.";
    }

    if (dom.aiFallbackCompatEndpoint && document.activeElement !== dom.aiFallbackCompatEndpoint) {
      dom.aiFallbackCompatEndpoint.value = compatSettings?.endpoint || defaultEndpoint;
      dom.aiFallbackCompatEndpoint.placeholder = defaultEndpoint;
    }
    if (dom.aiFallbackCompatEndpointHint) {
      dom.aiFallbackCompatEndpointHint.textContent = endpointHint;
    }
    if (dom.aiFallbackCompatApiKey && document.activeElement !== dom.aiFallbackCompatApiKey) {
      dom.aiFallbackCompatApiKey.value = compatSettings?.api_key || "";
    }

    // Model select
    // Render model cards (same visual pattern as Ollama model manager)
    renderCompatModelCards(compatSettings, provider);
  }

  applyProviderLaneVisibility(false);
  renderAIFallbackModelOptions(provider, ai?.model || "");

  if (dom.aiFallbackTemperature) {
    const temp = Math.max(0, Math.min(1, Number(ai?.temperature ?? 0.3)));
    dom.aiFallbackTemperature.value = temp.toFixed(2);
  }
  if (dom.aiFallbackTemperatureValue) {
    const temp = Math.max(0, Math.min(1, Number(ai?.temperature ?? 0.3)));
    dom.aiFallbackTemperatureValue.textContent = temp.toFixed(2);
  }
  if (dom.aiFallbackPreserveLanguage) {
    dom.aiFallbackPreserveLanguage.checked = Boolean(ai?.preserve_source_language ?? true);
  }
  if (dom.aiFallbackPreserveLanguageNote) {
    dom.aiFallbackPreserveLanguageNote.textContent = ai?.preserve_source_language
      ? "Language lock is active for built-in presets. Custom prompts are sent unchanged."
      : "Language lock is off for built-in presets. Refinement may switch language when model confidence drifts.";
  }
  if (dom.aiFallbackLowLatencyMode) {
    dom.aiFallbackLowLatencyMode.checked = Boolean(ai?.low_latency_mode);
  }
  if (dom.aiFallbackLowLatencyNote) {
    dom.aiFallbackLowLatencyNote.textContent = ai?.low_latency_mode
      ? "Low latency active: max_tokens is capped to <= 512 and temperature to <= 0.2 (currently forced to 0.15 if higher)."
      : "Standard latency: larger generation/context budgets, potentially slower refinement.";
  }
  if (dom.aiFallbackMaxTokens) {
    dom.aiFallbackMaxTokens.value = String(ai?.max_tokens ?? 4000);
  }
  const userPromptPresets = normalizeUserRefinementPromptPresets(ai?.prompt_presets);
  ai.prompt_presets = userPromptPresets;
  const activePromptPresetId = normalizeActiveRefinementPromptPresetId(
    ai?.active_prompt_preset_id,
    ai?.prompt_profile,
    userPromptPresets
  );
  ai.active_prompt_preset_id = activePromptPresetId;
  renderPromptPresetCards(userPromptPresets, activePromptPresetId);
  const selectedUserPromptPreset = findUserRefinementPromptPresetByOptionId(
    userPromptPresets,
    activePromptPresetId
  );
  const isNewPresetMode = activePromptPresetId === NEW_REFINEMENT_PROMPT_OPTION_ID;
  const promptProfile = selectedUserPromptPreset || isNewPresetMode
    ? "custom"
    : normalizeRefinementPromptPreset(activePromptPresetId);
  ai.prompt_profile = promptProfile;
  ai.custom_prompt_enabled = promptProfile === "custom";
  const effectiveLanguageHint = resolveEffectiveAsrLanguageHint(
    settings.language_mode,
    settings.language_pinned
  );
  const promptPreview = resolveEffectiveRefinementPrompt(
    promptProfile,
    effectiveLanguageHint,
    ai?.custom_prompt,
    Boolean(ai?.preserve_source_language ?? true)
  );
  const isCustomPrompt = activePromptPresetId === "custom";
  const isUserPrompt = Boolean(selectedUserPromptPreset);
  const isBuiltInPrompt = !isCustomPrompt && !isUserPrompt && !isNewPresetMode;
  const shownPrompt = isBuiltInPrompt
    ? promptPreview
    : ai?.custom_prompt || selectedUserPromptPreset?.prompt || "";
  if (dom.aiFallbackPromptPreviewLabel) {
    dom.aiFallbackPromptPreviewLabel.textContent = isBuiltInPrompt
      ? "Prompt preview"
      : isUserPrompt
        ? "User preset"
        : isNewPresetMode
          ? "New preset prompt"
          : "Custom prompt";
  }
  if (dom.aiFallbackPromptPreviewHint) {
    dom.aiFallbackPromptPreviewHint.textContent = isBuiltInPrompt
      ? "Built-in preset is read-only and acts as source of truth."
      : isUserPrompt
        ? "User presets are editable. Save explicitly or switch presets to auto-save."
        : isNewPresetMode
          ? "Start with an empty name and prompt, then click New."
          : "Custom prompt is editable and sent as-is.";
  }
  if (dom.aiFallbackCustomPrompt) {
    dom.aiFallbackCustomPrompt.value = shownPrompt;
    dom.aiFallbackCustomPrompt.readOnly = isBuiltInPrompt;
    dom.aiFallbackCustomPrompt.classList.toggle("is-readonly", isBuiltInPrompt);
  }
  // Show the name/save/delete row only when editing is possible
  if (dom.aiFallbackPresetNameField) {
    dom.aiFallbackPresetNameField.hidden = !(isUserPrompt || isNewPresetMode);
  }
  if (dom.aiFallbackPromptPresetName) {
    if (document.activeElement !== dom.aiFallbackPromptPresetName) {
      if (isUserPrompt) {
        dom.aiFallbackPromptPresetName.value = selectedUserPromptPreset?.name || "";
      } else if (!isNewPresetMode) {
        dom.aiFallbackPromptPresetName.value = "";
      }
    }
  }
  if (dom.aiFallbackPromptPresetSave) {
    if (isUserPrompt) {
      dom.aiFallbackPromptPresetSave.textContent = "Save";
      dom.aiFallbackPromptPresetSave.disabled = false;
      dom.aiFallbackPromptPresetSave.title = "Save changes to this user preset";
    } else if (isNewPresetMode) {
      dom.aiFallbackPromptPresetSave.textContent = "Save new preset";
      dom.aiFallbackPromptPresetSave.disabled = false;
      dom.aiFallbackPromptPresetSave.title = "Create a new user preset";
    } else {
      dom.aiFallbackPromptPresetSave.textContent = "Save";
      dom.aiFallbackPromptPresetSave.disabled = true;
    }
  }
  if (dom.aiFallbackPromptPresetDelete) {
    dom.aiFallbackPromptPresetDelete.disabled = !isUserPrompt;
    dom.aiFallbackPromptPresetDelete.hidden = !isUserPrompt;
  }
}

export function renderSettings() {
  if (!settings) return;
  ensureContinuousDumpDefaults();
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
  applyOverlayDimensionSliderBounds();
  if (dom.captureEnabledToggle) dom.captureEnabledToggle.checked = settings.capture_enabled;
  if (dom.transcribeEnabledToggle) dom.transcribeEnabledToggle.checked = settings.transcribe_enabled;
  if (dom.modeSelect) dom.modeSelect.value = settings.mode;
  if (dom.pttHotkey) dom.pttHotkey.value = settings.hotkey_ptt;
  if (dom.toggleHotkey) dom.toggleHotkey.value = settings.hotkey_toggle;
  syncCaptureModeVisibility(settings.mode, settings.ptt_use_vad);
  if (dom.deviceSelect) dom.deviceSelect.value = settings.input_device;
  if (dom.languageSelect) dom.languageSelect.value = settings.language_mode;
  if (dom.languagePinnedToggle) dom.languagePinnedToggle.checked = settings.language_pinned;
  syncAsrLanguageHintUi();
  if (dom.modelSourceSelect) dom.modelSourceSelect.value = settings.model_source;
  if (dom.modelCustomUrl) dom.modelCustomUrl.value = settings.model_custom_url ?? "";
  if (dom.modelStoragePath && settings.model_storage_dir) {
    dom.modelStoragePath.value = settings.model_storage_dir;
  }
  if (dom.modelCustomUrlField) {
    dom.modelCustomUrlField.classList.toggle("hidden", settings.model_source !== "custom");
  }
  if (dom.audioCuesToggle) dom.audioCuesToggle.checked = settings.audio_cues;
  if (dom.pttUseVadToggle) dom.pttUseVadToggle.checked = settings.ptt_use_vad;
  if (dom.audioCuesVolume) dom.audioCuesVolume.value = Math.round(settings.audio_cues_volume * 100).toString();
  if (dom.audioCuesVolumeValue) {
    dom.audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
  }
  if (dom.hallucinationFilterToggle) {
    dom.hallucinationFilterToggle.checked = settings.hallucination_filter_enabled;
  }
  if (dom.activationWordsToggle) {
    dom.activationWordsToggle.checked = settings.activation_words_enabled;
  }
  if (dom.activationWordsList) {
    dom.activationWordsList.value = settings.activation_words.join('\n');
  }
  if (dom.activationWordsConfig) {
    dom.activationWordsConfig.classList.toggle('hidden', !settings.activation_words_enabled);
  }
  if (dom.micGain) dom.micGain.value = Math.round(settings.mic_input_gain_db).toString();
  if (dom.micGainValue) {
    const gain = Math.round(settings.mic_input_gain_db);
    dom.micGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
  }
  // Display start threshold in dB (main user-facing threshold)
  const vadThresholdDb = thresholdToDb(settings.vad_threshold_start, VAD_DB_FLOOR);
  if (dom.vadThreshold) dom.vadThreshold.value = Math.round(vadThresholdDb).toString();
  if (dom.vadThresholdValue) dom.vadThresholdValue.textContent = `${Math.round(vadThresholdDb)} dB`;
  if (dom.vadSilence) dom.vadSilence.value = settings.vad_silence_ms.toString();
  if (dom.vadSilenceValue) dom.vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
  if (dom.transcribeHotkey) dom.transcribeHotkey.value = settings.transcribe_hotkey;
  if (dom.toggleActivationWordsHotkey) dom.toggleActivationWordsHotkey.value = settings.hotkey_toggle_activation_words;
  if (dom.transcribeDeviceSelect) {
    dom.transcribeDeviceSelect.value = settings.transcribe_output_device;
    // If the stored device ID is not present in the current option list, the browser
    // silently leaves the dropdown on "Default (System)" (value = "default").
    // Sync the settings object so the next persistSettings() sends the actual value.
    if (dom.transcribeDeviceSelect.value !== settings.transcribe_output_device) {
      settings.transcribe_output_device = dom.transcribeDeviceSelect.value;
    }
  }
  if (dom.transcribeVadToggle) dom.transcribeVadToggle.checked = settings.transcribe_vad_mode;
  const transcribeThresholdDb = thresholdToDb(settings.transcribe_vad_threshold, VAD_DB_FLOOR);
  if (dom.transcribeVadThreshold) {
    dom.transcribeVadThreshold.value = Math.round(transcribeThresholdDb).toString();
  }
  if (dom.transcribeVadThresholdValue) {
    dom.transcribeVadThresholdValue.textContent = `${Math.round(transcribeThresholdDb)} dB`;
  }
  if (dom.transcribeVadSilence) {
    dom.transcribeVadSilence.value = settings.transcribe_vad_silence_ms.toString();
  }
  if (dom.transcribeVadSilenceValue) {
    dom.transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
  }
  updateTranscribeThreshold(settings.transcribe_vad_threshold);
  updateTranscribeVadVisibility(settings.transcribe_vad_mode);
  if (dom.transcribeBatchInterval) {
    dom.transcribeBatchInterval.value = settings.transcribe_batch_interval_ms.toString();
  }
  if (dom.transcribeBatchValue) {
    dom.transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
  }
  if (dom.transcribeChunkOverlap) {
    dom.transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
  }
  if (dom.transcribeOverlapValue) {
    dom.transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
  }
  if (dom.transcribeGain) {
    dom.transcribeGain.value = Math.round(settings.transcribe_input_gain_db).toString();
  }
  if (dom.transcribeGainValue) {
    const gain = Math.round(settings.transcribe_input_gain_db);
    dom.transcribeGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
  }
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
  if (dom.overlayMinRadius) {
    const clamped = clampToSliderBounds(
      dom.overlayMinRadius,
      Math.round(settings.overlay_min_radius)
    );
    dom.overlayMinRadius.value = clamped.toString();
    settings.overlay_min_radius = clamped;
  }
  if (dom.overlayMinRadiusValue) dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
  if (dom.overlayMaxRadius) {
    const clamped = clampToSliderBounds(
      dom.overlayMaxRadius,
      Math.round(settings.overlay_max_radius)
    );
    dom.overlayMaxRadius.value = clamped.toString();
    settings.overlay_max_radius = clamped;
  }
  if (dom.overlayMaxRadiusValue) dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
  const overlayStyleValue = settings.overlay_style || "dot";
  if (dom.overlayStyle) dom.overlayStyle.value = overlayStyleValue;
  if (dom.overlayRefiningIndicatorEnabled) {
    dom.overlayRefiningIndicatorEnabled.checked = settings.overlay_refining_indicator_enabled ?? true;
  }
  settings.overlay_refining_indicator_preset = normalizeOverlayRefiningPreset(
    settings.overlay_refining_indicator_preset
  );
  if (dom.overlayRefiningIndicatorPreset) {
    dom.overlayRefiningIndicatorPreset.value = settings.overlay_refining_indicator_preset;
  }
  // Apply accent color
  settings.accent_color = normalizeColorHex(settings.accent_color, DEFAULT_ACCENT_COLOR);
  if (dom.accentColor) dom.accentColor.value = settings.accent_color;
  applyAccentColor(settings.accent_color);

  settings.overlay_refining_indicator_color = normalizeRefiningIndicatorColor(
    settings.overlay_refining_indicator_color
  );
  settings.overlay_refining_indicator_speed_ms = normalizeRefiningIndicatorSpeedMs(
    settings.overlay_refining_indicator_speed_ms
  );
  settings.overlay_refining_indicator_range = normalizeRefiningIndicatorRange(
    settings.overlay_refining_indicator_range
  );
  if (dom.overlayRefiningIndicatorColor) {
    dom.overlayRefiningIndicatorColor.value = settings.overlay_refining_indicator_color;
  }
  if (dom.overlayRefiningIndicatorSpeed) {
    dom.overlayRefiningIndicatorSpeed.value = String(settings.overlay_refining_indicator_speed_ms);
  }
  if (dom.overlayRefiningIndicatorSpeedValue) {
    dom.overlayRefiningIndicatorSpeedValue.textContent = `${settings.overlay_refining_indicator_speed_ms} ms`;
  }
  if (dom.overlayRefiningIndicatorRange) {
    dom.overlayRefiningIndicatorRange.value = String(settings.overlay_refining_indicator_range);
  }
  if (dom.overlayRefiningIndicatorRangeValue) {
    dom.overlayRefiningIndicatorRangeValue.textContent = `${settings.overlay_refining_indicator_range}%`;
  }
  updateOverlayStyleVisibility(overlayStyleValue);
  applyOverlaySharedUi(overlayStyleValue);
  if (dom.overlayPosX) {
    dom.overlayPosX.value = Math.round(
      overlayStyleValue === "kitt" ? settings.overlay_kitt_pos_x : settings.overlay_pos_x
    ).toString();
  }
  if (dom.overlayPosY) {
    dom.overlayPosY.value = Math.round(
      overlayStyleValue === "kitt" ? settings.overlay_kitt_pos_y : settings.overlay_pos_y
    ).toString();
  }
  if (dom.overlayKittMinWidth) dom.overlayKittMinWidth.value = Math.round(settings.overlay_kitt_min_width).toString();
  if (dom.overlayKittMinWidthValue) dom.overlayKittMinWidthValue.textContent = `${Math.round(settings.overlay_kitt_min_width)}`;
  if (dom.overlayKittMaxWidth) {
    const clamped = clampToSliderBounds(
      dom.overlayKittMaxWidth,
      Math.round(settings.overlay_kitt_max_width)
    );
    dom.overlayKittMaxWidth.value = clamped.toString();
    settings.overlay_kitt_max_width = clamped;
  }
  if (dom.overlayKittMaxWidthValue) dom.overlayKittMaxWidthValue.textContent = `${Math.round(settings.overlay_kitt_max_width)}`;
  if (dom.overlayKittHeight) dom.overlayKittHeight.value = Math.round(settings.overlay_kitt_height).toString();
  if (dom.overlayKittHeightValue) dom.overlayKittHeightValue.textContent = `${Math.round(settings.overlay_kitt_height)}`;
  renderOverlayHealthNote();

  // Quality & Encoding settings
  if (dom.opusEnabledToggle) {
    dom.opusEnabledToggle.checked = settings.opus_enabled ?? true;
  }
  if (dom.opusBitrateSelect) {
    dom.opusBitrateSelect.value = (settings.opus_bitrate_kbps ?? 64).toString();
  }
  if (dom.autoSaveSystemAudioToggle) {
    dom.autoSaveSystemAudioToggle.checked = settings.auto_save_system_audio ?? false;
  }
  if (dom.autoSaveMicAudioToggle) {
    dom.autoSaveMicAudioToggle.checked = settings.auto_save_mic_audio ?? false;
  }
  if (dom.continuousDumpEnabledToggle) {
    dom.continuousDumpEnabledToggle.checked = settings.continuous_dump_enabled ?? true;
  }
  if (dom.continuousDumpProfile) {
    dom.continuousDumpProfile.value = settings.continuous_dump_profile ?? "balanced";
  }
  if (dom.continuousHardCut) {
    dom.continuousHardCut.value = String(settings.continuous_hard_cut_ms ?? 45000);
  }
  if (dom.continuousHardCutValue) {
    dom.continuousHardCutValue.textContent = `${Math.round((settings.continuous_hard_cut_ms ?? 45000) / 1000)}s`;
  }
  if (dom.continuousMinChunk) {
    dom.continuousMinChunk.value = String(settings.continuous_min_chunk_ms ?? 1000);
  }
  if (dom.continuousMinChunkValue) {
    dom.continuousMinChunkValue.textContent = `${((settings.continuous_min_chunk_ms ?? 1000) / 1000).toFixed(1)}s`;
  }
  if (dom.continuousPreRoll) {
    dom.continuousPreRoll.value = String(settings.continuous_pre_roll_ms ?? 300);
  }
  if (dom.continuousPreRollValue) {
    dom.continuousPreRollValue.textContent = `${((settings.continuous_pre_roll_ms ?? 300) / 1000).toFixed(2)}s`;
  }
  if (dom.continuousPostRoll) {
    dom.continuousPostRoll.value = String(settings.continuous_post_roll_ms ?? 200);
  }
  if (dom.continuousPostRollValue) {
    dom.continuousPostRollValue.textContent = `${((settings.continuous_post_roll_ms ?? 200) / 1000).toFixed(2)}s`;
  }
  if (dom.continuousKeepalive) {
    dom.continuousKeepalive.value = String(settings.continuous_idle_keepalive_ms ?? 60000);
  }
  if (dom.continuousKeepaliveValue) {
    dom.continuousKeepaliveValue.textContent = `${Math.round((settings.continuous_idle_keepalive_ms ?? 60000) / 1000)}s`;
  }
  if (dom.continuousSystemOverrideToggle) {
    dom.continuousSystemOverrideToggle.checked = settings.continuous_system_override_enabled ?? false;
  }
  if (dom.continuousSystemSoftFlush) {
    dom.continuousSystemSoftFlush.value = String(settings.continuous_system_soft_flush_ms ?? 10000);
  }
  if (dom.continuousSystemSoftFlushValue) {
    dom.continuousSystemSoftFlushValue.textContent = `${Math.round((settings.continuous_system_soft_flush_ms ?? 10000) / 1000)}s`;
  }
  if (dom.continuousSystemSilenceFlush) {
    dom.continuousSystemSilenceFlush.value = String(settings.continuous_system_silence_flush_ms ?? 1200);
  }
  if (dom.continuousSystemSilenceFlushValue) {
    dom.continuousSystemSilenceFlushValue.textContent = `${((settings.continuous_system_silence_flush_ms ?? 1200) / 1000).toFixed(1)}s`;
  }
  if (dom.continuousSystemHardCut) {
    dom.continuousSystemHardCut.value = String(settings.continuous_system_hard_cut_ms ?? 45000);
  }
  if (dom.continuousSystemHardCutValue) {
    dom.continuousSystemHardCutValue.textContent = `${Math.round((settings.continuous_system_hard_cut_ms ?? 45000) / 1000)}s`;
  }
  if (dom.continuousMicOverrideToggle) {
    dom.continuousMicOverrideToggle.checked = settings.continuous_mic_override_enabled ?? false;
  }
  if (dom.continuousMicSoftFlush) {
    dom.continuousMicSoftFlush.value = String(settings.continuous_mic_soft_flush_ms ?? 10000);
  }
  if (dom.continuousMicSoftFlushValue) {
    dom.continuousMicSoftFlushValue.textContent = `${Math.round((settings.continuous_mic_soft_flush_ms ?? 10000) / 1000)}s`;
  }
  if (dom.continuousMicSilenceFlush) {
    dom.continuousMicSilenceFlush.value = String(settings.continuous_mic_silence_flush_ms ?? 1200);
  }
  if (dom.continuousMicSilenceFlushValue) {
    dom.continuousMicSilenceFlushValue.textContent = `${((settings.continuous_mic_silence_flush_ms ?? 1200) / 1000).toFixed(1)}s`;
  }
  if (dom.continuousMicHardCut) {
    dom.continuousMicHardCut.value = String(settings.continuous_mic_hard_cut_ms ?? 45000);
  }
  if (dom.continuousMicHardCutValue) {
    dom.continuousMicHardCutValue.textContent = `${Math.round((settings.continuous_mic_hard_cut_ms ?? 45000) / 1000)}s`;
  }

  // Post-processing settings
  if (dom.postprocEnabled) {
    dom.postprocEnabled.checked = settings.postproc_enabled;
  }
  if (dom.postprocSettings) {
    dom.postprocSettings.style.display = settings.postproc_enabled ? "grid" : "none";
  }
  if (dom.postprocLanguageDerived) {
    dom.postprocLanguageDerived.textContent = derivedPostprocLanguageLabel(
      settings.postproc_language as "en" | "de" | "multi"
    );
  }
  if (dom.postprocPunctuation) {
    dom.postprocPunctuation.checked = settings.postproc_punctuation_enabled;
  }
  if (dom.postprocCapitalization) {
    dom.postprocCapitalization.checked = settings.postproc_capitalization_enabled;
  }
  if (dom.postprocNumbers) {
    dom.postprocNumbers.checked = settings.postproc_numbers_enabled;
  }
  if (dom.postprocCustomVocabEnabled) {
    dom.postprocCustomVocabEnabled.checked = settings.postproc_custom_vocab_enabled;
  }
  if (dom.postprocCustomVocabConfig) {
    dom.postprocCustomVocabConfig.style.display = settings.postproc_custom_vocab_enabled ? "block" : "none";
  }
  renderVocabulary();

  renderAIRefinementTab();
  renderVoiceOutputSettings();
}

/**
 * Render the AI Refinement tab content.
 * Covers provider/model setup (AI Fallback) and topic keyword editor.
 * Called from renderSettings() and can be called independently after
 * provider-specific changes.
 */
export function renderAIRefinementTab(): void {
  ensureTopicKeywordDefaults();
  syncAIRefinementExpanders();
  renderAIFallbackSettingsUi();
  renderTopicKeywords();
  renderAIRefinementStaticHelp();
}

/**
 * Render topic keyword editor in settings
 */
export async function renderTopicKeywords(): Promise<void> {
  if (!dom.topicKeywordsList || !settings) return;
  const currentSettings = settings;
  ensureTopicKeywordDefaults();
  const keywords = cloneTopicKeywords(currentSettings.topic_keywords);

  dom.topicKeywordsList.innerHTML = "";

  Object.entries(keywords).forEach(([topic, words]) => {
    const container = document.createElement("div");
    container.className = "field";
    container.style.marginBottom = "12px";

    const label = document.createElement("span");
    label.className = "field-label";
    label.textContent = `${topic.charAt(0).toUpperCase() + topic.slice(1)} keywords`;

    const input = document.createElement("input");
    input.type = "text";
    input.value = words.join(", ");
    input.placeholder = "Separate keywords with commas";
    input.title = `Comma-separated keywords for the "${topic}" topic`;
    input.addEventListener("change", async () => {
      const updated = cloneTopicKeywords(currentSettings.topic_keywords);
      updated[topic] = input.value
        .split(",")
        .map((w) => w.trim().toLowerCase())
        .filter((w) => w.length > 0);
      currentSettings.topic_keywords = normalizeTopicKeywords(updated);
      setTopicKeywords(currentSettings.topic_keywords);
      await persistSettings();
    });

    container.appendChild(label);
    container.appendChild(input);
    if (dom.topicKeywordsList) {
      dom.topicKeywordsList.appendChild(container);
    }
  });
}

/**
 * Render Voice Output Settings from settings.voice_output_settings to the UI.
 */
export function renderVoiceOutputSettings(): void {
  if (!settings?.voice_output_settings) return;

  const vo = settings.voice_output_settings;

  if (dom.voiceOutputDefaultProvider) {
    dom.voiceOutputDefaultProvider.value = vo.default_provider ?? "windows_native";
  }
  if (dom.voiceOutputFallbackProvider) {
    dom.voiceOutputFallbackProvider.value = vo.fallback_provider ?? "windows_native";
  }
  if (dom.voiceOutputPolicy) {
    dom.voiceOutputPolicy.value = vo.output_policy ?? "agent_replies_only";
  }

  // Rate slider
  if (dom.voiceOutputRate) {
    const rate = vo.rate ?? 1.0;
    dom.voiceOutputRate.value = String(rate);
    if (dom.voiceOutputRateValue) {
      dom.voiceOutputRateValue.textContent = rate.toFixed(2);
    }
  }

  // Volume slider
  if (dom.voiceOutputVolume) {
    const volume = vo.volume ?? 1.0;
    dom.voiceOutputVolume.value = String(volume);
    if (dom.voiceOutputVolumeValue) {
      dom.voiceOutputVolumeValue.textContent = volume.toFixed(2);
    }
  }

  // Piper paths
  if (dom.voiceOutputPiperBinary) {
    dom.voiceOutputPiperBinary.value = vo.piper_binary_path ?? "";
  }
  if (dom.voiceOutputPiperModel) {
    dom.voiceOutputPiperModel.value = vo.piper_model_path ?? "";
  }
  if (dom.voiceOutputPiperModelDir) {
    dom.voiceOutputPiperModelDir.value = vo.piper_model_dir ?? "";
  }
}
