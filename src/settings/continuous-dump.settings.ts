import * as dom from "../dom-refs";
import { settings } from "../state";

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

export function renderContinuousDumpSettings(): void {
  if (!settings) return;
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
}