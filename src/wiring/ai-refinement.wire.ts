// AI Refinement wiring (R2 slice 5).
//
// Covers: AI Refinement + Post-Processing + topic-keywords +
// prompt-editor + local-runtime per CONTEXT.md. Language controls
// (languageSelect, languagePinnedToggle, whisperInputLanguageSelect) are folded
// in because they mutate AI-Refinement prompt state. See OQ-3 clause 8
// (project-spec/decisions/2026-05-15/refactoring-plan.md).

import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "../types";
import { settings } from "../state";
import * as dom from "../dom-refs";
import {
  renderAIFallbackSettingsUi,
  renderTopicKeywords,
} from "../settings/ai-refinement.settings";
import { renderSettings } from "../settings";
import {
  derivePostprocLanguageFromAsr,
  resolveEffectiveAsrLanguageHint,
} from "../language-utils";
import { persistSettings, syncDerivedLanguageSettings } from "../settings-persist";
import { addVocabRow } from "../settings/vocabulary.settings";
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
  RUNTIME_CATALOG_UPDATED_EVENT,
  getOllamaRuntimeCardState,
  importOllamaModelFromLocalFile,
  refreshOllamaInstalledModels,
  refreshOllamaRuntimeVersionCatalog,
  runRuntimeVersionAction,
  refreshOllamaRuntimeAndModels,
  refreshOllamaRuntimeState,
  renderOllamaModelManager,
  showOllamaRequiredModal,
  startOllamaRuntime,
  verifyOllamaRuntime,
} from "../ollama-models";
import {
  DEFAULT_RUNTIME_VERSION,
  RUNTIME_VERSION_ACTION_EVENT,
  syncRuntimeTarget,
  type RuntimeVersionActionId,
} from "../ollama-runtime-versions";
import { onChangePersist, scheduleSettingsRender } from "./wire-helpers";

// ── Module-level constants ──────────────────────────────────────────────────

const AI_REFINEMENT_MODULE_ID = "ai_refinement";
const AI_REFINEMENT_MIGRATION_FLAG_KEY = "ai_refinement.migrated_legacy";

// ── Private helpers ─────────────────────────────────────────────────────────

// Duplicated one-liners per OQ-3 clause 3 (private, not exported; matching
// the pattern established in refinement-pipeline-graph.ts).
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

function isLaneControlTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest("button,select,input,textarea,summary,details,label,a,.ollama-zebra-host"),
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

// `.has-unsaved-edits` is the authoritative dirty flag: the `input` listener
// adds it on keystroke, the renderer removes it when the textarea is re-sourced
// from the effective prompt.
function hasUnsavedPromptEditorEdits(): boolean {
  return Boolean(
    dom.aiFallbackCustomPrompt?.classList.contains("has-unsaved-edits"),
  );
}

// Blank the editor after a confirmed discard so the next render re-sources it
// from the newly selected preset. A stale non-empty value would otherwise be
// treated as an external edit by the renderer and never be replaced — the
// textarea would keep the old text and the dirty flag forever.
function clearPromptEditorEdits(): void {
  if (!dom.aiFallbackCustomPrompt) return;
  dom.aiFallbackCustomPrompt.value = "";
  dom.aiFallbackCustomPrompt.classList.remove("has-unsaved-edits");
  // The renderer only re-sources an unfocused textarea — make sure it may.
  if (document.activeElement === dom.aiFallbackCustomPrompt) {
    dom.aiFallbackCustomPrompt.blur();
  }
}

/** Returns true if the caller may proceed (editor clean, or user confirmed discard). */
function confirmDiscardPromptEdits(): boolean {
  if (!hasUnsavedPromptEditorEdits()) return true;
  if (!window.confirm("Discard unsaved changes to this preset?")) return false;
  clearPromptEditorEdits();
  return true;
}

function ensureAIFallbackSettingsDefaults() {
  if (!settings) return;
  if (!settings.ai_fallback) {
    settings.ai_fallback = {
      enabled: false,
      provider: "ollama",
      preserve_source_language: true,
      model: "gemma4:e4b",
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
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "per_user_zip",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: DEFAULT_RUNTIME_VERSION,
        runtime_target_explicit: false,
        last_health_check: null,
      },
    };
  }
  if (!settings.providers.ollama) {
    settings.providers.ollama = {
      endpoint: "http://127.0.0.1:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "per_user_zip",
      runtime_path: "",
      runtime_version: "",
      runtime_target_version: DEFAULT_RUNTIME_VERSION,
      runtime_target_explicit: false,
      last_health_check: null,
    };
  }
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
  // Refinement is Ollama-only; saved cloud / LM Studio / Oobabooga values land on Ollama.
  settings.ai_fallback.provider = "ollama";
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
  settings.providers.ollama.runtime_source = "per_user_zip";
  settings.providers.ollama.runtime_path ??= "";
  settings.providers.ollama.runtime_version ??= "";
  // Legacy default (0.20.2), empty target and a stale target nobody picked (older than the
  // runtime in use) are fixed here; a pick in the version list stays as it is.
  settings.providers.ollama.runtime_target_explicit ??= false;
  syncRuntimeTarget(settings.providers.ollama);
  settings.providers.ollama.last_health_check ??= null;
  settings.setup ??= {
    local_ai_wizard_completed: false,
    local_ai_wizard_pending: true,
  };
  normalizeAiRefinementModuleBindingInSettings();
  syncDerivedLanguageSettings();
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

