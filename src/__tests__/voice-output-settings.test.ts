import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.hoisted(() => {
  document.body.innerHTML = `
    <input id="tts-stop-hotkey" />
    <select id="voice-output-default-provider">
      <option value="windows_native">Windows Native</option>
      <option value="windows_natural">Windows Natural</option>
      <option value="local_custom">Piper</option>
      <option value="qwen3_tts">Qwen3</option>
    </select>
    <select id="voice-output-fallback-provider">
      <option value="windows_native">Windows Native</option>
      <option value="windows_natural">Windows Natural</option>
      <option value="local_custom">Piper</option>
      <option value="qwen3_tts">Qwen3</option>
    </select>
    <select id="voice-output-policy">
      <option value="agent_replies_only">agent_replies_only</option>
      <option value="explicit_only">explicit_only</option>
    </select>
    <select id="voice-output-device-select"></select>
    <label id="voice-output-windows-voice-field">
      <select id="voice-output-windows-voice-select"></select>
      <span id="voice-output-windows-voice-hint"></span>
    </label>
    <label id="voice-output-fallback-voice-field">
      <select id="voice-output-fallback-voice-select"></select>
      <span id="voice-output-fallback-voice-hint"></span>
    </label>
    <div id="voice-output-auto-language-voice-field">
      <input id="voice-output-auto-language-voice" type="checkbox" />
    </div>
    <div id="voice-output-piper-download-status" hidden>
      <div><div id="voice-output-piper-download-fill"></div></div>
      <span id="voice-output-piper-download-text"></span>
    </div>
    <input id="voice-output-rate" type="range" min="0.5" max="2" step="0.05" />
    <span id="voice-output-rate-value"></span>
    <input id="voice-output-volume" type="range" min="0" max="1" step="0.05" />
    <span id="voice-output-volume-value"></span>
    <input id="voice-output-piper-gain-db" type="range" min="-24" max="6" step="1" />
    <span id="voice-output-piper-gain-db-value"></span>
    <input id="voice-output-piper-binary" />
    <input id="voice-output-piper-model" />
    <input id="voice-output-piper-model-dir" />
    <input id="voice-output-qwen-endpoint" />
    <input id="voice-output-qwen-model" />
    <input id="voice-output-qwen-voice" />
    <input id="voice-output-qwen-api-key" />
    <input id="voice-output-qwen-timeout-sec" type="number" min="3" max="180" />
    <section id="voice-output-qwen3-section"></section>
  `;
});

import {
  handlePiperVoiceDownloadProgress,
  handleProviderVoiceSelection,
  refreshProviderAvailability,
  refreshProviderVoices,
  renderVoiceOutputSettings,
} from "../settings/voice-output.settings";
import * as dom from "../dom-refs";
import { setOutputDevices, setSettings, settings } from "../state";
import type { Settings, TtsProviderInfo, TtsVoiceInfo } from "../types";

const mockedInvoke = vi.mocked(invoke);

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    hotkey_tts_stop: "CommandOrControl+Shift+F12",
    voice_output_settings: {
      default_provider: "windows_native",
      fallback_provider: "local_custom",
      output_policy: "agent_replies_only",
      output_device: "speakers-1",
      auto_voice_by_detected_language: true,
      rate: 1.25,
      volume: 0.7,
      piper_gain_db: -12,
      piper_binary_path: "C:/piper/piper.exe",
      piper_model_path: "de_DE-thorsten-medium",
      piper_model_dir: "C:/piper/voices",
      voice_id_windows: "",
      voice_id_windows_fallback: "",
      qwen3_tts_enabled: true,
      qwen3_tts_endpoint: "http://localhost:8000/v1/audio/speech",
      qwen3_tts_model: "qwen-model",
      qwen3_tts_voice: "vivian",
      qwen3_tts_api_key: "secret",
      qwen3_tts_timeout_sec: 45,
    },
    ...overrides,
  } as unknown as Settings;
}

