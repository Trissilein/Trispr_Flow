/**
 * Runtime version list in wireAiRefinement(): a button of the list runs the runtime flow for that
 * version, and the target must survive the settings normalisation: a pick in the list stays (even an
 * older one), a legacy default and a stale target nobody picked are fixed. The page has no backend,
 * runtime source or endpoint controls any more: the wiring must work without them.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>
    <input id="ai-fallback-enabled" type="checkbox" />
    <button id="ai-fallback-local-primary-action" data-runtime-action="ready"></button>
    <button id="ai-fallback-fetch-versions-action"></button>
    <span id="ai-fallback-fetch-versions-status"></span>
    <div id="ai-fallback-runtime-version-list"></div>
  `;
});

import { wireAiRefinement } from "../wiring/ai-refinement.wire";
import * as dom from "../dom-refs";
import { settings, setSettings } from "../state";
import { fetchOnlineVersionCatalog, runRuntimeVersionAction } from "../ollama-models";
import { RUNTIME_VERSION_ACTION_EVENT } from "../ollama-runtime-versions";
import type { Settings } from "../types";

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
    runRuntimeVersionAction: vi.fn().mockResolvedValue(undefined),
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
    mode: "ptt",
    input_device: "default",
    audio_cues: false,
    audio_cues_volume: 0.5,
    ptt_use_vad: false,
    ptt_hot_keepalive_ms: 600000,
    hallucination_filter_enabled: false,
    activation_words_enabled: false,
    activation_words: [],
    continuous_dump_enabled: false,
    continuous_dump_profile: "balanced",
    continuous_soft_flush_ms: 10000,
    continuous_silence_flush_ms: 1200,
    continuous_hard_cut_ms: 45000,
    continuous_min_chunk_ms: 1000,
    continuous_pre_roll_ms: 300,
    continuous_post_roll_ms: 200,
    continuous_idle_keepalive_ms: 60000,
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
    postproc_llm_model: "",
    postproc_llm_prompt: "",
    topic_keywords: [],
    module_settings: {
      enabled_modules: ["ai_refinement"],
    },
    providers: {
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "per_user_zip",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.20.2",
        runtime_target_explicit: false,
        last_health_check: null,
      },
    },
    ai_fallback: {
      enabled: false,
      provider: "ollama",
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

function withRuntime(installed: string, target: string, explicit = false, source = "per_user_zip"): Settings {
  const base = freshSettings();
  base.providers.ollama.runtime_source = source as never;
  base.providers.ollama.runtime_version = installed;
  base.providers.ollama.runtime_target_version = target;
  base.providers.ollama.runtime_target_explicit = explicit;
  return base;
}

function clickListAction(version: string, action: string) {
  dom.aiFallbackRuntimeVersionList!.dispatchEvent(
    new CustomEvent(RUNTIME_VERSION_ACTION_EVENT, { detail: { version, action } }),
  );
}

beforeEach(() => {
  if (!wired) {
    wireAiRefinement();
    wired = true;
  }
  mockedInvoke.mockReset();
  mockedInvoke.mockResolvedValue(undefined);
  vi.mocked(runRuntimeVersionAction).mockClear();
});

describe("runtime version list actions", () => {
  it("runs the runtime flow for the version and action of the clicked row", async () => {
    setSettings(withRuntime("0.30.4", "0.40.14"));

    clickListAction("0.40.7", "install");
    await vi.waitFor(() => expect(vi.mocked(runRuntimeVersionAction)).toHaveBeenCalledTimes(1));
    expect(vi.mocked(runRuntimeVersionAction)).toHaveBeenCalledWith("0.40.7", "install");

    clickListAction("0.30.4", "delete");
    await vi.waitFor(() => expect(vi.mocked(runRuntimeVersionAction)).toHaveBeenCalledTimes(2));
    expect(vi.mocked(runRuntimeVersionAction)).toHaveBeenLastCalledWith("0.30.4", "delete");
  });

  it("does nothing before the settings are loaded", async () => {
    setSettings(null as never);
    clickListAction("0.40.7", "install");
    await Promise.resolve();
    expect(vi.mocked(runRuntimeVersionAction)).not.toHaveBeenCalled();
  });
});

describe("runtime target normalisation", () => {
  it("keeps a target picked in the list while another version is installed", async () => {
    setSettings(withRuntime("0.30.4", "0.40.7", true));

    // Any normalisation pass (the primary-action click runs one first) must not fold the pick
    // back into the installed 0.30.4.
    dom.aiFallbackLocalPrimaryAction!.click();
    await vi.waitFor(() => expect(mockedInvoke.mock.calls.length).toBeGreaterThan(0));

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.7");
    expect(settings?.providers.ollama.runtime_version).toBe("0.30.4");
  });

  it("keeps an older pick as well", async () => {
    setSettings(withRuntime("0.40.14", "0.35.1", true));

    dom.aiFallbackLocalPrimaryAction!.click();
    await vi.waitFor(() => expect(mockedInvoke.mock.calls.length).toBeGreaterThan(0));

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.35.1");
    expect(settings?.providers.ollama.runtime_target_explicit).toBe(true);
  });

  it("lifts an older target nobody picked to the installed runtime (never a downgrade offer)", async () => {
    setSettings(withRuntime("0.40.14", "0.30.4", false));

    dom.aiFallbackLocalPrimaryAction!.click();
    await vi.waitFor(() => expect(mockedInvoke.mock.calls.length).toBeGreaterThan(0));

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.14");
  });

  it("moves a stored legacy default (0.20.2) to the recommended target instead of folding it into the installed one", async () => {
    setSettings(withRuntime("0.30.4", "0.20.2"));

    dom.aiFallbackLocalPrimaryAction!.click();
    await vi.waitFor(() => expect(mockedInvoke.mock.calls.length).toBeGreaterThan(0));

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.14");
    expect(settings?.providers.ollama.runtime_version).toBe("0.30.4");
  });
});

describe("version catalog", () => {
  it("the 'Get versions' button forces a fresh fetch", async () => {
    setSettings(withRuntime("0.40.7", "0.40.7"));
    dom.aiFallbackFetchVersionsAction!.click();
    await vi.waitFor(() => expect(vi.mocked(fetchOnlineVersionCatalog)).toHaveBeenCalledTimes(1));
    expect(vi.mocked(fetchOnlineVersionCatalog).mock.calls[0].length).toBe(1);
  });
});
