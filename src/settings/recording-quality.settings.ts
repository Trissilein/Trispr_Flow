import * as dom from "../dom-refs";
import { settings } from "../state";

export function renderRecordingQualitySettings(): void {
  if (!settings) return;
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
}