function providerCatalog(overrides: Partial<TtsProviderInfo>[] = []): TtsProviderInfo[] {
  return [
    { id: "windows_native", label: "Windows Native", available: true, surface: "runtime_stable" },
    { id: "windows_natural", label: "Windows Natural", available: true, surface: "runtime_stable" },
    { id: "local_custom", label: "Piper", available: true, surface: "runtime_stable" },
    { id: "qwen3_tts", label: "Qwen3", available: true, surface: "benchmark_experimental" },
  ].map((provider, index) => ({ ...provider, ...overrides[index] })) as TtsProviderInfo[];
}

function defaultInvokeImpl(command: string): unknown {
  switch (command) {
    case "list_tts_providers":
      return providerCatalog();
    case "list_tts_voices":
      return [];
    case "list_piper_voice_catalog":
      return [];
    case "download_piper_voice_key":
    case "save_settings":
      return undefined;
    default:
      return undefined;
  }
}

async function flushAsync(): Promise<void> {
  for (let tick = 0; tick < 8; tick += 1) {
    await Promise.resolve();
  }
}

function byId<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

beforeEach(async () => {
  await flushAsync();
  setSettings(makeSettings());
  setOutputDevices([
    { id: "default", label: "Default" },
    { id: "speakers-1", label: "Desk Speakers" },
    { id: "headset-1", label: "Headset" },
  ]);
  mockedInvoke.mockReset();
  mockedInvoke.mockImplementation(async (command: string) => defaultInvokeImpl(command));
  vi.spyOn(window, "confirm").mockReturnValue(false);
  handlePiperVoiceDownloadProgress({
    voice_key: "reset",
    stage: "completed",
    file_name: "reset.onnx",
    downloaded_bytes: 0,
    total_bytes: 0,
    percent: 100,
    message: "Bereit.",
  });
  if (dom.voiceOutputPiperDownloadStatus) dom.voiceOutputPiperDownloadStatus.hidden = true;
  if (dom.voiceOutputPiperDownloadText) dom.voiceOutputPiperDownloadText.textContent = "";
  if (dom.voiceOutputPiperDownloadFill) dom.voiceOutputPiperDownloadFill.style.width = "";
});

afterEach(async () => {
  await flushAsync();
  vi.restoreAllMocks();
});

