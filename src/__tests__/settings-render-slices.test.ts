import { beforeEach, describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
  document.body.innerHTML = `
    <input id="continuous-dump-enabled-toggle" type="checkbox" />
    <select id="continuous-dump-profile"><option value="balanced">balanced</option><option value="low_latency">low_latency</option></select>
    <input id="continuous-hard-cut" />
    <span id="continuous-hard-cut-value"></span>
    <input id="continuous-min-chunk" />
    <span id="continuous-min-chunk-value"></span>
    <input id="continuous-pre-roll" />
    <span id="continuous-pre-roll-value"></span>
    <input id="continuous-post-roll" />
    <span id="continuous-post-roll-value"></span>
    <input id="continuous-keepalive" />
    <span id="continuous-keepalive-value"></span>
    <input id="continuous-system-override-toggle" type="checkbox" />
    <input id="continuous-system-soft-flush" />
    <span id="continuous-system-soft-flush-value"></span>
    <input id="continuous-system-silence-flush" />
    <span id="continuous-system-silence-flush-value"></span>
    <input id="continuous-system-hard-cut" />
    <span id="continuous-system-hard-cut-value"></span>
    <input id="continuous-mic-override-toggle" type="checkbox" />
    <input id="continuous-mic-soft-flush" />
    <span id="continuous-mic-soft-flush-value"></span>
    <input id="continuous-mic-silence-flush" />
    <span id="continuous-mic-silence-flush-value"></span>
    <input id="continuous-mic-hard-cut" />
    <span id="continuous-mic-hard-cut-value"></span>

    <input id="opus-enabled-toggle" type="checkbox" />
    <input id="opus-archive-toggle" type="checkbox" />
    <select id="opus-bitrate-select"><option value="64">64</option><option value="96">96</option></select>
    <input id="auto-save-system-audio-toggle" type="checkbox" />
    <input id="auto-save-mic-audio-toggle" type="checkbox" />

    <input id="postproc-enabled" type="checkbox" />
    <div id="postproc-settings"></div>
    <span id="postproc-language-derived"></span>
    <input id="postproc-punctuation" type="checkbox" />
    <input id="postproc-capitalization" type="checkbox" />
    <input id="postproc-numbers" type="checkbox" />
    <input id="postproc-custom-vocab-enabled" type="checkbox" />
    <div id="postproc-custom-vocab-config"></div>
    <div id="postproc-vocab-rows"></div>
    <span id="vocab-terms-count"></span>
    <div id="vocab-terms-list"></div>
    <div id="vocab-observing-list"></div>
  `;
});

import {
  ensureContinuousDumpDefaults,
  renderContinuousDumpSettings,
} from "../settings/continuous-dump.settings";
import { renderPostProcessingSettings } from "../settings/post-processing.settings";
import { renderRecordingQualitySettings } from "../settings/recording-quality.settings";
import { setSettings } from "../state";
import type { Settings } from "../types";

function byId<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    continuous_dump_enabled: true,
    continuous_dump_profile: "low_latency",
    continuous_hard_cut_ms: 30000,
    continuous_min_chunk_ms: 800,
    continuous_pre_roll_ms: 200,
    continuous_post_roll_ms: 150,
    continuous_idle_keepalive_ms: 45000,
    continuous_system_override_enabled: true,
    continuous_system_soft_flush_ms: 8000,
    continuous_system_silence_flush_ms: 900,
    continuous_system_hard_cut_ms: 30000,
    continuous_mic_override_enabled: true,
    continuous_mic_soft_flush_ms: 9000,
    continuous_mic_silence_flush_ms: 1100,
    continuous_mic_hard_cut_ms: 35000,
    opus_enabled: false,
    opus_bitrate_kbps: 96,
    auto_save_system_audio: true,
    auto_save_mic_audio: true,
    postproc_enabled: true,
    postproc_language: "de",
    postproc_punctuation_enabled: true,
    postproc_capitalization_enabled: false,
    postproc_numbers_enabled: true,
    postproc_custom_vocab_enabled: true,
    postproc_custom_vocab: { teh: "the" },
    learned_vocabulary: [],
    ...overrides,
  } as unknown as Settings;
}

beforeEach(() => {
  setSettings(makeSettings());
});

describe("renderContinuousDumpSettings", () => {
  it("renders continuous dump controls and value labels", () => {
    renderContinuousDumpSettings();
    expect(byId<HTMLInputElement>("continuous-dump-enabled-toggle").checked).toBe(true);
    expect(byId<HTMLSelectElement>("continuous-dump-profile").value).toBe("low_latency");
    expect(byId<HTMLInputElement>("continuous-hard-cut").value).toBe("30000");
    expect(byId("continuous-hard-cut-value").textContent).toBe("30s");
    expect(byId("continuous-min-chunk-value").textContent).toBe("0.8s");
    expect(byId("continuous-pre-roll-value").textContent).toBe("0.20s");
    expect(byId("continuous-system-silence-flush-value").textContent).toBe("0.9s");
    expect(byId("continuous-mic-hard-cut-value").textContent).toBe("35s");
  });

  it("fills missing continuous dump defaults", () => {
    setSettings({} as Settings);
    ensureContinuousDumpDefaults();
    renderContinuousDumpSettings();
    expect(byId<HTMLSelectElement>("continuous-dump-profile").value).toBe("balanced");
    expect(byId("continuous-hard-cut-value").textContent).toBe("45s");
  });
});

describe("renderRecordingQualitySettings", () => {
  it("renders opus and auto-save controls", () => {
    renderRecordingQualitySettings();
    expect(byId<HTMLInputElement>("opus-enabled-toggle").checked).toBe(false);
    expect(byId<HTMLInputElement>("opus-archive-toggle").checked).toBe(false);
    expect(byId<HTMLSelectElement>("opus-bitrate-select").value).toBe("96");
    expect(byId<HTMLInputElement>("auto-save-system-audio-toggle").checked).toBe(true);
    expect(byId<HTMLInputElement>("auto-save-mic-audio-toggle").checked).toBe(true);
  });
});

describe("renderPostProcessingSettings", () => {
  it("renders post-processing controls and vocabulary", () => {
    renderPostProcessingSettings();
    expect(byId<HTMLInputElement>("postproc-enabled").checked).toBe(true);
    expect(byId<HTMLDivElement>("postproc-settings").style.display).toBe("grid");
    expect(byId("postproc-language-derived").textContent).toContain("German");
    expect(byId<HTMLInputElement>("postproc-punctuation").checked).toBe(true);
    expect(byId<HTMLInputElement>("postproc-capitalization").checked).toBe(false);
    expect(byId<HTMLInputElement>("postproc-numbers").checked).toBe(true);
    expect(byId<HTMLDivElement>("postproc-custom-vocab-config").style.display).toBe("block");
    expect(document.querySelectorAll("#postproc-vocab-rows .vocab-row")).toHaveLength(1);
  });
});