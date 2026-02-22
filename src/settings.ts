// Settings persistence and UI rendering
import { invoke } from "@tauri-apps/api/core";
import { settings } from "./state";
import * as dom from "./dom-refs";
import { thresholdToDb, VAD_DB_FLOOR } from "./ui-helpers";
import { renderVocabulary } from "./event-listeners";
import { getTopicKeywords, setTopicKeywords } from "./history";
import type { AIProviderSettings, AIFallbackProvider, OllamaSettings } from "./types";

function ensureContinuousDumpDefaults() {
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
  try {
    await invoke("save_settings", { settings });
  } catch (error) {
    console.error("save_settings failed", error);
  }
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
  if (dom.overlayRise) dom.overlayRise.value = shared.rise_ms.toString();
  if (dom.overlayRiseValue) dom.overlayRiseValue.textContent = `${shared.rise_ms}`;
  if (dom.overlayFall) dom.overlayFall.value = shared.fall_ms.toString();
  if (dom.overlayFallValue) dom.overlayFallValue.textContent = `${shared.fall_ms}`;
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

const AI_FALLBACK_PROVIDER_IDS: AIFallbackProvider[] = ["claude", "openai", "gemini", "ollama"];

function normalizeAIFallbackProvider(provider: string | undefined): AIFallbackProvider {
  if (provider && AI_FALLBACK_PROVIDER_IDS.includes(provider as AIFallbackProvider)) {
    return provider as AIFallbackProvider;
  }
  return "ollama";
}

function getProviderSettings(provider: AIFallbackProvider): AIProviderSettings | null {
  if (!settings?.providers) return null;
  if (provider === "claude") return settings.providers.claude;
  if (provider === "openai") return settings.providers.openai;
  if (provider === "gemini") return settings.providers.gemini;
  // Ollama uses OllamaSettings, not AIProviderSettings
  return null;
}

function getOllamaSettings(): OllamaSettings | null {
  return settings?.providers?.ollama ?? null;
}

function applyOllamaProviderVisibility(isOllama: boolean) {
  if (dom.aiFallbackOllamaSection)
    dom.aiFallbackOllamaSection.style.display = isOllama ? "block" : "none";
  if (dom.aiFallbackApiKeySection)
    dom.aiFallbackApiKeySection.style.display = isOllama ? "none" : "block";
}

function renderAIFallbackModelOptions(provider: AIFallbackProvider, selectedModel: string) {
  if (!dom.aiFallbackModel) return;

  const models = provider === "ollama"
    ? (getOllamaSettings()?.available_models ?? [])
    : (getProviderSettings(provider)?.available_models ?? []);

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
    const placeholder = provider === "ollama" ? "Click Refresh to load models" : "No models available";
    const option = document.createElement("option");
    option.value = selectedModel || "";
    option.textContent = selectedModel || placeholder;
    dom.aiFallbackModel.appendChild(option);
    dom.aiFallbackModel.value = option.value;
  }
}

export function renderAIFallbackSettingsUi() {
  if (!settings) return;
  const ai = settings.ai_fallback;
  const provider = normalizeAIFallbackProvider(ai?.provider);
  const providerConfig = getProviderSettings(provider);
  const ollamaConfig = getOllamaSettings();
  const isOllama = provider === "ollama";

  if (dom.aiFallbackEnabled) {
    dom.aiFallbackEnabled.checked = Boolean(ai?.enabled);
  }
  if (dom.aiFallbackSettings) {
    dom.aiFallbackSettings.style.display = ai?.enabled ? "block" : "none";
  }
  if (dom.aiFallbackProvider) {
    dom.aiFallbackProvider.value = provider;
  }

  applyOllamaProviderVisibility(isOllama);
  renderAIFallbackModelOptions(provider, ai?.model || "");

  // Ollama: show endpoint
  if (dom.aiFallbackOllamaEndpoint && ollamaConfig) {
    dom.aiFallbackOllamaEndpoint.value = ollamaConfig.endpoint || "http://localhost:11434";
  }

  // Cloud: show API key status
  if (dom.aiFallbackKeyStatus) {
    dom.aiFallbackKeyStatus.textContent = providerConfig?.api_key_stored
      ? "API key stored in secure system keyring (fallback: local encrypted file)."
      : "No API key stored for this provider yet.";
  }

  if (dom.aiFallbackTemperature) {
    const temp = Math.max(0, Math.min(1, Number(ai?.temperature ?? 0.3)));
    dom.aiFallbackTemperature.value = temp.toFixed(2);
  }
  if (dom.aiFallbackTemperatureValue) {
    const temp = Math.max(0, Math.min(1, Number(ai?.temperature ?? 0.3)));
    dom.aiFallbackTemperatureValue.textContent = temp.toFixed(2);
  }
  if (dom.aiFallbackMaxTokens) {
    dom.aiFallbackMaxTokens.value = String(ai?.max_tokens ?? 4000);
  }
  if (dom.aiFallbackCustomPromptEnabled) {
    dom.aiFallbackCustomPromptEnabled.checked = Boolean(ai?.custom_prompt_enabled);
  }
  if (dom.aiFallbackCustomPromptField) {
    dom.aiFallbackCustomPromptField.style.display = ai?.custom_prompt_enabled ? "block" : "none";
  }
  if (dom.aiFallbackCustomPrompt) {
    dom.aiFallbackCustomPrompt.value = ai?.custom_prompt || "";
  }
}

