// Settings persistence and UI rendering
import { isAssistantCoreAvailable, settings } from "../state";
import * as dom from "../dom-refs";
import { applyAccentColor, DEFAULT_ACCENT_COLOR, normalizeColorHex } from "../utils";
import {
  ensureSetupDefaults,
  persistSettings,
  syncDerivedLanguageSettings,
} from "../settings-persist";
import { renderLearnedVocabChips, renderVocabulary } from "./vocabulary.settings";
import { renderOverlaySettings } from "./overlay.settings";
import { renderTranscriptionSettings } from "./transcription.settings";
import { renderVoiceOutputSettings } from "./voice-output.settings";
import {
  renderAIRefinementTab,
  renderOverlayHealthNote,
} from "./ai-refinement.settings";

export { persistSettings };

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

export function ensureCaptureRuntimeDefaults() {
  if (!settings) return;
  settings.ptt_hot_keepalive_ms ??= 30000;
}

export function ensureDiagnosticsDefaults() {
  if (!settings) return;
  settings.diagnostic_logging_enabled ??= false;
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

function renderProductModeSettings(): void {
  if (!settings) return;
  const assistantCoreAvailable = isAssistantCoreAvailable();
  if (!assistantCoreAvailable) {
    settings.product_mode = "transcribe";
  }
  const productMode = settings.product_mode === "assistant" ? "assistant" : "transcribe";
  settings.product_mode = productMode;
  const transcribeActive = productMode === "transcribe";
  const assistantActive = productMode === "assistant";
  dom.productModeControl?.toggleAttribute("hidden", !assistantCoreAvailable);
  if (dom.productModeTranscribeBtn) {
    dom.productModeTranscribeBtn.classList.toggle("is-active", transcribeActive);
    dom.productModeTranscribeBtn.setAttribute("aria-pressed", transcribeActive ? "true" : "false");
  }
  if (dom.productModeAssistantBtn) {
    dom.productModeAssistantBtn.classList.toggle("is-active", assistantActive);
    dom.productModeAssistantBtn.setAttribute("aria-pressed", assistantActive ? "true" : "false");
    dom.productModeAssistantBtn.disabled = !assistantCoreAvailable;
  }
  dom.globalOnlineControl?.toggleAttribute(
    "hidden",
    !assistantCoreAvailable || productMode !== "assistant"
  );
}

function renderGlobalOnlineModeSettings(): void {
  if (!settings) return;
  const onlineEnabled = Boolean(settings.workflow_agent?.online_enabled);
  if (dom.globalOnlineOfflineBtn) {
    dom.globalOnlineOfflineBtn.classList.toggle("is-active", !onlineEnabled);
    dom.globalOnlineOfflineBtn.setAttribute("aria-pressed", onlineEnabled ? "false" : "true");
  }
  if (dom.globalOnlineEnabledBtn) {
    dom.globalOnlineEnabledBtn.classList.toggle("is-active", onlineEnabled);
    dom.globalOnlineEnabledBtn.setAttribute("aria-pressed", onlineEnabled ? "true" : "false");
  }
}

export function renderSettings() {
  if (!settings) return;
  ensureContinuousDumpDefaults();
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
  renderProductModeSettings();
  renderGlobalOnlineModeSettings();
  renderTranscriptionSettings();
  renderOverlaySettings();
  renderVoiceOutputSettings();
  // Apply accent color
  settings.accent_color = normalizeColorHex(settings.accent_color, DEFAULT_ACCENT_COLOR);
  if (dom.accentColor) dom.accentColor.value = settings.accent_color;
  applyAccentColor(settings.accent_color);
  renderOverlayHealthNote();

  // Quality & Encoding settings
  if (dom.opusEnabledToggle) {
    dom.opusEnabledToggle.checked = settings.opus_enabled ?? true;
  }
  if (dom.opusArchiveToggle) {
    dom.opusArchiveToggle.checked = settings.opus_enabled ?? true;
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

  renderLearnedVocabChips();

  renderAIRefinementTab();
}

