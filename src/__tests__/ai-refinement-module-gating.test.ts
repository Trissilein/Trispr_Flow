import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../types";

function makeSettings(moduleEnabled: boolean, aiEnabled: boolean): Settings {
  const enabledModules: string[] = [];
  if (moduleEnabled) enabledModules.push("ai_refinement");
  return {
    ai_fallback: {
      enabled: aiEnabled,
      provider: "ollama",
      preserve_source_language: true,
      model: "",
      temperature: 0.3,
      max_tokens: 4000,
      low_latency_mode: false,
      prompt_profile: "wording",
      custom_prompt_enabled: false,
      custom_prompt: "",
      use_default_prompt: false,
      prompt_presets: [],
      active_prompt_preset_id: "wording",
    },
    providers: {
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "manual",
        runtime_path: "",
        runtime_version: "",
        runtime_target_version: "0.17.7",
        last_health_check: null,
      },
    },
    module_settings: {
      enabled_modules: enabledModules,
      consented_permissions: {},
      module_overrides: {},
    },
    postproc_llm_enabled: aiEnabled,
  } as unknown as Settings;
}

function mountMainTabDom(): void {
  document.body.innerHTML = `
    <button id="tab-btn-transcription"></button>
    <button id="tab-btn-settings"></button>
    <button id="tab-btn-ai-refinement"></button>
    <button id="tab-btn-modules"></button>
    <div id="tab-transcription"></div>
    <div id="tab-settings"></div>
    <div id="tab-ai-refinement"></div>
    <div id="tab-modules"></div>
  `;
}

describe("AI refinement module tab gating", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    mountMainTabDom();
  });

  it("shows AI tab only when ai_refinement module is enabled", async () => {
    const state = await import("../state");
    const listeners = await import("../wiring/app-chrome.wire");

    state.setSettings(makeSettings(false, false));
    listeners.initMainTab();

    const aiTabBtn = document.getElementById("tab-btn-ai-refinement") as HTMLButtonElement | null;
    const aiTabPanel = document.getElementById("tab-ai-refinement") as HTMLDivElement | null;
    expect(aiTabBtn?.hidden).toBe(true);
    expect(aiTabPanel?.hidden).toBe(true);

    state.setSettings(makeSettings(true, true));
    listeners.reconcileMainTabVisibility();
    expect(aiTabBtn?.hidden).toBe(false);
    expect(aiTabPanel?.hidden).toBe(false);
  });

  it("falls back to transcription when active AI tab gets disabled", async () => {
    const state = await import("../state");
    const listeners = await import("../wiring/app-chrome.wire");

    state.setSettings(makeSettings(true, true));
    listeners.initMainTab();

    const transBtn = document.getElementById("tab-btn-transcription");
    const aiBtn = document.getElementById("tab-btn-ai-refinement");
    const transPanel = document.getElementById("tab-transcription");
    const aiPanel = document.getElementById("tab-ai-refinement");

    transBtn?.classList.remove("active");
    aiBtn?.classList.add("active");
    transPanel?.classList.remove("active");
    aiPanel?.classList.add("active");
    localStorage.setItem("trispr-active-tab", "ai-refinement");

    state.setSettings(makeSettings(false, false));
    listeners.reconcileMainTabVisibility();

    expect(localStorage.getItem("trispr-active-tab")).toBe("transcription");
    expect(transBtn?.classList.contains("active")).toBe(true);
    expect(aiBtn?.classList.contains("active")).toBe(false);
  });

  it("reports effective refinement enabled only when module+setting are on", async () => {
    const state = await import("../state");

    state.setSettings(makeSettings(false, true));
    expect(state.isRefinementEnabled()).toBe(false);

    state.setSettings(makeSettings(true, false));
    expect(state.isRefinementEnabled()).toBe(false);

    state.setSettings(makeSettings(true, true));
    expect(state.isRefinementEnabled()).toBe(true);
  });

  it("ignores a persisted tab id of a removed tab (voice-output)", async () => {
    const state = await import("../state");
    const listeners = await import("../wiring/app-chrome.wire");

    localStorage.setItem("trispr-active-tab", "voice-output");
    state.setSettings(makeSettings(true, true));
    listeners.initMainTab();

    const transBtn = document.getElementById("tab-btn-transcription");
    expect(transBtn?.classList.contains("active")).toBe(true);
    expect(localStorage.getItem("trispr-active-tab")).toBe("transcription");
  });
});
