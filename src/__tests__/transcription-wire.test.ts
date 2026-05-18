/**
 * Smoke tests for wireTranscription() - R2 slice 4.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>
    <button id="product-mode-transcribe-btn"></button>
    <button id="product-mode-assistant-btn"></button>

    <input id="capture-enabled-toggle" type="checkbox" />
    <input id="transcribe-enabled-toggle" type="checkbox" />
    <select id="mode-select"><option value="ptt">ptt</option><option value="vad">vad</option></select>

    <input id="ptt-hotkey" value="" />
    <button id="ptt-hotkey-record"></button>
    <span id="ptt-hotkey-status"></span>
    <input id="toggle-hotkey" value="" />
    <button id="toggle-hotkey-record"></button>
    <span id="toggle-hotkey-status"></span>
    <input id="transcribe-hotkey" value="" />
    <button id="transcribe-hotkey-record"></button>
    <span id="transcribe-hotkey-status"></span>
    <input id="toggle-activation-words-hotkey" value="" />
    <button id="toggle-activation-words-hotkey-record"></button>
    <span id="toggle-activation-words-hotkey-status"></span>

    <select id="device-select"><option value="mic-1">mic-1</option><option value="mic-2">mic-2</option></select>
    <select id="transcribe-device-select"><option value="default">default</option><option value="speakers">speakers</option></select>
    <input id="transcribe-vad-toggle" type="checkbox" />
    <div id="transcribe-batch-field"></div>
    <div id="transcribe-overlap-field"></div>
    <div id="transcribe-vad-threshold-field"></div>
    <div id="transcribe-vad-silence-field"></div>
    <input id="transcribe-vad-threshold" type="range" min="-80" max="0" value="-35" />
    <span id="transcribe-vad-threshold-value"></span>
    <span id="transcribe-threshold-db"></span>
    <span id="transcribe-threshold-label"></span>
    <input id="transcribe-vad-silence" type="range" min="200" max="5000" value="1200" />
    <span id="transcribe-vad-silence-value"></span>
    <input id="transcribe-batch-interval" type="range" min="4000" max="15000" value="10000" />
    <span id="transcribe-batch-value"></span>
    <input id="transcribe-chunk-overlap" type="range" min="0" max="3000" value="300" />
    <span id="transcribe-overlap-value"></span>
    <input id="transcribe-gain" type="range" min="-30" max="30" value="0" />
    <span id="transcribe-gain-value"></span>

    <input id="audio-cues-toggle" type="checkbox" />
    <input id="ptt-use-vad-toggle" type="checkbox" />
    <input id="audio-cues-volume" type="range" min="0" max="100" value="50" />
    <span id="audio-cues-volume-value"></span>
    <input id="hallucination-filter-toggle" type="checkbox" />
    <input id="activation-words-toggle" type="checkbox" />
    <textarea id="activation-words-list"></textarea>

    <input id="opus-enabled-toggle" type="checkbox" />
    <input id="opus-archive-toggle" type="checkbox" />
    <select id="opus-bitrate-select"><option value="24">24</option><option value="48">48</option></select>
    <input id="auto-save-system-audio-toggle" type="checkbox" />
    <input id="auto-save-mic-audio-toggle" type="checkbox" />

    <input id="continuous-dump-enabled-toggle" type="checkbox" />
    <select id="continuous-dump-profile"><option value="balanced">balanced</option><option value="low_latency">low_latency</option><option value="high_quality">high_quality</option></select>
    <input id="continuous-hard-cut" type="range" min="15000" max="120000" value="45000" />
    <span id="continuous-hard-cut-value"></span>
    <input id="continuous-min-chunk" type="range" min="250" max="5000" value="1000" />
    <span id="continuous-min-chunk-value"></span>
    <input id="continuous-pre-roll" type="range" min="0" max="1500" value="300" />
    <span id="continuous-pre-roll-value"></span>
    <input id="continuous-post-roll" type="range" min="0" max="1500" value="200" />
    <span id="continuous-post-roll-value"></span>
    <input id="continuous-keepalive" type="range" min="10000" max="120000" value="60000" />
    <span id="continuous-keepalive-value"></span>
    <input id="continuous-system-override-toggle" type="checkbox" />
    <input id="continuous-system-soft-flush" type="range" min="4000" max="30000" value="10000" />
    <span id="continuous-system-soft-flush-value"></span>
    <input id="continuous-system-silence-flush" type="range" min="300" max="5000" value="1200" />
    <span id="continuous-system-silence-flush-value"></span>
    <input id="continuous-system-hard-cut" type="range" min="15000" max="120000" value="45000" />
    <span id="continuous-system-hard-cut-value"></span>
    <input id="continuous-mic-override-toggle" type="checkbox" />
    <input id="continuous-mic-soft-flush" type="range" min="4000" max="30000" value="10000" />
    <span id="continuous-mic-soft-flush-value"></span>
    <input id="continuous-mic-silence-flush" type="range" min="300" max="5000" value="1200" />
    <span id="continuous-mic-silence-flush-value"></span>
    <input id="continuous-mic-hard-cut" type="range" min="15000" max="120000" value="45000" />
    <span id="continuous-mic-hard-cut-value"></span>

    <button id="gpu-backend-cuda"></button>
    <button id="gpu-backend-vulkan"></button>
    <button id="gpu-purge-btn"></button>
    <span id="gpu-vram">123 MB</span>
  `;
});

import { wireTranscription } from "../wiring/transcription.wire";
import * as dom from "../dom-refs";
import { settings, setSettings } from "../state";
import type { Settings } from "../types";

const mockedInvoke = vi.mocked(invoke);

function freshSettings(): Settings {
  return {
    capture_enabled: false,
    transcribe_enabled: false,
    mode: "ptt",
    input_device: "mic-1",
    transcribe_output_device: "default",
    transcribe_vad_mode: false,
    transcribe_vad_threshold: 0.5,
    transcribe_vad_silence_ms: 1200,
    transcribe_batch_interval_ms: 10000,
    transcribe_chunk_overlap_ms: 300,
    transcribe_input_gain_db: 0,
    audio_cues: false,
    audio_cues_volume: 0.5,
    ptt_use_vad: false,
    hallucination_filter_enabled: false,
    activation_words_enabled: false,
    activation_words: [],
    opus_enabled: false,
    opus_bitrate_kbps: 24,
    auto_save_system_audio: false,
    auto_save_mic_audio: false,
    continuous_dump_enabled: true,
    continuous_dump_profile: "balanced",
    continuous_soft_flush_ms: 10000,
    continuous_silence_flush_ms: 1200,
    continuous_hard_cut_ms: 45000,
    continuous_min_chunk_ms: 1000,
    continuous_pre_roll_ms: 300,
    continuous_post_roll_ms: 200,
    continuous_idle_keepalive_ms: 60000,
    continuous_system_override_enabled: false,
    continuous_system_soft_flush_ms: 10000,
    continuous_system_silence_flush_ms: 1200,
    continuous_system_hard_cut_ms: 45000,
    continuous_mic_override_enabled: false,
    continuous_mic_soft_flush_ms: 10000,
    continuous_mic_silence_flush_ms: 1200,
    continuous_mic_hard_cut_ms: 45000,
    local_backend_preference: "auto",
    providers: {
      ollama: {
        available_models: [],
        preferred_model: "",
        endpoint: "",
        auth_status: "locked",
        auth_verified_at: null,
        api_key_stored: false,
        auth_method_preference: "api_key",
        runtime_target_version: "0.20.2",
      },
    },
    ai_fallback: {
      enabled: false,
      provider: "ollama",
      fallback_provider: null,
      execution_mode: "local_first",
      strict_local_mode: false,
      preserve_source_language: true,
      low_latency_mode: false,
      model: "",
      prompt_profile: "balanced",
      prompt_presets: [],
      active_prompt_preset_id: null,
    },
  } as unknown as Settings;
}

let wired = false;
function wireOnce() {
  if (wired) return;
  wireTranscription();
  wired = true;
}

beforeEach(() => {
  setSettings(freshSettings());
  mockedInvoke.mockReset();
  mockedInvoke.mockResolvedValue(undefined);
  if (dom.captureEnabledToggle) dom.captureEnabledToggle.checked = false;
  if (dom.transcribeEnabledToggle) dom.transcribeEnabledToggle.checked = false;
  if (dom.modeSelect) dom.modeSelect.value = "ptt";
  if (dom.deviceSelect) dom.deviceSelect.value = "mic-1";
  if (dom.transcribeDeviceSelect) dom.transcribeDeviceSelect.value = "default";
  if (dom.transcribeVadToggle) dom.transcribeVadToggle.checked = false;
  if (dom.transcribeVadThreshold) dom.transcribeVadThreshold.value = "-35";
  if (dom.transcribeVadSilence) dom.transcribeVadSilence.value = "1200";
  if (dom.transcribeBatchInterval) dom.transcribeBatchInterval.value = "10000";
  if (dom.transcribeChunkOverlap) dom.transcribeChunkOverlap.value = "300";
  if (dom.transcribeGain) dom.transcribeGain.value = "0";
  if (dom.audioCuesToggle) dom.audioCuesToggle.checked = false;
  if (dom.pttUseVadToggle) dom.pttUseVadToggle.checked = false;
  if (dom.audioCuesVolume) dom.audioCuesVolume.value = "50";
  if (dom.hallucinationFilterToggle) dom.hallucinationFilterToggle.checked = false;
  if (dom.activationWordsToggle) dom.activationWordsToggle.checked = false;
  if (dom.activationWordsList) dom.activationWordsList.value = "";
  if (dom.opusEnabledToggle) dom.opusEnabledToggle.checked = false;
  if (dom.opusArchiveToggle) dom.opusArchiveToggle.checked = false;
  if (dom.opusBitrateSelect) dom.opusBitrateSelect.value = "24";
  if (dom.autoSaveSystemAudioToggle) dom.autoSaveSystemAudioToggle.checked = false;
  if (dom.autoSaveMicAudioToggle) dom.autoSaveMicAudioToggle.checked = false;
  if (dom.continuousDumpEnabledToggle) dom.continuousDumpEnabledToggle.checked = true;
  if (dom.continuousDumpProfile) dom.continuousDumpProfile.value = "balanced";
  if (dom.continuousHardCut) dom.continuousHardCut.value = "45000";
  if (dom.continuousMinChunk) dom.continuousMinChunk.value = "1000";
  if (dom.continuousPreRoll) dom.continuousPreRoll.value = "300";
  if (dom.continuousPostRoll) dom.continuousPostRoll.value = "200";
  if (dom.continuousKeepalive) dom.continuousKeepalive.value = "60000";
  if (dom.continuousSystemOverrideToggle) dom.continuousSystemOverrideToggle.checked = false;
  if (dom.continuousSystemSoftFlush) dom.continuousSystemSoftFlush.value = "10000";
  if (dom.continuousSystemSilenceFlush) dom.continuousSystemSilenceFlush.value = "1200";
  if (dom.continuousSystemHardCut) dom.continuousSystemHardCut.value = "45000";
  if (dom.continuousMicOverrideToggle) dom.continuousMicOverrideToggle.checked = false;
  if (dom.continuousMicSoftFlush) dom.continuousMicSoftFlush.value = "10000";
  if (dom.continuousMicSilenceFlush) dom.continuousMicSilenceFlush.value = "1200";
  if (dom.continuousMicHardCut) dom.continuousMicHardCut.value = "45000";
  if (dom.gpuVramLabel) dom.gpuVramLabel.textContent = "123 MB";
  wireOnce();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function fire(el: HTMLElement | null, type: string) {
  el?.dispatchEvent(new Event(type, { bubbles: true }));
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("wireTranscription - top-level capture controls", () => {
  it("capture toggle updates settings", async () => {
    if (dom.captureEnabledToggle) dom.captureEnabledToggle.checked = true;
    fire(dom.captureEnabledToggle, "change");
    await flush();
    expect(settings!.capture_enabled).toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("transcribe toggle updates settings", async () => {
    if (dom.transcribeEnabledToggle) dom.transcribeEnabledToggle.checked = true;
    fire(dom.transcribeEnabledToggle, "change");
    await flush();
    expect(settings!.transcribe_enabled).toBe(true);
  });

  it("capture mode select updates mode", async () => {
    if (dom.modeSelect) dom.modeSelect.value = "vad";
    fire(dom.modeSelect, "change");
    await flush();
    expect(settings!.mode).toBe("vad");
  });
});

describe("wireTranscription - hotkeys and devices", () => {
  it("ptt hotkey record button enters recording state", () => {
    dom.pttHotkeyRecord?.click();
    expect(dom.pttHotkeyRecord?.classList.contains("recording")).toBe(true);
    expect(dom.pttHotkeyStatus?.textContent).toContain("Press your key combination");
  });

  it("toggle activation words hotkey record button enters recording state", () => {
    dom.toggleActivationWordsHotkeyRecord?.click();
    expect(dom.toggleActivationWordsHotkeyRecord?.classList.contains("recording")).toBe(true);
  });

  it("input device select updates settings", async () => {
    if (dom.deviceSelect) dom.deviceSelect.value = "mic-2";
    fire(dom.deviceSelect, "change");
    await flush();
    expect(settings!.input_device).toBe("mic-2");
  });

  it("transcribe output device select updates settings", async () => {
    if (dom.transcribeDeviceSelect) dom.transcribeDeviceSelect.value = "speakers";
    fire(dom.transcribeDeviceSelect, "change");
    await flush();
    expect(settings!.transcribe_output_device).toBe("speakers");
  });
});

describe("wireTranscription - vad and gain", () => {
  it("vad toggle enables vad mode and disables batch controls", async () => {
    if (dom.transcribeVadToggle) dom.transcribeVadToggle.checked = true;
    fire(dom.transcribeVadToggle, "change");
    await flush();
    expect(settings!.transcribe_vad_mode).toBe(true);
    expect(dom.transcribeBatchField?.classList.contains("is-disabled")).toBe(true);
    expect(dom.transcribeVadThresholdField?.classList.contains("is-disabled")).toBe(false);
  });

  it("vad threshold input writes normalized threshold and label", () => {
    if (dom.transcribeVadThreshold) dom.transcribeVadThreshold.value = "-20";
    fire(dom.transcribeVadThreshold, "input");
    expect(settings!.transcribe_vad_threshold).toBeGreaterThan(0);
    expect(dom.transcribeVadThresholdValue?.textContent).toBe("-20 dB");
  });

  it("vad silence input clamps and mirrors continuous system silence", () => {
    if (dom.transcribeVadSilence) dom.transcribeVadSilence.value = "50";
    fire(dom.transcribeVadSilence, "input");
    expect(settings!.transcribe_vad_silence_ms).toBe(200);
    expect(settings!.continuous_system_silence_flush_ms).toBe(200);
  });

  it("batch interval input clamps and mirrors continuous system soft flush", () => {
    if (dom.transcribeBatchInterval) dom.transcribeBatchInterval.value = "20000";
    fire(dom.transcribeBatchInterval, "input");
    expect(settings!.transcribe_batch_interval_ms).toBe(15000);
    expect(settings!.continuous_system_soft_flush_ms).toBe(15000);
  });

  it("chunk overlap input does not exceed half the batch interval", () => {
    settings!.transcribe_batch_interval_ms = 2000;
    if (dom.transcribeChunkOverlap) dom.transcribeChunkOverlap.value = "3000";
    fire(dom.transcribeChunkOverlap, "input");
    expect(settings!.transcribe_chunk_overlap_ms).toBe(1000);
  });

  it("gain input clamps to +30 dB", () => {
    if (dom.transcribeGain) dom.transcribeGain.value = "100";
    fire(dom.transcribeGain, "input");
    expect(settings!.transcribe_input_gain_db).toBe(30);
    expect(dom.transcribeGainValue?.textContent).toBe("+30 dB");
  });
});

describe("wireTranscription - cues, activation, quality", () => {
  it("audio cues toggle persists", async () => {
    if (dom.audioCuesToggle) dom.audioCuesToggle.checked = true;
    fire(dom.audioCuesToggle, "change");
    await flush();
    expect(settings!.audio_cues).toBe(true);
  });

  it("ptt-use-vad toggle persists", async () => {
    if (dom.pttUseVadToggle) dom.pttUseVadToggle.checked = true;
    fire(dom.pttUseVadToggle, "change");
    await flush();
    expect(settings!.ptt_use_vad).toBe(true);
  });

  it("audio cues volume input writes normalized percent", () => {
    if (dom.audioCuesVolume) dom.audioCuesVolume.value = "75";
    fire(dom.audioCuesVolume, "input");
    expect(settings!.audio_cues_volume).toBe(0.75);
    expect(dom.audioCuesVolumeValue?.textContent).toBe("75%");
  });

  it("hallucination filter toggle persists", async () => {
    if (dom.hallucinationFilterToggle) dom.hallucinationFilterToggle.checked = true;
    fire(dom.hallucinationFilterToggle, "change");
    await flush();
    expect(settings!.hallucination_filter_enabled).toBe(true);
  });

  it("activation words toggle persists", async () => {
    if (dom.activationWordsToggle) dom.activationWordsToggle.checked = true;
    fire(dom.activationWordsToggle, "change");
    await flush();
    expect(settings!.activation_words_enabled).toBe(true);
  });

  it("activation words list trims blank lines", async () => {
    if (dom.activationWordsList) dom.activationWordsList.value = " alpha \n\n beta ";
    fire(dom.activationWordsList, "change");
    await flush();
    expect(settings!.activation_words).toEqual(["alpha", "beta"]);
  });

  it("opus enabled toggle syncs archive toggle", async () => {
    if (dom.opusEnabledToggle) dom.opusEnabledToggle.checked = true;
    fire(dom.opusEnabledToggle, "change");
    await flush();
    expect(settings!.opus_enabled).toBe(true);
    expect(dom.opusArchiveToggle?.checked).toBe(true);
  });

  it("opus bitrate select writes numeric bitrate", async () => {
    if (dom.opusBitrateSelect) dom.opusBitrateSelect.value = "48";
    fire(dom.opusBitrateSelect, "change");
    await flush();
    expect(settings!.opus_bitrate_kbps).toBe(48);
  });

  it("auto-save toggles update settings", async () => {
    if (dom.autoSaveSystemAudioToggle) dom.autoSaveSystemAudioToggle.checked = true;
    if (dom.autoSaveMicAudioToggle) dom.autoSaveMicAudioToggle.checked = true;
    fire(dom.autoSaveSystemAudioToggle, "change");
    fire(dom.autoSaveMicAudioToggle, "change");
    await flush();
    expect(settings!.auto_save_system_audio).toBe(true);
    expect(settings!.auto_save_mic_audio).toBe(true);
  });
});

describe("wireTranscription - continuous dump", () => {
  it("continuous dump toggle updates setting", async () => {
    if (dom.continuousDumpEnabledToggle) dom.continuousDumpEnabledToggle.checked = false;
    fire(dom.continuousDumpEnabledToggle, "change");
    await flush();
    expect(settings!.continuous_dump_enabled).toBe(false);
  });

  it("low-latency profile applies profile and derived transcribe values", async () => {
    if (dom.continuousDumpProfile) dom.continuousDumpProfile.value = "low_latency";
    fire(dom.continuousDumpProfile, "change");
    await flush();
    expect(settings!.continuous_soft_flush_ms).toBe(8000);
    expect(settings!.transcribe_batch_interval_ms).toBe(8000);
  });

  it("hard cut input mirrors both channel hard cuts when overrides are disabled", () => {
    if (dom.continuousHardCut) dom.continuousHardCut.value = "60000";
    fire(dom.continuousHardCut, "input");
    expect(settings!.continuous_hard_cut_ms).toBe(60000);
    expect(settings!.continuous_system_hard_cut_ms).toBe(60000);
    expect(settings!.continuous_mic_hard_cut_ms).toBe(60000);
  });

  it("pre-roll input mirrors transcribe chunk overlap", () => {
    if (dom.continuousPreRoll) dom.continuousPreRoll.value = "750";
    fire(dom.continuousPreRoll, "input");
    expect(settings!.continuous_pre_roll_ms).toBe(750);
    expect(settings!.transcribe_chunk_overlap_ms).toBe(750);
  });

  it("system soft flush input mirrors transcribe batch interval", () => {
    if (dom.continuousSystemSoftFlush) dom.continuousSystemSoftFlush.value = "18000";
    fire(dom.continuousSystemSoftFlush, "input");
    expect(settings!.continuous_system_soft_flush_ms).toBe(18000);
    expect(settings!.transcribe_batch_interval_ms).toBe(15000);
  });

  it("mic override toggle copies base values back when disabled", async () => {
    settings!.continuous_mic_override_enabled = true;
    settings!.continuous_soft_flush_ms = 11000;
    if (dom.continuousMicOverrideToggle) dom.continuousMicOverrideToggle.checked = false;
    fire(dom.continuousMicOverrideToggle, "change");
    await flush();
    expect(settings!.continuous_mic_override_enabled).toBe(false);
    expect(settings!.continuous_mic_soft_flush_ms).toBe(11000);
  });
});

describe("wireTranscription - whisper backend gpu controls", () => {
  it("cuda backend button updates preference and shows toast", async () => {
    dom.gpuBackendCudaBtn?.click();
    await flush();
    expect(settings!.local_backend_preference).toBe("cuda");
    expect(document.querySelector("#toast-container")?.textContent).toContain("Backend updated");
  });

  it("vulkan backend button updates preference", async () => {
    dom.gpuBackendVulkanBtn?.click();
    await flush();
    expect(settings!.local_backend_preference).toBe("vulkan");
  });

  it("gpu purge button calls backend and restores label after timeout", async () => {
    vi.useFakeTimers();
    dom.gpuPurgeBtn?.click();
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("purge_gpu_memory");
    expect(dom.gpuVramLabel?.textContent).toBe("Purged ✓");
    vi.advanceTimersByTime(2000);
    expect(dom.gpuVramLabel?.textContent).toBe("123 MB");
  });
});
