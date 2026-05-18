/**
 * Smoke tests for wireVoiceOutput() — R2 slice 3.
 *
 * Integration-style per OQ-3: build the DOM, call wireVoiceOutput(),
 * dispatch real events, assert observable state. The settings.ts helpers
 * (refreshProviderAvailability, refreshProviderVoices,
 * handleProviderVoiceSelection) run for real; the only mock is the Tauri
 * `invoke` boundary (already globally mocked via tauri.setup.ts) — we
 * configure it per-command to return sensible empties.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

// Inject DOM nodes before module imports so dom-refs.ts captures them.
vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>

    <!-- Provider chain -->
    <select id="voice-output-default-provider">
      <option value="windows_native">windows_native</option>
      <option value="windows_natural">windows_natural</option>
      <option value="local_custom">local_custom</option>
      <option value="qwen3_tts" selected>qwen3_tts</option>
    </select>
    <select id="voice-output-fallback-provider">
      <option value="windows_native" selected>windows_native</option>
      <option value="windows_natural">windows_natural</option>
      <option value="local_custom">local_custom</option>
      <option value="qwen3_tts">qwen3_tts</option>
    </select>
    <select id="voice-output-policy">
      <option value="agent_replies_only" selected>agent_replies_only</option>
      <option value="replies_and_events">replies_and_events</option>
      <option value="explicit_only">explicit_only</option>
    </select>

    <!-- Output device -->
    <select id="voice-output-device-select">
      <option value="default" selected>default</option>
      <option value="speakers">speakers</option>
    </select>

    <!-- Voice selectors + auto-language -->
    <div class="field" id="voice-output-windows-voice-field">
      <select id="voice-output-windows-voice-select"></select>
      <span class="field-hint" id="voice-output-windows-voice-hint"></span>
    </div>
    <div class="field" id="voice-output-fallback-voice-field">
      <select id="voice-output-fallback-voice-select"></select>
      <span class="field-hint" id="voice-output-fallback-voice-hint"></span>
    </div>
    <div class="field toggle" id="voice-output-auto-language-voice-field">
      <input id="voice-output-auto-language-voice" type="checkbox" />
    </div>

    <!-- Sliders -->
    <input id="voice-output-rate" type="range" min="0.5" max="2.0" step="0.05" value="1.0" />
    <span id="voice-output-rate-value">1.00</span>
    <input id="voice-output-volume" type="range" min="0" max="1" step="0.05" value="1.0" />
    <span id="voice-output-volume-value">1.00</span>

    <!-- Test button -->
    <button id="voice-output-test-btn" type="button"></button>
    <span id="voice-output-test-status"></span>

    <!-- Piper gain + paths -->
    <input id="voice-output-piper-gain-db" type="range" min="-24" max="6" step="1" value="-12" />
    <span id="voice-output-piper-gain-db-value">-12 dB</span>
    <input id="voice-output-piper-binary" type="text" value="" />
    <input id="voice-output-piper-model" type="text" value="" />
    <input id="voice-output-piper-model-dir" type="text" value="" />

    <!-- Qwen3 fields -->
    <input id="voice-output-qwen-endpoint" type="text" value="" />
    <input id="voice-output-qwen-model" type="text" value="" />
    <input id="voice-output-qwen-voice" type="text" value="" />
    <input id="voice-output-qwen-api-key" type="password" value="" />
    <input id="voice-output-qwen-timeout-sec" type="number" min="3" max="180" step="1" value="45" />
  `;
});

import { wireVoiceOutput } from "../wiring/voice-output.wire";
import * as dom from "../dom-refs";
import { settings, setSettings } from "../state";
import type { Settings, TtsSpeakResult } from "../types";

const mockedInvoke = vi.mocked(invoke);

function freshSettings(): Settings {
  return {
    voice_output_settings: {
      default_provider: "qwen3_tts",
      fallback_provider: "windows_native",
      output_policy: "agent_replies_only",
      output_device: "default",
      auto_voice_by_detected_language: false,
      rate: 1.0,
      volume: 1.0,
      piper_gain_db: -12,
      piper_binary_path: "",
      piper_model_path: "",
      piper_model_dir: "",
      qwen3_tts_endpoint: "",
      qwen3_tts_model: "",
      qwen3_tts_voice: "",
      qwen3_tts_api_key: "",
      qwen3_tts_timeout_sec: 45,
    },
  } as unknown as Settings;
}

/** Per-command invoke mock: returns empty arrays for the data-fetch commands
 *  used by refresh helpers, and an undefined-shaped TtsSpeakResult for the
 *  test button. Tests can override `test_tts_provider` per case. */
