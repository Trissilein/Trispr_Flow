// Transcription + Whisper Backend wiring (R2 slice 4).

import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "../types";
import * as dom from "../dom-refs";
import { settings } from "../state";
import {
  persistSettings,
  updateTranscribeVadVisibility,
  updateTranscribeThreshold,
  syncCaptureModeVisibility,
  renderSettings,
} from "../settings";
import { renderHero } from "../ui-state";
import { setupHotkeyRecorder } from "../hotkeys";
import { updateRangeAria } from "../accessibility";
import { showToast } from "../toast";
import { dbToLevel, VAD_DB_FLOOR } from "../ui-helpers";
import { onChangePersist, scheduleSettingsRender } from "./wire-helpers";
import { refreshModels, refreshModelsDir } from "../models";

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

async function syncOpusToggles(source: HTMLInputElement): Promise<void> {
  if (!settings) return;
  settings.opus_enabled = source.checked;
  if (dom.opusEnabledToggle && dom.opusEnabledToggle !== source) dom.opusEnabledToggle.checked = source.checked;
  if (dom.opusArchiveToggle && dom.opusArchiveToggle !== source) dom.opusArchiveToggle.checked = source.checked;
  await persistSettings();
}

let backendSwitchBusy = false;

async function setWhisperBackendPreference(backend: "cuda" | "vulkan"): Promise<void> {
  if (!settings || backendSwitchBusy) return;
  const normalizedCurrent = (settings.local_backend_preference ?? "auto").trim().toLowerCase();
  if (normalizedCurrent === backend) return;
  backendSwitchBusy = true;
  if (dom.gpuBackendCudaBtn) dom.gpuBackendCudaBtn.disabled = true;
  if (dom.gpuBackendVulkanBtn) dom.gpuBackendVulkanBtn.disabled = true;
  try {
    settings.local_backend_preference = backend;
    renderHero();
    await persistSettings();
    renderHero();
    showToast({
      type: "success",
      title: "Backend updated",
      message: `Whisper backend preference set to ${backend.toUpperCase()}.`,
      duration: 2200,
    });
  } finally {
    backendSwitchBusy = false;
    if (dom.gpuBackendCudaBtn) dom.gpuBackendCudaBtn.disabled = false;
    if (dom.gpuBackendVulkanBtn) dom.gpuBackendVulkanBtn.disabled = false;
  }
}

export function wireTranscription(): void {
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

  setupHotkeyRecorder("ptt", dom.pttHotkey, dom.pttHotkeyRecord, dom.pttHotkeyStatus);
  setupHotkeyRecorder("toggle", dom.toggleHotkey, dom.toggleHotkeyRecord, dom.toggleHotkeyStatus);
  setupHotkeyRecorder("transcribe", dom.transcribeHotkey, dom.transcribeHotkeyRecord, dom.transcribeHotkeyStatus);
  setupHotkeyRecorder("toggleActivationWords", dom.toggleActivationWordsHotkey, dom.toggleActivationWordsHotkeyRecord, dom.toggleActivationWordsHotkeyStatus);

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

  dom.opusEnabledToggle?.addEventListener("change", () => void syncOpusToggles(dom.opusEnabledToggle!));
  dom.opusArchiveToggle?.addEventListener("change", () => void syncOpusToggles(dom.opusArchiveToggle!));

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

  dom.gpuBackendCudaBtn?.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    void setWhisperBackendPreference("cuda");
  });

  dom.gpuBackendVulkanBtn?.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    void setWhisperBackendPreference("vulkan");
  });

  const purgeTrigger = dom.gpuPurgeBtn ?? dom.gpuStatusItem;
  purgeTrigger?.addEventListener("click", async () => {
    if (!dom.gpuVramLabel) return;
    const originalVramText = dom.gpuVramLabel.textContent;
    if (dom.gpuPurgeBtn) {
      dom.gpuPurgeBtn.disabled = true;
    } else if (dom.gpuStatusItem) {
      dom.gpuStatusItem.style.pointerEvents = "none";
    }
    dom.gpuVramLabel.textContent = "Purging...";
    try {
      await invoke("purge_gpu_memory");
      dom.gpuVramLabel.textContent = "Purged ✓";
      setTimeout(() => {
        dom.gpuVramLabel!.textContent = originalVramText;
        if (dom.gpuPurgeBtn) {
          dom.gpuPurgeBtn.disabled = false;
        } else if (dom.gpuStatusItem) {
          dom.gpuStatusItem.style.pointerEvents = "auto";
        }
      }, 2000);
    } catch (error) {
      dom.gpuVramLabel.textContent = "Error";
      if (dom.gpuPurgeBtn) {
        dom.gpuPurgeBtn.disabled = false;
      } else if (dom.gpuStatusItem) {
        dom.gpuStatusItem.style.pointerEvents = "auto";
      }
      setTimeout(() => {
        dom.gpuVramLabel!.textContent = originalVramText;
      }, 3000);
    }
  });

  if (!dom.gpuPurgeBtn) {
    dom.gpuStatusItem?.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        dom.gpuStatusItem?.click();
      }
    });
  }

  // Whisper model source & storage controls (R2 slice 4 backfill).
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
}
