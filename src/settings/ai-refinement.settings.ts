import { overlayHealth, runtimeDiagnostics, settings, startupStatus } from "../state";
import * as dom from "../dom-refs";
import { DEFAULT_TOPICS, setTopicKeywords, type TopicKeywords } from "../history";
import { renderAIRefinementStaticHelp } from "../ai-refinement-help";
import {
  getOllamaRuntimeCardState,
  getOllamaRuntimeVersionCatalog,
  isOnlineVersionFetchInProgress,
} from "../ollama-models";
import { traceFrontendWarn } from "../frontend-trace";
import { syncRefinementPipelineGraphFromSettings } from "../refinement-pipeline-graph";
import {
  BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS,
  DEFAULT_REFINEMENT_PROMPT_PRESET,
  findUserRefinementPromptPresetByOptionId,
  hasPresetOverride,
  NEW_REFINEMENT_PROMPT_OPTION_ID,
  normalizeActiveRefinementPromptPresetId,
  normalizeRefinementPromptPreset,
  normalizeUserRefinementPromptPresets,
  resolveEffectiveRefinementPrompt,
  toUserRefinementPromptOptionId,
  type BuiltInRefinementPromptPreset,
} from "../refinement-prompts";
import type {
  AIProviderSettings,
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthMethodPreference,
  OpenAICompatSettings,
  UserRefinementPromptPreset,
} from "../types";
import {
  CLOUD_PROVIDER_IDS,
  CLOUD_PROVIDER_LABELS,
  normalizeCloudProvider,
  normalizeExecutionMode,
  normalizeAuthMethodPreference,
  isVerifiedAuthStatus,
} from "../ai-provider-utils";
import {
  ensureSetupDefaults,
  persistSettings,
  syncDerivedLanguageSettings,
} from "../settings-persist";
import { resolveEffectiveAsrLanguageHint } from "../language-utils";

// AI refinement settings rendering (R3 slice 5).
//
// Exports:
//   Primary   - renderAIFallbackSettingsUi() called directly from main.ts and ai-refinement.wire.ts
//   Secondary - renderAIRefinementTab()       called by renderSettings() in index.ts
//               renderTopicKeywords()         called by ai-refinement.wire.ts
//               renderOverlayHealthNote()     called by renderSettings() in index.ts
//               __resetForTesting()           test-only state reset helper
function authStatusLabel(status?: string | null): string {
  if (status === "verified_api_key") return "Verified";
  if (status === "verified_oauth") return "Verified (OAuth)";
  return "Locked";
}

function authMethodLabel(method?: AIProviderAuthMethodPreference | null): string {
  return method === "oauth" ? "OAuth (coming soon)" : "API key";
}

