import { overlayHealth, runtimeDiagnostics, settings, startupStatus } from "../state";
import * as dom from "../dom-refs";
import { DEFAULT_TOPICS, setTopicKeywords, type TopicKeywords } from "../history";
import { renderAIRefinementStaticHelp } from "../ai-refinement-help";
import {
  getOllamaRuntimeCardState,
  getOllamaRuntimeVersionCatalog,
  isOnlineVersionFetchInProgress,
} from "../ollama-models";
import {
  RUNTIME_VERSION_ACTION_EVENT,
  RUNTIME_VERSION_COLUMNS,
  buildRuntimeVersionRows,
  syncRuntimeTarget,
} from "../ollama-runtime-versions";
import { renderZebraList } from "../ollama-zebra-list";
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
import type { UserRefinementPromptPreset } from "../types";
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

function renderRefinementPipelineNote() {
  if (!settings || !dom.refinementPipelineNote) return;
  const aiEnabled = Boolean(settings.ai_fallback?.enabled);
  const rulesEnabled = Boolean(settings.postproc_enabled);
  const aiReady = Boolean(startupStatus?.ollama_ready);
  const aiStarting = Boolean(startupStatus?.ollama_starting);
  const label = "Ollama";

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

function syncOllamaRuntimeTargetWithInstalledRuntime(runningVersion?: string | null): void {
  if (!settings?.providers?.ollama) return;
  // Legacy default (0.20.2), empty target, and a target nobody picked that is older than the
  // running runtime (never a downgrade offer). A pick in the version list stays as it is.
  syncRuntimeTarget(settings.providers.ollama, runningVersion);
}

export function __resetForTesting(): void {
  _expanderStateCache = null;
  aiRuntimeStateDriftLogged = false;
}

export function renderAIFallbackSettingsUi() {
  if (!settings) return;
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
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
  ai.provider = "ollama";

  const runtimeCardState = getOllamaRuntimeCardState();
  syncOllamaRuntimeTargetWithInstalledRuntime(runtimeCardState.version);
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
  const aiRuntimeBannerVisible = Boolean(ai?.enabled) && runtimeStarting;
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
  if (dom.aiFallbackLocalLane) {
    dom.aiFallbackLocalLane.classList.toggle("is-active", true);
    dom.aiFallbackLocalLane.classList.toggle("is-runtime-busy", runtimeCardState.busy);
    dom.aiFallbackLocalLane.setAttribute("aria-pressed", "true");
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
    const statusText = `Runtime ${healthText} • Source: ${runtimeCardState.source} • Version: ${runtimeCardState.version} • Process: ${processText}`;
    dom.aiFallbackLocalPrimaryStatus.textContent = statusText;
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
  if (dom.aiFallbackRuntimeVersionList) {
    const host = dom.aiFallbackRuntimeVersionList;
    renderZebraList(host, {
      label: "Ollama runtime versions",
      columns: RUNTIME_VERSION_COLUMNS,
      emptyText: "No runtime versions known yet. Use “Get versions”.",
      busy: runtimeCardState.busy,
      rows: buildRuntimeVersionRows({
        catalog: getOllamaRuntimeVersionCatalog(),
        // Only a server that answers is "running"; a stopped one is just the recorded version.
        runningVersion: runtimeHealthy ? runtimeCardState.version : null,
        targetVersion: settings.providers.ollama.runtime_target_version || "",
        recordedVersion: settings.providers.ollama.runtime_version,
        busy: runtimeCardState.busy,
      }),
      onAction: (version, action) => {
        host.dispatchEvent(
          new CustomEvent(RUNTIME_VERSION_ACTION_EVENT, { detail: { version, action } })
        );
      },
    });
  }

  if (dom.aiFallbackLocalPrimaryAction) {
    dom.aiFallbackLocalPrimaryAction.title = "Install or start local Ollama runtime";
  }
  if (dom.aiFallbackLocalImportAction) {
    dom.aiFallbackLocalImportAction.title = "Import a local GGUF or Modelfile into Ollama";
  }

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
      ? "Keeps refined output in the source language for built-in and custom presets. Custom prompt text is not changed. LLM Prompt is exempt and requests English output."
      : "Language guard is off for built-in and custom presets; the model may change language. Custom prompt text is not changed. LLM Prompt is exempt and requests English output.";
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
