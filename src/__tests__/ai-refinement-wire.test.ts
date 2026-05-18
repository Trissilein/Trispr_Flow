/**
 * Smoke tests for wireAiRefinement() — R2 slice 5.
 * Covers: language controls, post-processing toggles, AI fallback enable gate,
 * local backend switching, temperature slider, auth modal keydown, topic keywords reset.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>

    <!-- Language controls -->
    <select id="language-select">
      <option value="auto">auto</option>
      <option value="en">en</option>
      <option value="de">de</option>
    </select>
    <input id="language-pinned-toggle" type="checkbox" />
    <select id="whisper-input-language-select">
      <option value="auto">auto</option>
      <option value="en">en</option>
      <option value="de">de</option>
    </select>

    <!-- Post-processing -->
    <input id="postproc-enabled" type="checkbox" />
    <input id="postproc-punctuation" type="checkbox" />
    <input id="postproc-capitalization" type="checkbox" />
    <input id="postproc-numbers" type="checkbox" />
    <input id="postproc-custom-vocab-enabled" type="checkbox" />
    <div id="postproc-custom-vocab-config" style="display:none"></div>
    <button id="postproc-vocab-add"></button>

    <!-- AI fallback -->
    <input id="ai-fallback-enabled" type="checkbox" />
    <div id="ai-fallback-cloud-provider-list"></div>
    <select id="ai-fallback-local-backend-select">
      <option value="ollama">ollama</option>
      <option value="lm_studio">lm_studio</option>
      <option value="oobabooga">oobabooga</option>
    </select>
    <input id="ai-fallback-temperature" type="range" min="0" max="1" step="0.01" value="0.3" />
    <span id="ai-fallback-temperature-value"></span>
    <input id="ai-fallback-preserve-language" type="checkbox" />
    <input id="ai-fallback-low-latency-mode" type="checkbox" />
    <input id="ai-fallback-max-tokens" type="number" value="4000" />
    <select id="ai-fallback-model"><option value="">-</option></select>
    <div id="ai-fallback-local-lane"></div>
    <div id="ai-fallback-online-lane"></div>
    <button id="ai-fallback-local-primary-action" data-runtime-action="install"></button>
    <button id="ai-fallback-local-import-action"></button>
    <button id="ai-fallback-local-verify-action"></button>
    <button id="ai-fallback-local-refresh-action"></button>
    <button id="ai-fallback-fetch-versions-action"></button>
    <span id="ai-fallback-fetch-versions-status"></span>
    <select id="ai-fallback-local-runtime-version"><option value="0.20.2">0.20.2</option></select>
    <select id="ai-fallback-local-runtime-source">
      <option value="managed">managed</option>
      <option value="system">system</option>
    </select>
    <textarea id="ai-fallback-local-fallback-endpoints"></textarea>
    <input id="ai-fallback-compat-endpoint" value="http://127.0.0.1:1234" />
    <input id="ai-fallback-compat-api-key" value="" />
    <button id="ai-fallback-compat-fetch-models">Fetch models</button>
    <span id="ai-fallback-compat-status"></span>
    <button id="ai-fallback-compat-verify-action"></button>
    <button id="ai-fallback-lm-studio-install-action">Install LM Studio</button>

    <!-- Auth modal -->
    <div id="ai-auth-modal" hidden></div>
    <button id="ai-auth-modal-close"></button>
    <div id="ai-auth-modal-backdrop"></div>
    <select id="ai-auth-method"><option value="api_key">api_key</option><option value="oauth">oauth</option></select>
    <span id="ai-auth-provider-name"></span>
    <input id="ai-auth-api-key-input" type="password" value="" />
    <button id="ai-auth-save-key"></button>
    <button id="ai-auth-clear-key"></button>
    <button id="ai-auth-verify-key"></button>
    <span id="ai-auth-status"></span>

    <!-- Prompt presets -->
    <div id="prompt-preset-list"></div>
    <input id="ai-fallback-prompt-preset-name" value="" />
    <button id="ai-fallback-prompt-preset-save"></button>
    <button id="ai-fallback-prompt-preset-reset"></button>
    <button id="ai-fallback-prompt-preset-revert"></button>
    <button id="ai-fallback-prompt-preset-discard"></button>
    <button id="ai-fallback-prompt-preset-delete"></button>
    <textarea id="ai-fallback-custom-prompt"></textarea>

    <!-- Topic keywords -->
    <button id="topic-keywords-reset"></button>
    <div id="topic-keywords-list"></div>
    <button id="topic-keywords-add"></button>

    <!-- Modules -->
    <input id="modules-ai-refinement-toggle" type="checkbox" />
  `;
});

import { wireAiRefinement } from "../wiring/ai-refinement.wire";
import * as dom from "../dom-refs";
import { settings, setSettings } from "../state";
import type { Settings } from "../types";

// Stub out heavy rendering functions that touch uninitialized module state
vi.mock("../settings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../settings")>();
  return {
    ...actual,
    renderAIFallbackSettingsUi: vi.fn(),
    renderSettings: vi.fn(),
    renderTopicKeywords: vi.fn().mockResolvedValue(undefined),
    syncDerivedLanguageSettings: vi.fn(),
  };
});
vi.mock("../ollama-models", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ollama-models")>();
  return {
    ...actual,
    renderOllamaModelManager: vi.fn(),
    refreshOllamaRuntimeState: vi.fn().mockResolvedValue(undefined),
    getOllamaRuntimeCardState: vi.fn().mockReturnValue({ healthy: false }),
    refreshOllamaInstalledModels: vi.fn().mockResolvedValue(undefined),
    autoStartLocalRuntimeIfNeeded: vi.fn().mockResolvedValue(undefined),
    getOllamaRuntimeVersionCatalog: vi.fn().mockReturnValue([]),
    refreshOllamaRuntimeVersionCatalog: vi.fn().mockResolvedValue(undefined),
    refreshOllamaRuntimeAndModels: vi.fn().mockResolvedValue(undefined),
    showOllamaRequiredModal: vi.fn().mockResolvedValue(false),
    fetchOnlineVersionCatalog: vi.fn().mockResolvedValue(undefined),
  };
});
vi.mock("../ui-state", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ui-state")>();
  return { ...actual, renderHero: vi.fn() };
});

const mockedInvoke = vi.mocked(invoke);

function freshSettings(): Settings {
  return {
    capture_enabled: false,
    transcribe_enabled: false,
    mode: "ptt",
    input_device: "default",
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
    continuous_dump_enabled: false,
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
    language_mode: "auto",
    language_pinned: false,
    postproc_enabled: false,
    postproc_punctuation_enabled: true,
    postproc_capitalization_enabled: true,
    postproc_numbers_enabled: false,
    postproc_custom_vocab_enabled: false,
    postproc_language: "auto",
    postproc_llm_enabled: false,
    postproc_llm_provider: "ollama",
    postproc_llm_model: "",
    postproc_llm_prompt: "",
    topic_keywords: [],
    module_settings: {
      enabled_modules: ["ai_refinement"],
    },
    providers: {
      claude: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["claude-3-5-sonnet-20241022"],
        preferred_model: "claude-3-5-sonnet-20241022",
      },
      openai: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["gpt-4o-mini"],
        preferred_model: "gpt-4o-mini",
      },
      gemini: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: ["gemini-2.0-flash"],
        preferred_model: "gemini-2.0-flash",
      },
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "managed",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.20.2",
        last_health_check: null,
      },
    },
    ai_fallback: {
      enabled: false,
      provider: "ollama",
      fallback_provider: null,
      execution_mode: "local_primary",
      strict_local_mode: true,
      preserve_source_language: true,
      low_latency_mode: false,
      model: "",
      temperature: 0.3,
      max_tokens: 4000,
      prompt_profile: "balanced",
      custom_prompt_enabled: false,
      custom_prompt: "",
      use_default_prompt: true,
      prompt_presets: [],
      active_prompt_preset_id: "balanced",
    },
    local_backend_preference: "auto",
  } as unknown as Settings;
}

let wired = false;
function wireOnce() {
  if (wired) return;
  wireAiRefinement();
  wired = true;
}

beforeEach(() => {
  setSettings(freshSettings());
  mockedInvoke.mockReset();
  mockedInvoke.mockResolvedValue(undefined);
  if (dom.postprocEnabled) dom.postprocEnabled.checked = false;
  if (dom.postprocPunctuation) dom.postprocPunctuation.checked = true;
  if (dom.postprocCapitalization) dom.postprocCapitalization.checked = true;
  if (dom.postprocNumbers) dom.postprocNumbers.checked = false;
  if (dom.languageSelect) dom.languageSelect.value = "auto";
  if (dom.languagePinnedToggle) dom.languagePinnedToggle.checked = false;
  if (dom.whisperInputLanguageSelect) dom.whisperInputLanguageSelect.value = "auto";
  if (dom.aiFallbackEnabled) dom.aiFallbackEnabled.checked = false;
  if (dom.aiFallbackLocalBackendSelect) dom.aiFallbackLocalBackendSelect.value = "ollama";
  if (dom.aiFallbackTemperature) dom.aiFallbackTemperature.value = "0.3";
  if (dom.aiFallbackPreserveLanguage) dom.aiFallbackPreserveLanguage.checked = true;
  if (dom.aiFallbackLowLatencyMode) dom.aiFallbackLowLatencyMode.checked = false;
  if (dom.aiFallbackMaxTokens) dom.aiFallbackMaxTokens.value = "4000";
  const modal = document.getElementById("ai-auth-modal");
  if (modal) modal.hidden = true;
});

// ── Post-processing listeners ────────────────────────────────────────────────

describe("postproc toggles", () => {
  it("postprocEnabled: sets settings.postproc_enabled and calls save_settings", async () => {
    wireOnce();
    dom.postprocEnabled!.checked = true;
    dom.postprocEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_enabled).toBe(true);
  });

  it("postprocEnabled: unchecking sets postproc_enabled false and saves", async () => {
    wireOnce();
    setSettings({ ...freshSettings(), postproc_enabled: true });
    dom.postprocEnabled!.checked = false;
    dom.postprocEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_enabled).toBe(false);
  });

  it("postprocPunctuation: sets postproc_punctuation_enabled and saves", async () => {
    wireOnce();
    dom.postprocPunctuation!.checked = false;
    dom.postprocPunctuation!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_punctuation_enabled).toBe(false);
  });

  it("postprocCapitalization: sets postproc_capitalization_enabled and saves", async () => {
    wireOnce();
    dom.postprocCapitalization!.checked = false;
    dom.postprocCapitalization!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_capitalization_enabled).toBe(false);
  });

  it("postprocNumbers: sets postproc_numbers_enabled and saves", async () => {
    wireOnce();
    dom.postprocNumbers!.checked = true;
    dom.postprocNumbers!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_numbers_enabled).toBe(true);
  });

  it("postprocCustomVocabEnabled: shows/hides vocab config block", async () => {
    wireOnce();
    const config = document.getElementById("postproc-custom-vocab-config") as HTMLElement;
    dom.postprocCustomVocabEnabled!.checked = true;
    dom.postprocCustomVocabEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(settings?.postproc_custom_vocab_enabled).toBe(true));
    expect(config.style.display).toBe("flex");
    dom.postprocCustomVocabEnabled!.checked = false;
    dom.postprocCustomVocabEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(settings?.postproc_custom_vocab_enabled).toBe(false));
    expect(config.style.display).toBe("none");
  });
});

// ── Language controls ────────────────────────────────────────────────────────

describe("language controls", () => {
  it("languageSelect: change updates language_mode and saves", async () => {
    wireOnce();
    dom.languageSelect!.value = "en";
    dom.languageSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.language_mode).toBe("en");
  });

  it("languagePinnedToggle: sets language_pinned and saves", async () => {
    wireOnce();
    dom.languagePinnedToggle!.checked = true;
    dom.languagePinnedToggle!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.language_pinned).toBe(true);
  });

  it("whisperInputLanguageSelect: 'auto' sets mode=auto, pinned=false", async () => {
    wireOnce();
    setSettings({ ...freshSettings(), language_mode: "en", language_pinned: true });
    dom.whisperInputLanguageSelect!.value = "auto";
    dom.whisperInputLanguageSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.language_mode).toBe("auto");
    expect(settings?.language_pinned).toBe(false);
  });

  it("whisperInputLanguageSelect: non-auto value sets mode and pins language", async () => {
    wireOnce();
    dom.whisperInputLanguageSelect!.value = "de";
    dom.whisperInputLanguageSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.language_mode).toBe("de");
    expect(settings?.language_pinned).toBe(true);
  });
});

// ── AI fallback module gate ──────────────────────────────────────────────────

describe("AI fallback module gate", () => {
  it("aiFallbackEnabled: module disabled → checkbox is unchecked and no enable", async () => {
    wireOnce();
    setSettings({
      ...freshSettings(),
      module_settings: { enabled_modules: [] }, // ai_refinement NOT in list
    });
    dom.aiFallbackEnabled!.checked = true;
    dom.aiFallbackEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.enabled).toBe(false);
    expect(dom.aiFallbackEnabled!.checked).toBe(false);
  });

  it("aiFallbackEnabled: module enabled + ollama detected → enables AI", async () => {
    wireOnce();
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "detect_ollama_runtime") return { found: true };
      if (cmd === "save_settings") return undefined;
      return undefined;
    });
    dom.aiFallbackEnabled!.checked = true;
    dom.aiFallbackEnabled!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(settings?.ai_fallback.enabled).toBe(true), { timeout: 3000 });
    expect(settings?.postproc_llm_enabled).toBe(true);
  });
});

// ── Temperature slider ───────────────────────────────────────────────────────

describe("temperature slider", () => {
  it("input event: updates settings.ai_fallback.temperature immediately", () => {
    wireOnce();
    dom.aiFallbackTemperature!.value = "0.7";
    dom.aiFallbackTemperature!.dispatchEvent(new Event("input"));
    expect(settings?.ai_fallback.temperature).toBeCloseTo(0.7);
  });

  it("input event: clamps value to [0, 1]", () => {
    wireOnce();
    dom.aiFallbackTemperature!.value = "1.5";
    dom.aiFallbackTemperature!.dispatchEvent(new Event("input"));
    expect(settings?.ai_fallback.temperature).toBe(1);
  });

  it("change event: triggers save_settings", async () => {
    wireOnce();
    dom.aiFallbackTemperature!.value = "0.5";
    dom.aiFallbackTemperature!.dispatchEvent(new Event("input"));
    dom.aiFallbackTemperature!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
  });
});

// ── Local backend switching ──────────────────────────────────────────────────

describe("local backend switching", () => {
  it("aiFallbackLocalBackendSelect: switching to lm_studio updates provider", async () => {
    wireOnce();
    dom.aiFallbackLocalBackendSelect!.value = "lm_studio";
    dom.aiFallbackLocalBackendSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.provider).toBe("lm_studio");
  });

  it("aiFallbackLocalBackendSelect: switching to oobabooga updates provider", async () => {
    wireOnce();
    dom.aiFallbackLocalBackendSelect!.value = "oobabooga";
    dom.aiFallbackLocalBackendSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.provider).toBe("oobabooga");
  });

  it("aiFallbackLocalBackendSelect: switching back to ollama triggers runtime refresh", async () => {
    wireOnce();
    setSettings({ ...freshSettings(), ai_fallback: { ...freshSettings().ai_fallback, provider: "lm_studio" } });
    mockedInvoke.mockResolvedValue({ found: false, healthy: false });
    dom.aiFallbackLocalBackendSelect!.value = "ollama";
    dom.aiFallbackLocalBackendSelect!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(settings?.ai_fallback.provider).toBe("ollama"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
  });
});

// ── Auth modal ───────────────────────────────────────────────────────────────

describe("auth modal keydown", () => {
  it("Escape closes the auth modal when it is visible", () => {
    wireOnce();
    const modal = document.getElementById("ai-auth-modal") as HTMLElement;
    modal.hidden = false;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(modal.hidden).toBe(true);
  });

  it("Escape has no effect when modal is already hidden", () => {
    wireOnce();
    const modal = document.getElementById("ai-auth-modal") as HTMLElement;
    modal.hidden = true;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(modal.hidden).toBe(true);
  });

  it("other keys do not close the modal", () => {
    wireOnce();
    const modal = document.getElementById("ai-auth-modal") as HTMLElement;
    modal.hidden = false;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(modal.hidden).toBe(false);
  });
});

// ── Low-latency mode ─────────────────────────────────────────────────────────

describe("low-latency mode", () => {
  it("enabling low-latency clamps max_tokens to ≤512 and temperature to ≤0.2", async () => {
    wireOnce();
    setSettings({
      ...freshSettings(),
      ai_fallback: { ...freshSettings().ai_fallback, max_tokens: 4000, temperature: 0.8 },
    });
    dom.aiFallbackLowLatencyMode!.checked = true;
    dom.aiFallbackLowLatencyMode!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.max_tokens).toBeLessThanOrEqual(512);
    expect(settings?.ai_fallback.temperature).toBeLessThanOrEqual(0.2);
  });

  it("disabling low-latency does not change max_tokens or temperature", async () => {
    wireOnce();
    setSettings({
      ...freshSettings(),
      ai_fallback: { ...freshSettings().ai_fallback, low_latency_mode: true, max_tokens: 256, temperature: 0.15 },
    });
    dom.aiFallbackLowLatencyMode!.checked = false;
    dom.aiFallbackLowLatencyMode!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(settings?.ai_fallback.low_latency_mode).toBe(false));
    // Values should NOT be clamped on disable
    expect(settings?.ai_fallback.max_tokens).toBe(256);
  });
});

// ── Preserve source language ─────────────────────────────────────────────────

describe("preserve source language", () => {
  it("unchecking disables preserve_source_language and saves", async () => {
    wireOnce();
    dom.aiFallbackPreserveLanguage!.checked = false;
    dom.aiFallbackPreserveLanguage!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.preserve_source_language).toBe(false);
  });

  it("checking enables preserve_source_language and saves", async () => {
    wireOnce();
    setSettings({ ...freshSettings(), ai_fallback: { ...freshSettings().ai_fallback, preserve_source_language: false } });
    dom.aiFallbackPreserveLanguage!.checked = true;
    dom.aiFallbackPreserveLanguage!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.preserve_source_language).toBe(true);
  });
});

// ── Max tokens ───────────────────────────────────────────────────────────────

describe("max tokens", () => {
  it("change event: clamps to [128, 8192] and saves", async () => {
    wireOnce();
    dom.aiFallbackMaxTokens!.value = "99999";
    dom.aiFallbackMaxTokens!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.max_tokens).toBe(8192);
  });

  it("change event: clamps low values to 128", async () => {
    wireOnce();
    dom.aiFallbackMaxTokens!.value = "10";
    dom.aiFallbackMaxTokens!.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.ai_fallback.max_tokens).toBe(128);
  });
});

// ── Topic keywords reset ─────────────────────────────────────────────────────

describe("topic keywords reset", () => {
  it("click calls save_settings (reset flow runs)", async () => {
    wireOnce();
    setSettings({ ...freshSettings(), topic_keywords: [{ term: "custom", boost: 1 }] as any });
    const btn = document.getElementById("topic-keywords-reset") as HTMLButtonElement;
    btn.click();
    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
  });
});