function getProviderSettings(provider: AIFallbackProvider): AIProviderSettings | null {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  // Ollama uses OllamaSettings, not AIProviderSettings
  return null;
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
  compatSettings: OpenAICompatSettings | undefined,
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

    const header = document.createElement("div");
    header.className = "model-header";

    const nameEl = document.createElement("div");
    nameEl.className = "model-name";
    nameEl.textContent = modelName;
    header.appendChild(nameEl);
    card.appendChild(header);

    const statusEl = document.createElement("div");
    statusEl.className = `model-status ${isActive ? "active" : "downloaded"}`;
    statusEl.textContent = isActive ? "Active" : "Available";
    card.appendChild(statusEl);

    if (isReasoningModel(modelName)) {
      const warningEl = document.createElement("div");
      warningEl.className = "model-reasoning-warn";
      warningEl.textContent = "⚠ Reasoning model - refinement may take 20-30s. Prefer an instruct model.";
      card.appendChild(warningEl);
    }

    const actionsEl = document.createElement("div");
    actionsEl.className = "model-actions";
    card.appendChild(actionsEl);

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

export function renderOverlayHealthNote() {
  if (!dom.overlayHealthNote) return;
  const health = overlayHealth;
  if (!health) {
    dom.overlayHealthNote.hidden = true;
    dom.overlayHealthNote.textContent = "";
    return;
  }
  dom.overlayHealthNote.hidden = false;
  dom.overlayHealthNote.textContent = health.status === "failed"
    ? `Overlay degraded after ${health.attempt} recovery attempts: ${health.reason}`
    : health.status === "recovered"
      ? `Overlay recovered: ${health.reason}`
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

  const overrides = settings?.ai_fallback?.prompt_preset_overrides;

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
    const modified = hasPresetOverride(overrides, preset.id);
    const extra = modified ? "preset-chip--modified" : "";
    const chip = makeChip(preset.id, preset.label, isActive, extra, "use-preset", false);
    if (modified) chip.title = "Customized — saved";
    container.appendChild(chip);
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

const DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION = "0.20.2";

function syncOllamaRuntimeTargetWithInstalledRuntime(): void {
  if (!settings?.providers?.ollama) return;
  const ollama = settings.providers.ollama;
  const installedVersion = ollama.runtime_version?.trim() || "";
  if (!installedVersion) {
    ollama.runtime_target_version ||= DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION;
    return;
  }
  const targetVersion = ollama.runtime_target_version?.trim() || "";
  if (!targetVersion || targetVersion === DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION) {
    ollama.runtime_target_version = installedVersion;
  }
}

export function __resetForTesting(): void {
  _expanderStateCache = null;
  aiRuntimeStateDriftLogged = false;
}

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
  syncOllamaRuntimeTargetWithInstalledRuntime();
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
      settings.providers.ollama.runtime_target_version?.trim()
      || runtimeCardState.version
      || DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION;
    const optionPool = [...runtimeVersionOptions];
    const appendIfMissing = (version: string) => {
      if (!version) return;
      if (optionPool.some((entry) => entry.version === version)) return;
      optionPool.push({
        version,
        source: "online",
        selected: version === selectedVersion,
        installed: version === runtimeCardState.version,
        recommended: version === DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION,
        prerelease: /(?:-rc|-alpha|-beta)/i.test(version),
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
      option.textContent = `${entry.version} (${entry.installable ? "installable" : "not installable"})`;
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
    const selected = settings.providers.ollama.runtime_target_version || DEFAULT_OLLAMA_RUNTIME_TARGET_VERSION;
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
  ai.prompt_preset_overrides ??= {};
  const overrides = ai.prompt_preset_overrides;
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
    Boolean(ai?.preserve_source_language ?? true),
    ai?.model,
    overrides
  );
  const isCustomPrompt = activePromptPresetId === "custom";
  const isUserPrompt = Boolean(selectedUserPromptPreset);
  const isBuiltInPrompt = !isCustomPrompt && !isUserPrompt && !isNewPresetMode;
  const builtInId = isBuiltInPrompt
    ? (normalizeRefinementPromptPreset(activePromptPresetId) as BuiltInRefinementPromptPreset)
    : null;
  const builtInHasOverride = Boolean(
    builtInId && typeof overrides[builtInId] === "string" && overrides[builtInId]!.trim().length > 0
  );
  const userHasPrevious = Boolean(
    selectedUserPromptPreset?.previous_prompt &&
    selectedUserPromptPreset.previous_prompt.trim().length > 0
  );
  const shownPrompt = isBuiltInPrompt
    ? promptPreview
    : ai?.custom_prompt || selectedUserPromptPreset?.prompt || "";
  // Dirty comparisons must run against the SAVED prompt. For user presets
  // `custom_prompt` mirrors the live editor value, so comparing against
  // `shownPrompt` would never detect an edit.
  const dirtyBaseline = selectedUserPromptPreset
    ? selectedUserPromptPreset.prompt
    : shownPrompt;
  if (dom.aiFallbackPromptPreviewLabel) {
    dom.aiFallbackPromptPreviewLabel.textContent = isBuiltInPrompt
      ? builtInHasOverride
        ? "Prompt (customized)"
        : "Prompt preview"
      : isUserPrompt
        ? "User preset"
        : isNewPresetMode
          ? "New preset prompt"
          : "Custom prompt";
  }
  if (dom.aiFallbackPromptPreviewHint) {
    dom.aiFallbackPromptPreviewHint.textContent = isBuiltInPrompt
      ? builtInHasOverride
        ? "Customized — saved. Override replaces EN and DE defaults."
        : "Built-in preset — edit to customize."
      : isUserPrompt
        ? userHasPrevious
          ? "User preset — previous saved version available via Revert."
          : "User preset — Save to persist changes."
        : isNewPresetMode
          ? "Start with a name and prompt, then click Save."
          : "Custom prompt is editable and sent as-is.";
  }
  if (dom.aiFallbackCustomPrompt) {
    const textarea = dom.aiFallbackCustomPrompt;
    const isFocused = document.activeElement === textarea;
    const currentValue = textarea.value;
    // Preserve the value only when the input listener flagged a real user edit
    // (`has-unsaved-edits`). A bare value diff cannot distinguish an unsaved
    // edit from the stale text of a previously selected preset — keying on the
    // diff alone kept re-flagging clean editors as dirty and blocked newly
    // selected presets from ever loading.
    const externalDirty =
      !isFocused
      && (isBuiltInPrompt || isUserPrompt)
      && textarea.classList.contains("has-unsaved-edits")
      && currentValue.trim().length > 0
      && currentValue.trim() !== (dirtyBaseline || "").trim();
    if (!isFocused && !externalDirty) {
      textarea.value = shownPrompt;
      textarea.classList.remove("has-unsaved-edits");
    } else if (externalDirty) {
      textarea.classList.add("has-unsaved-edits");
    }
    textarea.readOnly = false;
    textarea.classList.remove("is-readonly");
  }
  // Button/name row is always visible when any chip is selected (built-in, user, or new).
  if (dom.aiFallbackPresetNameField) {
    dom.aiFallbackPresetNameField.hidden = isCustomPrompt;
  }
  if (dom.aiFallbackPresetNameInputWrap) {
    dom.aiFallbackPresetNameInputWrap.hidden = !(isUserPrompt || isNewPresetMode);
  }
  if (dom.aiFallbackPromptPresetName) {
    dom.aiFallbackPromptPresetName.hidden = !(isUserPrompt || isNewPresetMode);
    if (document.activeElement !== dom.aiFallbackPromptPresetName) {
      if (isUserPrompt) {
        dom.aiFallbackPromptPresetName.value = selectedUserPromptPreset?.name || "";
      } else if (!isNewPresetMode) {
        dom.aiFallbackPromptPresetName.value = "";
      }
    }
  }
  const textareaDirty = isTextareaDirtyAgainstEffective(dirtyBaseline);
  if (dom.aiFallbackPromptPresetSave) {
    if (isNewPresetMode) {
      dom.aiFallbackPromptPresetSave.textContent = "Save new preset";
      dom.aiFallbackPromptPresetSave.disabled = false;
      dom.aiFallbackPromptPresetSave.title = "Create a new user preset";
      dom.aiFallbackPromptPresetSave.hidden = false;
    } else if (isUserPrompt) {
      dom.aiFallbackPromptPresetSave.textContent = "Save";
      dom.aiFallbackPromptPresetSave.disabled = !textareaDirty;
      dom.aiFallbackPromptPresetSave.title = "Save changes to this user preset";
      dom.aiFallbackPromptPresetSave.hidden = false;
    } else if (isBuiltInPrompt) {
      dom.aiFallbackPromptPresetSave.textContent = "Save";
      dom.aiFallbackPromptPresetSave.disabled = !textareaDirty;
      dom.aiFallbackPromptPresetSave.title = "Save override for this built-in preset";
      dom.aiFallbackPromptPresetSave.hidden = false;
    } else {
      dom.aiFallbackPromptPresetSave.hidden = true;
    }
  }
  if (dom.aiFallbackPromptPresetReset) {
    // Always visible for built-in presets so the factory reset is discoverable;
    // enabled once there is anything to reset (saved override or unsaved edits).
    dom.aiFallbackPromptPresetReset.hidden = !isBuiltInPrompt;
    dom.aiFallbackPromptPresetReset.disabled =
      !isBuiltInPrompt || !(builtInHasOverride || textareaDirty);
  }
  if (dom.aiFallbackPromptPresetRevert) {
    const show = isUserPrompt && userHasPrevious;
    dom.aiFallbackPromptPresetRevert.hidden = !show;
    dom.aiFallbackPromptPresetRevert.disabled = !show;
  }
  if (dom.aiFallbackPromptPresetDiscard) {
    const show = textareaDirty && (isBuiltInPrompt || isUserPrompt);
    dom.aiFallbackPromptPresetDiscard.hidden = !show;
    dom.aiFallbackPromptPresetDiscard.disabled = !show;
  }
  if (dom.aiFallbackPromptPresetDelete) {
    dom.aiFallbackPromptPresetDelete.disabled = !isUserPrompt;
    dom.aiFallbackPromptPresetDelete.hidden = !isUserPrompt;
  }
}

// No focus requirement: an unsaved edit stays dirty after the textarea blurs
// (e.g. before clicking Save). When the editor is clean the render loop above
// re-sources the value from the effective prompt, so the comparison holds.
function isTextareaDirtyAgainstEffective(effective: string): boolean {
  const textarea = dom.aiFallbackCustomPrompt;
  if (!textarea) return false;
  return textarea.value.trim() !== (effective || "").trim();
}


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