describe("renderVoiceOutputSettings", () => {
  it("is a no-op when settings are null", () => {
    setSettings(null);
    byId<HTMLInputElement>("tts-stop-hotkey").value = "unchanged";
    renderVoiceOutputSettings();
    expect(byId<HTMLInputElement>("tts-stop-hotkey").value).toBe("unchanged");
  });

  it("renders hotkey, provider chain, policy, sliders, and paths", () => {
    renderVoiceOutputSettings();
    expect(byId<HTMLInputElement>("tts-stop-hotkey").value).toContain("CommandOrControl");
    expect(byId<HTMLSelectElement>("voice-output-default-provider").value).toBe("windows_native");
    expect(byId<HTMLSelectElement>("voice-output-fallback-provider").value).toBe("local_custom");
    expect(byId<HTMLSelectElement>("voice-output-policy").value).toBe("agent_replies_only");
    expect(byId<HTMLInputElement>("voice-output-rate").value).toBe("1.25");
    expect(byId<HTMLElement>("voice-output-rate-value").textContent).toBe("1.25");
    expect(byId<HTMLInputElement>("voice-output-volume").value).toBe("0.7");
    expect(byId<HTMLElement>("voice-output-volume-value").textContent).toBe("0.70");
    expect(byId<HTMLInputElement>("voice-output-auto-language-voice").checked).toBe(true);
    expect(byId<HTMLInputElement>("voice-output-piper-binary").value).toBe("C:/piper/piper.exe");
    expect(byId<HTMLInputElement>("voice-output-piper-model-dir").value).toBe("C:/piper/voices");
  });

  it("rebuilds output devices and preserves a known configured device", () => {
    renderVoiceOutputSettings();
    const options = Array.from(byId<HTMLSelectElement>("voice-output-device-select").options).map((option) => option.value);
    expect(options).toEqual(["default", "speakers-1", "headset-1"]);
    expect(byId<HTMLSelectElement>("voice-output-device-select").value).toBe("speakers-1");
  });

  it("normalizes a missing output device back to default", () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, output_device: "missing" } }));
    renderVoiceOutputSettings();
    expect(settings!.voice_output_settings!.output_device).toBe("default");
    expect(byId<HTMLSelectElement>("voice-output-device-select").value).toBe("default");
  });

  it("coerces auto-language and clamps Piper gain before rendering", () => {
    setSettings(makeSettings({
      voice_output_settings: {
        ...settings!.voice_output_settings!,
        auto_voice_by_detected_language: "yes" as unknown as boolean,
        piper_gain_db: 99,
      },
    }));
    renderVoiceOutputSettings();
    expect(settings!.voice_output_settings!.auto_voice_by_detected_language).toBe(false);
    expect(byId<HTMLInputElement>("voice-output-auto-language-voice").checked).toBe(false);
    expect(settings!.voice_output_settings!.piper_gain_db).toBe(6);
    expect(byId<HTMLInputElement>("voice-output-piper-gain-db").value).toBe("6");
    expect(byId<HTMLElement>("voice-output-piper-gain-db-value").textContent).toBe("6 dB");
  });

  it("renders Qwen fields and clamps timeout", () => {
    setSettings(makeSettings({
      voice_output_settings: {
        ...settings!.voice_output_settings!,
        qwen3_tts_timeout_sec: 999,
      },
    }));
    renderVoiceOutputSettings();
    expect(byId<HTMLInputElement>("voice-output-qwen-endpoint").value).toBe("http://localhost:8000/v1/audio/speech");
    expect(byId<HTMLInputElement>("voice-output-qwen-model").value).toBe("qwen-model");
    expect(byId<HTMLInputElement>("voice-output-qwen-voice").value).toBe("vivian");
    expect(byId<HTMLInputElement>("voice-output-qwen-api-key").value).toBe("secret");
    expect(settings!.voice_output_settings!.qwen3_tts_timeout_sec).toBe(180);
    expect(byId<HTMLInputElement>("voice-output-qwen-timeout-sec").value).toBe("180");
  });

  it("toggles the Qwen section from the enabled flag", () => {
    renderVoiceOutputSettings();
    expect(byId<HTMLElement>("voice-output-qwen3-section").style.display).toBe("block");
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, qwen3_tts_enabled: false } }));
    renderVoiceOutputSettings();
    expect(byId<HTMLElement>("voice-output-qwen3-section").style.display).toBe("none");
  });
});

describe("refreshProviderAvailability", () => {
  it("populates provider options from backend data", async () => {
    await refreshProviderAvailability();
    expect(byId<HTMLSelectElement>("voice-output-default-provider").options.length).toBe(4);
    expect(byId<HTMLSelectElement>("voice-output-default-provider").value).toBe("windows_native");
    expect(mockedInvoke).toHaveBeenCalledWith("list_tts_providers");
  });

  it("normalizes unavailable preferred providers and persists the change", async () => {
    setSettings(makeSettings({
      voice_output_settings: {
        ...settings!.voice_output_settings!,
        default_provider: "qwen3_tts",
        fallback_provider: "qwen3_tts",
      },
    }));
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_tts_providers") {
        return providerCatalog([
          {},
          { available: false },
          {},
          { available: false, surface: "benchmark_experimental" },
        ]);
      }
      return defaultInvokeImpl(command);
    });

    await refreshProviderAvailability();

    expect(settings!.voice_output_settings!.default_provider).toBe("windows_native");
    expect(settings!.voice_output_settings!.fallback_provider).toBe("local_custom");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("leaves existing provider options untouched when backend availability fails", async () => {
    await refreshProviderAvailability();
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_tts_providers") throw new Error("offline");
      return defaultInvokeImpl(command);
    });

    await refreshProviderAvailability();

    expect(byId<HTMLSelectElement>("voice-output-default-provider").options.length).toBe(4);
  });

  it("mutually disables selected default and fallback options", async () => {
    setSettings(makeSettings({
      voice_output_settings: {
        ...settings!.voice_output_settings!,
        default_provider: "windows_native",
        fallback_provider: "local_custom",
      },
    }));
    await refreshProviderAvailability();
    const defaultLocalOption = Array.from(byId<HTMLSelectElement>("voice-output-default-provider").options)
      .find((option) => option.value === "local_custom");
    const fallbackNativeOption = Array.from(byId<HTMLSelectElement>("voice-output-fallback-provider").options)
      .find((option) => option.value === "windows_native");
    expect(defaultLocalOption?.disabled).toBe(true);
    expect(fallbackNativeOption?.disabled).toBe(true);
  });
});