async function activateLocalLane() {
  if (!settings) return;
  ensureAIFallbackSettingsDefaults();
  await refreshOllamaRuntimeState({ force: true });
  if (getOllamaRuntimeCardState().healthy) {
    await refreshOllamaInstalledModels();
  }
  await persistSettings();
  refreshAIUi();
}

// ── Public entry point ──────────────────────────────────────────────────────

export function wireAiRefinement(): void {
  ensureAIFallbackSettingsDefaults();

  // Language controls — folded into this slice because all three listeners
  // mutate AI-Refinement prompt state via syncActivePromptPresetSelection()
  // and refreshResolvedRefinementPromptInSettings().

  dom.languageSelect?.addEventListener("change", async () => {
    if (!settings) return;
    // Language changes swap the effective built-in prompt (EN/DE) — ask before
    // clobbering unsaved editor changes, and revert the control on cancel.
    if (!confirmDiscardPromptEdits()) {
      dom.languageSelect!.value = settings.language_mode;
      return;
    }
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
    if (!confirmDiscardPromptEdits()) {
      dom.languagePinnedToggle!.checked = settings.language_pinned;
      return;
    }
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
    if (!settings) return;
    if (!confirmDiscardPromptEdits()) {
      dom.whisperInputLanguageSelect!.value = settings.language_pinned
        ? settings.language_mode
        : "auto";
      return;
    }
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
      // Check if the runtime is available before enabling
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

    settings.ai_fallback.enabled = enabling;
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    await persistSettings();
    renderAIFallbackSettingsUi();
    renderHero();
    if (!settings.ai_fallback.enabled) {
      // Stop Ollama runtime when AI refinement is disabled
      try {
        await invoke("stop_ollama_runtime");
      } catch (error) {
        console.warn("Failed to stop Ollama runtime:", error);
      }
    } else {
      void autoStartLocalRuntimeIfNeeded("enable_toggle").finally(() => {
        renderAIFallbackSettingsUi();
        renderOllamaModelManager();
      });
      renderAIFallbackSettingsUi();
      renderOllamaModelManager();
    }
  });

  dom.aiFallbackLocalLane?.addEventListener("click", (event) => {
    if (isLaneControlTarget(event.target)) return;
    void activateLocalLane();
  });
  dom.aiFallbackLocalLane?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (isLaneControlTarget(event.target)) return;
    event.preventDefault();
    void activateLocalLane();
  });

  // Local runtime action handlers in Local Primary card
  dom.aiFallbackLocalPrimaryAction?.addEventListener("click", async () => {
    if (!settings) return;
    ensureAIFallbackSettingsDefaults();
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

  // The automatic once-per-session catalog refresh finishes after the first render.
  window.addEventListener(RUNTIME_CATALOG_UPDATED_EVENT, () => {
    renderAIFallbackSettingsUi();
  });

  // Explicit GitHub fetch for version list (bypasses the 6 h cache)
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

  // Row buttons of the runtime version list (Install / Switch to / Delete).
  dom.aiFallbackRuntimeVersionList?.addEventListener(RUNTIME_VERSION_ACTION_EVENT, async (event) => {
    if (!settings) return;
    const { version, action } = (event as CustomEvent<{ version: string; action: RuntimeVersionActionId }>).detail;
    ensureAIFallbackSettingsDefaults();
    const actionPromise = runRuntimeVersionAction(version, action);
    renderAIFallbackSettingsUi();
    await actionPromise;
    refreshAIUi();
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
      const hadUnsavedEdits = hasUnsavedPromptEditorEdits();
      if (!confirmDiscardPromptEdits()) return;
      if (!hadUnsavedEdits) {
        // Clean editor: still pick up pending name-only changes on user presets.
        const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
        if (hasPendingUserChanges) await persistSettings();
      }
      settings.ai_fallback.active_prompt_preset_id = presetId;
      syncActivePromptPresetSelection();
      refreshResolvedRefinementPromptInSettings();
      await persistSettings();
      renderAIFallbackSettingsUi();
    } else if (action === "new-preset") {
      const hadUnsavedEdits = hasUnsavedPromptEditorEdits();
      if (!confirmDiscardPromptEdits()) return;
      if (!hadUnsavedEdits) {
        const hasPendingUserChanges = applyPendingUserPresetEditsFromEditor();
        if (hasPendingUserChanges) await persistSettings();
      }
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
    const editorDirty = hasUnsavedPromptEditorEdits();
    if (!hasOverride && !editorDirty) return;
    if (
      !window.confirm(
        `Remove your customization for "${label}" and restore the factory default?`,
      )
    ) {
      return;
    }
    removePresetOverride(ai.prompt_preset_overrides, builtInId);
    clearPromptEditorEdits();
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
    if (activePresetId !== "custom") {
      // Plain custom prompt is persisted live — nothing can be lost, so no
      // dirty marker (it would trigger pointless discard confirmations).
      dom.aiFallbackCustomPrompt.classList.add("has-unsaved-edits");
    }
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

    if (presetId !== "__off__" && !confirmDiscardPromptEdits()) return;

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
    const menu = dom.engineRefinePresetMenu;
    if (menu && !menu.hidden) {
      menu.hidden = true;
      dom.engineRefinePreset?.setAttribute("aria-expanded", "false");
    }
  });
}
