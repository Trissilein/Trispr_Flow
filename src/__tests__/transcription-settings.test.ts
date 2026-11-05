import { beforeEach, describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
    document.body.innerHTML = `
    <input id="capture-enabled-toggle" type="checkbox" />
    <select id="mode-select"><option value="ptt">ptt</option><option value="vad">vad</option></select>
    <input id="ptt-hotkey" />
    <input id="toggle-hotkey" />
    <div id="hotkeys-block"></div>
    <div id="vad-block"></div>
    <div id="vad-silence-field-wrapper" class="field"><input id="vad-silence" /></div>
    <select id="device-select"><option value="default">default</option><option value="mic-1">mic-1</option></select>
    <div id="asr-language-field"></div>
    <select id="language-select"><option value="auto">auto</option><option value="en">en</option><option value="de">de</option></select>
    <input id="language-pinned-toggle" type="checkbox" />
    <div id="asr-language-hint-note"></div>
    <select id="whisper-input-language-select"><option value="auto">auto</option><option value="en">en</option><option value="de">de</option></select>
    <div id="whisper-input-language-note"></div>
    <select id="model-source-select"><option value="bundled">bundled</option><option value="custom">custom</option></select>
    <input id="model-custom-url" />
    <input id="model-storage-path" />
    <div id="model-custom-url-field"></div>
    <input id="audio-cues-toggle" type="checkbox" />
    <input id="ptt-use-vad-toggle" type="checkbox" />
    <input id="audio-cues-volume" type="range" />
    <span id="audio-cues-volume-value"></span>
    <input id="hallucination-filter-toggle" type="checkbox" />
    <input id="activation-words-toggle" type="checkbox" />
    <textarea id="activation-words-list"></textarea>
    <div id="activation-words-config"></div>
    <input id="mic-gain" type="range" />
    <span id="mic-gain-value"></span>
    <input id="vad-threshold" type="range" min="-60" max="0" />
    <span id="vad-threshold-value"></span>
    <span id="vad-silence-value"></span>
    <input id="toggle-activation-words-hotkey" />
      <option value="default" selected>default</option>
      <option value="loopback-1">loopback-1</option>
    </select>
  `;
});

import {
    renderTranscriptionSettings,
    syncCaptureModeVisibility,
} from "../settings/transcription.settings";
import { setSettings } from "../state";
import type { Settings } from "../types";

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function makeSettings(overrides: Partial<Settings> = {}): Settings {
    return {
        capture_enabled: true,
        mode: "ptt",
        hotkey_ptt: "CommandOrControl+Shift+Space",
        hotkey_toggle: "CommandOrControl+Shift+T",
        input_device: "mic-1",
        language_mode: "en",
        language_pinned: true,
        model_source: "bundled",
        model_custom_url: "https://example.com/model.bin",
        model_storage_dir: "C:/models",
        audio_cues: true,
        ptt_use_vad: false,
        audio_cues_volume: 0.5,
        hallucination_filter_enabled: true,
        activation_words_enabled: true,
        activation_words: ["go", "flow"],
        mic_input_gain_db: 4,
        vad_threshold_start: 0.5,
        vad_silence_ms: 1200,
        hotkey_toggle_activation_words: "CommandOrControl+Shift+A",
        ...overrides,
    } as unknown as Settings;
}

beforeEach(() => {
    setSettings(makeSettings());
});

describe("syncCaptureModeVisibility", () => {
    it("shows hotkeys and hides vad block in ptt mode", () => {
        syncCaptureModeVisibility("ptt", false);
        expect(byId<HTMLDivElement>("hotkeys-block").classList.contains("hidden")).toBe(false);
        expect(byId<HTMLDivElement>("vad-block").classList.contains("hidden")).toBe(true);
    });

    it("shows vad block and hides hotkeys in vad mode", () => {
        syncCaptureModeVisibility("vad", false);
        expect(byId<HTMLDivElement>("hotkeys-block").classList.contains("hidden")).toBe(true);
        expect(byId<HTMLDivElement>("vad-block").classList.contains("hidden")).toBe(false);
    });

    it("shows vad block in ptt mode when pttUseVad is true", () => {
        syncCaptureModeVisibility("ptt", true);
        expect(byId<HTMLDivElement>("vad-block").classList.contains("hidden")).toBe(false);
    });

    it("hides vad silence field in ptt mode", () => {
        syncCaptureModeVisibility("ptt", false);
        expect(byId<HTMLDivElement>("vad-silence-field-wrapper").classList.contains("hidden")).toBe(true);
    });
});

describe("renderTranscriptionSettings", () => {
    it("renders core transcription controls", () => {
        renderTranscriptionSettings();
        expect(byId<HTMLInputElement>("capture-enabled-toggle").checked).toBe(true);
        expect(byId<HTMLSelectElement>("mode-select").value).toBe("ptt");
        expect(byId<HTMLSelectElement>("language-select").value).toBe("en");
        expect(byId<HTMLInputElement>("ptt-hotkey").value).toContain("CommandOrControl");
        expect(byId<HTMLInputElement>("toggle-hotkey").value).toContain("CommandOrControl");
    });

    it("is a no-op when settings is null", () => {
        setSettings(null);
        byId<HTMLInputElement>("ptt-hotkey").value = "unchanged";
        renderTranscriptionSettings();
        expect(byId<HTMLInputElement>("ptt-hotkey").value).toBe("unchanged");
    });
}
);