describe("refreshProviderVoices", () => {
  it("hides voice selection for Qwen providers", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "qwen3_tts" } }));
    await refreshProviderVoices("default");
    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").disabled).toBe(true);
    expect(byId<HTMLElement>("voice-output-windows-voice-field").hidden).toBe(true);
    expect(byId<HTMLElement>("voice-output-windows-voice-hint").textContent).toContain("Qwen3-TTS");
  });

  it("renders Windows voices and resets an unavailable configured voice", async () => {
    const voices: TtsVoiceInfo[] = [
      { id: "voice-1", label: "Anna", provider: "windows_native", locale: "de-DE", profile: "natural" },
      { id: "voice-2", label: "Bob", provider: "windows_native", locale: "en-US", profile: "standard" },
    ];
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, voice_id_windows: "missing" } }));
    mockedInvoke.mockImplementation(async (command: string) => command === "list_tts_voices" ? voices : defaultInvokeImpl(command));

    await refreshProviderVoices("default");

    expect(settings!.voice_output_settings!.voice_id_windows).toBe("");
    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").options.length).toBe(3);
    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").options[1]?.textContent).toContain("Anna");
    expect(byId<HTMLElement>("voice-output-windows-voice-hint").textContent).toContain("2 Windows");
  });

  it("shows an auto option when Windows voice loading fails", async () => {
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_tts_voices") throw new Error("no voices");
      return defaultInvokeImpl(command);
    });

    await refreshProviderVoices("default");

    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").value).toBe("");
    expect(byId<HTMLElement>("voice-output-windows-voice-hint").textContent).toContain("no voices");
  });

  it("renders Piper catalog voices with installed voices first", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "local_custom" } }));
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_piper_voice_catalog") {
        return [
          { key: "downloadable", label: "Downloadable", installed: false, curated: true },
          { key: "de_DE-thorsten-medium", label: "Thorsten", installed: true, curated: true },
        ];
      }
      return defaultInvokeImpl(command);
    });

    await refreshProviderVoices("default");

    const select = byId<HTMLSelectElement>("voice-output-windows-voice-select");
    expect(select.options[0]?.value).toBe("de_DE-thorsten-medium");
    expect(select.options[0]?.dataset.piperInstalled).toBe("1");
    expect(select.options[1]?.value).toBe("downloadable");
    expect(byId<HTMLElement>("voice-output-windows-voice-hint").textContent).toContain("1/2");
  });

  it("resets a removed Piper voice key to the default voice", async () => {
    setSettings(makeSettings({
      voice_output_settings: {
        ...settings!.voice_output_settings!,
        default_provider: "local_custom",
        piper_model_path: "de_de-mls-medium",
      },
    }));
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_piper_voice_catalog") {
        return [{ key: "de_DE-thorsten-medium", label: "Thorsten", installed: true, curated: true }];
      }
      return defaultInvokeImpl(command);
    });

    await refreshProviderVoices("default");

    expect(settings!.voice_output_settings!.piper_model_path).toBe("de_DE-thorsten-medium");
    expect(byId<HTMLInputElement>("voice-output-piper-model").value).toBe("de_DE-thorsten-medium");
    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").value).toBe("de_DE-thorsten-medium");
  });

  it("falls back to the configured Piper path when catalog loading fails", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "local_custom" } }));
    mockedInvoke.mockImplementation(async (command: string) => {
      if (command === "list_piper_voice_catalog") throw new Error("catalog gone");
      return defaultInvokeImpl(command);
    });

    await refreshProviderVoices("default");

    expect(byId<HTMLSelectElement>("voice-output-windows-voice-select").value).toBe("de_DE-thorsten-medium");
    expect(byId<HTMLElement>("voice-output-windows-voice-hint").textContent).toContain("catalog gone");
  });
});

