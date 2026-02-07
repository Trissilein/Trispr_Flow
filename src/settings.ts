// Settings persistence and UI rendering
import { invoke } from "@tauri-apps/api/core";
import { settings } from "./state";
import * as dom from "./dom-refs";
import { thresholdToDb, VAD_DB_FLOOR } from "./ui-helpers";

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

export function renderSettings() {
  if (!settings) return;
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
  if (dom.modelSourceSelect) dom.modelSourceSelect.value = settings.model_source;
  if (dom.modelCustomUrl) dom.modelCustomUrl.value = settings.model_custom_url ?? "";
  if (dom.modelStoragePath && settings.model_storage_dir) {
    dom.modelStoragePath.value = settings.model_storage_dir;
  }
  if (dom.modelCustomUrlField) {
    dom.modelCustomUrlField.classList.toggle("hidden", settings.model_source !== "custom");
  }
  if (dom.cloudToggle) dom.cloudToggle.checked = settings.cloud_fallback;
  if (dom.audioCuesToggle) dom.audioCuesToggle.checked = settings.audio_cues;
  if (dom.pttUseVadToggle) dom.pttUseVadToggle.checked = settings.ptt_use_vad;
  if (dom.audioCuesVolume) dom.audioCuesVolume.value = Math.round(settings.audio_cues_volume * 100).toString();
  if (dom.audioCuesVolumeValue) {
    dom.audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
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
}
