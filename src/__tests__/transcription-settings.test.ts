import { beforeEach, describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
  document.body.innerHTML = `
    <input id="capture-enabled-toggle" type="checkbox" />
    <input id="transcribe-enabled-toggle" type="checkbox" />
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
    <input id="transcribe-hotkey" />
    <input id="toggle-activation-words-hotkey" />
    <input id="product-mode-hotkey" />
    <input id="tts-stop-hotkey" />
    <select id="transcribe-device-select">
      <option value="default" selected>default</option>
      <option value="loopback-1">loopback-1</option>
    </select>
    <input id="transcribe-vad-toggle" type="checkbox" />
    <input id="transcribe-vad-threshold" type="range" min="-60" max="0" />
    <span id="transcribe-vad-threshold-value"></span>
    <input id="transcribe-vad-silence" type="range" />
    <span id="transcribe-vad-silence-value"></span>
    <span id="transcribe-threshold-db"></span>
    <div id="transcribe-meter-threshold"></div>
    <div id="transcribe-threshold-label"></div>
    <input id="transcribe-batch-interval" type="range" min="1000" max="10000" />
    <span id="transcribe-batch-value"></span>
    <input id="transcribe-chunk-overlap" type="range" min="0" max="5000" />
    <span id="transcribe-overlap-value"></span>
    <input id="transcribe-gain" type="range" />
    <span id="transcribe-gain-value"></span>
    <div id="transcribe-batch-field"></div>
    <div id="transcribe-overlap-field"></div>
    <div id="transcribe-vad-threshold-field"></div>
    <div id="transcribe-vad-silence-field"></div>
  `;
});

import {
  derivePostprocLanguageFromAsr,
  renderTranscriptionSettings,
  resolveEffectiveAsrLanguageHint,
  syncCaptureModeVisibility,
  updateTranscribeThreshold,
  updateTranscribeVadVisibility,
} from "../settings/transcription.settings";
import { setSettings, settings } from "../state";
import type { Settings } from "../types";

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    capture_enabled: true,
    transcribe_enabled: true,
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
    transcribe_hotkey: "CommandOrControl+Shift+Enter",
    hotkey_toggle_activation_words: "CommandOrControl+Shift+A",
    hotkey_product_mode_toggle: "CommandOrControl+Shift+P",
    hotkey_tts_stop: "CommandOrControl+Shift+F12",
    transcribe_output_device: "loopback-1",
    transcribe_vad_mode: true,
    transcribe_vad_threshold: 0.6,
    transcribe_vad_silence_ms: 950,
    transcribe_batch_interval_ms: 4000,
    transcribe_chunk_overlap_ms: 1200,
    transcribe_input_gain_db: 2,
    ...overrides,
  } as unknown as Settings;
}

beforeEach(() => {
  setSettings(makeSettings());
});

describe("resolveEffectiveAsrLanguageHint", () => {
  it("returns pinned language when pinned", () => {
    expect(resolveEffectiveAsrLanguageHint("en", true)).toBe("en");
  });

  it("returns auto when unpinned", () => {
    expect(resolveEffectiveAsrLanguageHint("de", false)).toBe("auto");
  });

  it("normalizes uppercase language code", () => {
    expect(resolveEffectiveAsrLanguageHint("EN", true)).toBe("en");
  });

  it("defaults empty language to auto", () => {
    expect(resolveEffectiveAsrLanguageHint("", true)).toBe("auto");
  });
});

describe("derivePostprocLanguageFromAsr", () => {
  it("returns multi when language is not pinned", () => {
    expect(derivePostprocLanguageFromAsr("en", false)).toBe("multi");
  });

  it("returns en when pinned to English", () => {
    expect(derivePostprocLanguageFromAsr("en", true)).toBe("en");
  });

  it("returns de when pinned to German", () => {
    expect(derivePostprocLanguageFromAsr("de", true)).toBe("de");
  });

  it("returns multi for non en/de value", () => {
    expect(derivePostprocLanguageFromAsr("fr", true)).toBe("multi");
  });

  it("normalizes uppercase language code", () => {
    expect(derivePostprocLanguageFromAsr("DE", true)).toBe("de");
  });
});

describe("updateTranscribeVadVisibility", () => {
  it("shows threshold meter and label when enabled", () => {
    updateTranscribeVadVisibility(true);
    expect(byId<HTMLDivElement>("transcribe-meter-threshold").style.display).toBe("block");
    expect(byId<HTMLDivElement>("transcribe-threshold-label").style.display).toBe("block");
  });

  it("hides threshold meter and label when disabled", () => {
    updateTranscribeVadVisibility(false);
    expect(byId<HTMLDivElement>("transcribe-meter-threshold").style.display).toBe("none");
    expect(byId<HTMLDivElement>("transcribe-threshold-label").style.display).toBe("none");
  });
});

describe("updateTranscribeThreshold", () => {
  it("renders threshold db text", () => {
    updateTranscribeThreshold(0.5);
    expect(byId<HTMLElement>("transcribe-threshold-db").textContent).toBe("-6.0 dB");
  });

  it("updates threshold marker position", () => {
    updateTranscribeThreshold(1.0);
    expect(byId<HTMLDivElement>("transcribe-meter-threshold").style.left).toBe("100%");
  });
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

  it("renders transcribe VAD and timing controls", () => {
    renderTranscriptionSettings();
    expect(byId<HTMLInputElement>("transcribe-vad-toggle").checked).toBe(true);
    expect(byId<HTMLInputElement>("transcribe-vad-threshold").value).toBe("-4");
    expect(byId<HTMLElement>("transcribe-vad-threshold-value").textContent).toBe("-4 dB");
    expect(byId<HTMLInputElement>("transcribe-batch-interval").value).toBe("4000");
    expect(byId<HTMLElement>("transcribe-batch-value").textContent).toBe("4s");
  });

  it("syncs missing transcribe output device back to default", () => {
    setSettings(makeSettings({ transcribe_output_device: "missing-device" }));
    renderTranscriptionSettings();
    expect(settings?.transcribe_output_device).toBe(byId<HTMLSelectElement>("transcribe-device-select").value);
    expect(byId<HTMLInputElement>("transcribe-vad-toggle").checked).toBe(true);
  });

  it("is a no-op when settings is null", () => {
    setSettings(null);
    byId<HTMLInputElement>("ptt-hotkey").value = "unchanged";
    renderTranscriptionSettings();
    expect(byId<HTMLInputElement>("ptt-hotkey").value).toBe("unchanged");
  });
}
);