function defaultInvokeImpl(cmd: string): unknown {
  switch (cmd) {
    case "list_tts_providers":
      // Return all four providers as available so refreshProviderAvailability
      // does not force-normalise the selection back to a default.
      return [
        { id: "windows_native", label: "Windows Native", available: true },
        { id: "windows_natural", label: "Windows Natural", available: true },
        { id: "local_custom", label: "Local (Piper)", available: true },
        { id: "qwen3_tts", label: "Qwen3 TTS", available: true },
      ];
    case "list_tts_voices":
    case "list_piper_voice_catalog":
      return [];
    case "test_tts_provider":
      return {
        provider_used: "qwen3_tts",
        accepted: true,
        message: "ok",
        used_fallback: false,
        preferred_provider: "qwen3_tts",
      } as TtsSpeakResult;
    default:
      return undefined;
  }
}

let wired = false;
function wireOnce() {
  if (wired) return;
  wireVoiceOutput();
  wired = true;
}

beforeEach(() => {
  setSettings(freshSettings());
  mockedInvoke.mockReset();
  mockedInvoke.mockImplementation(async (cmd: string) => defaultInvokeImpl(cmd));
  // Reset DOM control values
  if (dom.voiceOutputDefaultProvider) dom.voiceOutputDefaultProvider.value = "qwen3_tts";
  if (dom.voiceOutputFallbackProvider) dom.voiceOutputFallbackProvider.value = "windows_native";
  if (dom.voiceOutputPolicy) dom.voiceOutputPolicy.value = "agent_replies_only";
  if (dom.voiceOutputDeviceSelect) dom.voiceOutputDeviceSelect.value = "default";
  if (dom.voiceOutputAutoLanguageVoice) dom.voiceOutputAutoLanguageVoice.checked = false;
  if (dom.voiceOutputRate) dom.voiceOutputRate.value = "1.0";
  if (dom.voiceOutputVolume) dom.voiceOutputVolume.value = "1.0";
  if (dom.voiceOutputPiperGainDb) dom.voiceOutputPiperGainDb.value = "-12";
  if (dom.voiceOutputPiperBinary) dom.voiceOutputPiperBinary.value = "";
  if (dom.voiceOutputPiperModel) dom.voiceOutputPiperModel.value = "";
  if (dom.voiceOutputPiperModelDir) dom.voiceOutputPiperModelDir.value = "";
  if (dom.voiceOutputQwenEndpoint) dom.voiceOutputQwenEndpoint.value = "";
  if (dom.voiceOutputQwenModel) dom.voiceOutputQwenModel.value = "";
  if (dom.voiceOutputQwenVoice) dom.voiceOutputQwenVoice.value = "";
  if (dom.voiceOutputQwenApiKey) dom.voiceOutputQwenApiKey.value = "";
  if (dom.voiceOutputQwenTimeoutSec) dom.voiceOutputQwenTimeoutSec.value = "45";
  if (dom.voiceOutputTestStatus) {
    dom.voiceOutputTestStatus.textContent = "";
    dom.voiceOutputTestStatus.title = "";
  }
  wireOnce();
});

afterEach(() => {
  vi.restoreAllMocks();
});