export function renderSettings() {
  if (!settings) return;
  ensureContinuousDumpDefaults();
  if (dom.captureEnabledToggle) dom.captureEnabledToggle.checked = settings.capture_enabled;
  if (dom.transcribeEnabledToggle) dom.transcribeEnabledToggle.checked = settings.transcribe_enabled;
  if (dom.modeSelect) dom.modeSelect.value = settings.mode;
  if (dom.pttHotkey) dom.pttHotkey.value = settings.hotkey_ptt;
  if (dom.toggleHotkey) dom.toggleHotkey.value = settings.hotkey_toggle;
  const hotkeysEnabled = settings.mode === "ptt";
  if (dom.hotkeysBlock) dom.hotkeysBlock.classList.toggle("hidden", !hotkeysEnabled);
  if (dom.vadBlock) dom.vadBlock.classList.toggle("hidden", hotkeysEnabled);
  if (dom.deviceSelect) dom.deviceSelect.value = settings.input_device;
  if (dom.languageSelect) dom.languageSelect.value = settings.language_mode;
  if (dom.languagePinnedToggle) dom.languagePinnedToggle.checked = settings.language_pinned;
  if (dom.modelSourceSelect) dom.modelSourceSelect.value = settings.model_source;
  if (dom.modelCustomUrl) dom.modelCustomUrl.value = settings.model_custom_url ?? "";
  if (dom.modelStoragePath && settings.model_storage_dir) {
    dom.modelStoragePath.value = settings.model_storage_dir;
  }
  if (dom.modelCustomUrlField) {
    dom.modelCustomUrlField.classList.toggle("hidden", settings.model_source !== "custom");
  }
  if (dom.cloudToggle) dom.cloudToggle.checked = settings.ai_fallback?.enabled ?? settings.cloud_fallback;
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
  if (dom.transcribeDeviceSelect) dom.transcribeDeviceSelect.value = settings.transcribe_output_device;
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
  if (dom.overlayMinRadius) dom.overlayMinRadius.value = Math.round(settings.overlay_min_radius).toString();
  if (dom.overlayMinRadiusValue) dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
  if (dom.overlayMaxRadius) dom.overlayMaxRadius.value = Math.round(settings.overlay_max_radius).toString();
  if (dom.overlayMaxRadiusValue) dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
  const overlayStyleValue = settings.overlay_style || "dot";
  if (dom.overlayStyle) dom.overlayStyle.value = overlayStyleValue;
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
  if (dom.overlayKittMaxWidth) dom.overlayKittMaxWidth.value = Math.round(settings.overlay_kitt_max_width).toString();
  if (dom.overlayKittMaxWidthValue) dom.overlayKittMaxWidthValue.textContent = `${Math.round(settings.overlay_kitt_max_width)}`;
  if (dom.overlayKittHeight) dom.overlayKittHeight.value = Math.round(settings.overlay_kitt_height).toString();
  if (dom.overlayKittHeightValue) dom.overlayKittHeightValue.textContent = `${Math.round(settings.overlay_kitt_height)}`;

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
  if (dom.postprocLanguage) {
    dom.postprocLanguage.value = settings.postproc_language;
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
  renderAIFallbackSettingsUi();

  // Chapter settings
  if (dom.chaptersEnabled) {
    dom.chaptersEnabled.checked = settings.chapters_enabled ?? false;
  }
  if (dom.chaptersSettings) {
    dom.chaptersSettings.style.display = (settings.chapters_enabled ?? false) ? "block" : "none";
  }
  if (dom.chaptersShowIn) {
    dom.chaptersShowIn.value = settings.chapters_show_in ?? "conversation";
  }
  if (dom.chaptersMethod) {
    dom.chaptersMethod.value = settings.chapters_method ?? "hybrid";
  }

  renderTopicKeywords();
}

/**
 * Render topic keyword editor in settings
 */
export async function renderTopicKeywords(): Promise<void> {
  if (!dom.topicKeywordsList) return;
  const keywords = getTopicKeywords();

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
    input.addEventListener("change", async () => {
      const updated = { ...keywords };
      updated[topic] = input.value
        .split(",")
        .map((w) => w.trim())
        .filter((w) => w.length > 0);
      setTopicKeywords(updated);
      await persistSettings();
    });

    container.appendChild(label);
    container.appendChild(input);
    if (dom.topicKeywordsList) {
      dom.topicKeywordsList.appendChild(container);
    }
  });
}