describe("handleProviderVoiceSelection", () => {
  it("persists the selected default Windows voice", async () => {
    const select = byId<HTMLSelectElement>("voice-output-windows-voice-select");
    select.innerHTML = `<option value="voice-1">Voice 1</option>`;
    select.value = "voice-1";

    await handleProviderVoiceSelection("default");

    expect(settings!.voice_output_settings!.voice_id_windows).toBe("voice-1");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("persists the selected fallback Windows voice", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, fallback_provider: "windows_native" } }));
    const select = byId<HTMLSelectElement>("voice-output-fallback-voice-select");
    select.innerHTML = `<option value="voice-2">Voice 2</option>`;
    select.value = "voice-2";

    await handleProviderVoiceSelection("fallback");

    expect(settings!.voice_output_settings!.voice_id_windows_fallback).toBe("voice-2");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("reverts an uninstalled Piper voice when the user declines the download", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "local_custom" } }));
    const select = byId<HTMLSelectElement>("voice-output-windows-voice-select");
    select.innerHTML = `<option value="de_DE-thorsten-medium" data-piper-installed="1">Thorsten</option><option value="new-voice" data-piper-installed="0">New Voice</option>`;
    select.value = "new-voice";

    await handleProviderVoiceSelection("default");

    expect(select.value).toBe("de_DE-thorsten-medium");
    expect(mockedInvoke).not.toHaveBeenCalledWith("download_piper_voice_key", expect.any(Object));
  });

  it("downloads and persists an uninstalled Piper voice when confirmed", async () => {
    vi.mocked(window.confirm).mockReturnValue(true);
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "local_custom" } }));
    const select = byId<HTMLSelectElement>("voice-output-windows-voice-select");
    select.innerHTML = `<option value="new-voice" data-piper-installed="0">New Voice</option>`;
    select.value = "new-voice";

    await handleProviderVoiceSelection("default");
    await flushAsync();

    expect(mockedInvoke).toHaveBeenCalledWith("download_piper_voice_key", { voiceKey: "new-voice" });
    expect(settings!.voice_output_settings!.piper_model_path).toBe("new-voice");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("resets removed Piper voices to the default voice", async () => {
    setSettings(makeSettings({ voice_output_settings: { ...settings!.voice_output_settings!, default_provider: "local_custom" } }));
    const select = byId<HTMLSelectElement>("voice-output-windows-voice-select");
    select.innerHTML = `<option value="de_de-mls-medium">Removed</option><option value="de_DE-thorsten-medium">Default</option>`;
    select.value = "de_de-mls-medium";

    await handleProviderVoiceSelection("default");

    expect(settings!.voice_output_settings!.piper_model_path).toBe("de_DE-thorsten-medium");
    expect(byId<HTMLInputElement>("voice-output-piper-model").value).toBe("de_DE-thorsten-medium");
  });
});

describe("handlePiperVoiceDownloadProgress", () => {
  it("updates visible progress for in-flight downloads", () => {
    handlePiperVoiceDownloadProgress({
      voice_key: "voice-a",
      stage: "downloading",
      file_name: "voice.onnx",
      downloaded_bytes: 35,
      total_bytes: 100,
      percent: 35,
      message: "Downloading",
    });
    expect(byId<HTMLDivElement>("voice-output-piper-download-fill").style.width).toBe("35%");
    expect(byId<HTMLDivElement>("voice-output-piper-download-status").hidden).toBe(false);
    expect(byId<HTMLElement>("voice-output-piper-download-text").textContent).toContain("Downloading");
  });

  it("marks completed downloads at full width", () => {
    handlePiperVoiceDownloadProgress({
      voice_key: "voice-a",
      stage: "completed",
      file_name: "voice.onnx",
      downloaded_bytes: 100,
      total_bytes: 100,
      percent: 100,
      message: "Done",
    });
    expect(byId<HTMLDivElement>("voice-output-piper-download-fill").style.width).toBe("100%");
    expect(byId<HTMLElement>("voice-output-piper-download-text").textContent).toContain("Done");
  });
});