function fire(el: HTMLElement | null, type: string) {
  el?.dispatchEvent(new Event(type, { bubbles: true }));
}

async function flush() {
  // Multiple microtask ticks for chains like change → refreshAvailability →
  // refreshVoices → persist.
  for (let i = 0; i < 6; i++) await Promise.resolve();
}

describe("wireVoiceOutput — provider chain", () => {
  it("default-provider change updates settings", async () => {
    if (dom.voiceOutputDefaultProvider) dom.voiceOutputDefaultProvider.value = "windows_native";
    fire(dom.voiceOutputDefaultProvider, "change");
    await flush();
    expect(settings!.voice_output_settings!.default_provider).toBe("windows_native");
  });

  it("default-provider change triggers refreshProviderAvailability via list_tts_providers", async () => {
    if (dom.voiceOutputDefaultProvider) dom.voiceOutputDefaultProvider.value = "windows_native";
    fire(dom.voiceOutputDefaultProvider, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("list_tts_providers");
  });

  it("default-provider change persists settings", async () => {
    if (dom.voiceOutputDefaultProvider) dom.voiceOutputDefaultProvider.value = "windows_native";
    fire(dom.voiceOutputDefaultProvider, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("fallback-provider change updates settings", async () => {
    if (dom.voiceOutputFallbackProvider) dom.voiceOutputFallbackProvider.value = "local_custom";
    fire(dom.voiceOutputFallbackProvider, "change");
    await flush();
    expect(settings!.voice_output_settings!.fallback_provider).toBe("local_custom");
  });

  it("fallback-provider change persists settings", async () => {
    if (dom.voiceOutputFallbackProvider) dom.voiceOutputFallbackProvider.value = "local_custom";
    fire(dom.voiceOutputFallbackProvider, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("policy change updates settings and persists", async () => {
    if (dom.voiceOutputPolicy) dom.voiceOutputPolicy.value = "replies_and_events";
    fire(dom.voiceOutputPolicy, "change");
    await flush();
    expect(settings!.voice_output_settings!.output_policy).toBe("replies_and_events");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — output device", () => {
  it("device change writes selected value", async () => {
    if (dom.voiceOutputDeviceSelect) dom.voiceOutputDeviceSelect.value = "speakers";
    fire(dom.voiceOutputDeviceSelect, "change");
    await flush();
    expect(settings!.voice_output_settings!.output_device).toBe("speakers");
  });

  it("device change falls back to 'default' on empty value", async () => {
    if (dom.voiceOutputDeviceSelect) dom.voiceOutputDeviceSelect.value = "";
    fire(dom.voiceOutputDeviceSelect, "change");
    await flush();
    expect(settings!.voice_output_settings!.output_device).toBe("default");
  });
});

describe("wireVoiceOutput — voice selectors", () => {
  it("auto-language toggle writes checked state", async () => {
    if (dom.voiceOutputAutoLanguageVoice) dom.voiceOutputAutoLanguageVoice.checked = true;
    fire(dom.voiceOutputAutoLanguageVoice, "change");
    await flush();
    expect(settings!.voice_output_settings!.auto_voice_by_detected_language).toBe(true);
  });

  it("auto-language toggle persists settings", async () => {
    if (dom.voiceOutputAutoLanguageVoice) dom.voiceOutputAutoLanguageVoice.checked = true;
    fire(dom.voiceOutputAutoLanguageVoice, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("windows-voice select change invokes handleProviderVoiceSelection (persists)", async () => {
    // handleProviderVoiceSelection only persists for Windows or Piper providers;
    // qwen3_tts (the test fixture default) early-returns. Switch the provider so
    // the persist branch is exercised.
    settings!.voice_output_settings!.default_provider = "windows_native";
    fire(dom.voiceOutputWindowsVoiceSelect, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("fallback-voice select change invokes handleProviderVoiceSelection (persists)", async () => {
    settings!.voice_output_settings!.fallback_provider = "windows_natural";
    fire(dom.voiceOutputFallbackVoiceSelect, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — rate slider", () => {
  it("input updates settings + live label without persisting", () => {
    if (dom.voiceOutputRate) dom.voiceOutputRate.value = "1.25";
    fire(dom.voiceOutputRate, "input");
    expect(settings!.voice_output_settings!.rate).toBeCloseTo(1.25);
    expect(dom.voiceOutputRateValue?.textContent).toBe("1.25");
    expect(mockedInvoke).not.toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("change persists settings", async () => {
    if (dom.voiceOutputRate) dom.voiceOutputRate.value = "1.5";
    fire(dom.voiceOutputRate, "change");
    await flush();
    expect(settings!.voice_output_settings!.rate).toBeCloseTo(1.5);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — volume slider", () => {
  it("input updates settings + live label without persisting", () => {
    if (dom.voiceOutputVolume) dom.voiceOutputVolume.value = "0.35";
    fire(dom.voiceOutputVolume, "input");
    expect(settings!.voice_output_settings!.volume).toBeCloseTo(0.35);
    expect(dom.voiceOutputVolumeValue?.textContent).toBe("0.35");
    expect(mockedInvoke).not.toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("change persists settings", async () => {
    if (dom.voiceOutputVolume) dom.voiceOutputVolume.value = "0.5";
    fire(dom.voiceOutputVolume, "change");
    await flush();
    expect(settings!.voice_output_settings!.volume).toBeCloseTo(0.5);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — piper gain", () => {
  it("input clamps below -24 to -24", () => {
    if (dom.voiceOutputPiperGainDb) dom.voiceOutputPiperGainDb.value = "-50";
    fire(dom.voiceOutputPiperGainDb, "input");
    expect(settings!.voice_output_settings!.piper_gain_db).toBe(-24);
    expect(dom.voiceOutputPiperGainDbValue?.textContent).toBe("-24 dB");
  });

  it("input clamps above 6 to 6", () => {
    if (dom.voiceOutputPiperGainDb) dom.voiceOutputPiperGainDb.value = "12";
    fire(dom.voiceOutputPiperGainDb, "input");
    expect(settings!.voice_output_settings!.piper_gain_db).toBe(6);
  });

  it("change persists settings and normalises input value", async () => {
    if (dom.voiceOutputPiperGainDb) dom.voiceOutputPiperGainDb.value = "100";
    fire(dom.voiceOutputPiperGainDb, "change");
    await flush();
    expect(settings!.voice_output_settings!.piper_gain_db).toBe(6);
    expect(dom.voiceOutputPiperGainDb!.value).toBe("6");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — test button", () => {
  it("renders success message when provider accepts", async () => {
    fire(dom.voiceOutputTestBtn, "click");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("test_tts_provider", { provider: "qwen3_tts" });
    expect(dom.voiceOutputTestStatus?.textContent).toContain("✓");
  });

  it("renders fallback-used warning when used_fallback is true", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "test_tts_provider") {
        return {
          provider_used: "windows_native",
          accepted: true,
          message: "",
          used_fallback: true,
          preferred_provider: "qwen3_tts",
        } as TtsSpeakResult;
      }
      return defaultInvokeImpl(cmd);
    });
    fire(dom.voiceOutputTestBtn, "click");
    await flush();
    expect(dom.voiceOutputTestStatus?.textContent).toContain("⚠");
    expect(dom.voiceOutputTestStatus?.textContent).toContain("qwen3_tts");
    expect(dom.voiceOutputTestStatus?.textContent).toContain("windows_native");
  });

  it("formats device-unavailable error with reset hint", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "test_tts_provider") {
        throw new Error("[tts_output_device_unavailable] device gone");
      }
      return defaultInvokeImpl(cmd);
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    fire(dom.voiceOutputTestBtn, "click");
    await flush();
    expect(dom.voiceOutputTestStatus?.textContent).toContain("Error:");
    expect(dom.voiceOutputTestStatus?.textContent).toContain("Voice Output device was reset to Default");
    errSpy.mockRestore();
  });

  it("formats generic error without reset hint", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "test_tts_provider") {
        throw new Error("boom");
      }
      return defaultInvokeImpl(cmd);
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    fire(dom.voiceOutputTestBtn, "click");
    await flush();
    expect(dom.voiceOutputTestStatus?.textContent).toBe("Error: boom");
    expect(dom.voiceOutputTestStatus?.textContent).not.toContain("Voice Output device was reset");
    errSpy.mockRestore();
  });
});

describe("wireVoiceOutput — piper paths", () => {
  it("piper-binary change persists value", async () => {
    if (dom.voiceOutputPiperBinary) dom.voiceOutputPiperBinary.value = "C:\\piper\\piper.exe";
    fire(dom.voiceOutputPiperBinary, "change");
    await flush();
    expect(settings!.voice_output_settings!.piper_binary_path).toBe("C:\\piper\\piper.exe");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("piper-model change persists value", async () => {
    if (dom.voiceOutputPiperModel) dom.voiceOutputPiperModel.value = "de_DE-thorsten.onnx";
    fire(dom.voiceOutputPiperModel, "change");
    await flush();
    expect(settings!.voice_output_settings!.piper_model_path).toBe("de_DE-thorsten.onnx");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("piper-model-dir change persists value", async () => {
    if (dom.voiceOutputPiperModelDir) dom.voiceOutputPiperModelDir.value = "C:\\models";
    fire(dom.voiceOutputPiperModelDir, "change");
    await flush();
    expect(settings!.voice_output_settings!.piper_model_dir).toBe("C:\\models");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireVoiceOutput — qwen3 fields", () => {
  it("qwen-endpoint change persists value", async () => {
    if (dom.voiceOutputQwenEndpoint) dom.voiceOutputQwenEndpoint.value = "https://api.example/tts";
    fire(dom.voiceOutputQwenEndpoint, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_endpoint).toBe("https://api.example/tts");
  });

  it("qwen-model change persists value", async () => {
    if (dom.voiceOutputQwenModel) dom.voiceOutputQwenModel.value = "qwen-tts-1";
    fire(dom.voiceOutputQwenModel, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_model).toBe("qwen-tts-1");
  });

  it("qwen-voice change persists value", async () => {
    if (dom.voiceOutputQwenVoice) dom.voiceOutputQwenVoice.value = "Cherry";
    fire(dom.voiceOutputQwenVoice, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_voice).toBe("Cherry");
  });

  it("qwen-api-key change persists value", async () => {
    if (dom.voiceOutputQwenApiKey) dom.voiceOutputQwenApiKey.value = "sk-test";
    fire(dom.voiceOutputQwenApiKey, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_api_key).toBe("sk-test");
  });

  it("qwen-timeout clamps below 3 to 3", async () => {
    if (dom.voiceOutputQwenTimeoutSec) dom.voiceOutputQwenTimeoutSec.value = "1";
    fire(dom.voiceOutputQwenTimeoutSec, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_timeout_sec).toBe(3);
    expect(dom.voiceOutputQwenTimeoutSec!.value).toBe("3");
  });

  it("qwen-timeout clamps above 180 to 180", async () => {
    if (dom.voiceOutputQwenTimeoutSec) dom.voiceOutputQwenTimeoutSec.value = "999";
    fire(dom.voiceOutputQwenTimeoutSec, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_timeout_sec).toBe(180);
  });

  it("qwen-timeout NaN value falls back to 45", async () => {
    if (dom.voiceOutputQwenTimeoutSec) dom.voiceOutputQwenTimeoutSec.value = "abc";
    fire(dom.voiceOutputQwenTimeoutSec, "change");
    await flush();
    expect(settings!.voice_output_settings!.qwen3_tts_timeout_sec).toBe(45);
  });
});
