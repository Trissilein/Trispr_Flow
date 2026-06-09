/**
 * Smoke tests for wireAppChrome() - R2 slice 6.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../types";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  persistSettings: vi.fn(),
  renderAIFallbackSettingsUi: vi.fn(),
  renderSettings: vi.fn(),
  renderHero: vi.fn(),
  updateDeviceLineClamp: vi.fn(),
  togglePanel: vi.fn(),
  initHotkeyStatusListener: vi.fn(),
  setupHotkeyRecorder: vi.fn(),
  getOllamaRuntimeCardState: vi.fn(),
  refreshOllamaInstalledModels: vi.fn(),
  refreshOllamaRuntimeState: vi.fn(),
  renderOllamaModelManager: vi.fn(),
  applyAccentColor: vi.fn(),
  syncWorkflowAgentConsoleState: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

vi.mock("../settings", () => ({
  renderSettings: mocks.renderSettings,
}));

vi.mock("../settings/ai-refinement.settings", () => ({
  renderAIFallbackSettingsUi: mocks.renderAIFallbackSettingsUi,
}));

vi.mock("../settings-persist", () => ({
  persistSettings: mocks.persistSettings,
}));

vi.mock("../ui-state", () => ({
  renderHero: mocks.renderHero,
  updateDeviceLineClamp: mocks.updateDeviceLineClamp,
}));

vi.mock("../panels", () => ({
  isPanelId: (panelId: string) => panelId === "panel-a",
  togglePanel: mocks.togglePanel,
}));

vi.mock("../hotkeys", () => ({
  initHotkeyStatusListener: mocks.initHotkeyStatusListener,
  setupHotkeyRecorder: mocks.setupHotkeyRecorder,
}));

vi.mock("../ollama-models", () => ({
  getOllamaRuntimeCardState: mocks.getOllamaRuntimeCardState,
  refreshOllamaInstalledModels: mocks.refreshOllamaInstalledModels,
  refreshOllamaRuntimeState: mocks.refreshOllamaRuntimeState,
  renderOllamaModelManager: mocks.renderOllamaModelManager,
}));

vi.mock("../utils", () => ({
  DEFAULT_ACCENT_COLOR: "#14b8a6",
  applyAccentColor: mocks.applyAccentColor,
}));

vi.mock("../workflow-agent-console", () => ({
  syncWorkflowAgentConsoleState: mocks.syncWorkflowAgentConsoleState,
}));

function mountDom(): void {
  document.body.innerHTML = `
    <button id="tab-btn-transcription"></button>
    <button id="tab-btn-settings"></button>
    <button id="tab-btn-ai-refinement"></button>
    <button id="tab-btn-voice-output"></button>
    <button id="tab-btn-video"></button>
    <button id="tab-btn-agent"></button>
    <button id="tab-btn-modules"></button>
    <div id="tab-transcription"></div>
    <div id="tab-settings"></div>
    <div id="tab-ai-refinement"></div>
    <div id="tab-voice-output"></div>
    <div id="tab-video"></div>
    <div id="tab-agent"></div>
    <div id="tab-modules"></div>

    <button id="product-mode-transcribe-btn"></button>
    <button id="product-mode-assistant-btn"></button>
    <button id="global-online-offline-btn"></button>
    <button id="global-online-enabled-btn"></button>

    <div class="panel" data-panel="panel-a">
      <button class="panel-collapse-btn" data-panel-collapse="panel-a"></button>
      <div class="panel-header">
        <span id="panel-header-label">Header</span>
        <span class="panel-actions"><button id="panel-action-btn"></button></span>
        <input id="panel-header-input" />
      </div>
    </div>
    <button class="panel-collapse-btn" data-panel-collapse="invalid-panel"></button>

    <button id="analyse-button"></button>
    <button id="open-recordings-btn"></button>
    <button id="open-modules-btn"></button>

    <input id="product-mode-hotkey" />
    <button id="product-mode-hotkey-record"></button>
    <span id="product-mode-hotkey-status"></span>
    <input id="tts-stop-hotkey" />
    <button id="tts-stop-hotkey-record"></button>
    <span id="tts-stop-hotkey-status"></span>

    <input id="accent-color" type="color" value="#00ff00" />
    <button id="accent-color-reset"></button>
  `;
}

function makeSettings(options: {
  ai?: boolean;
  voice?: boolean;
  video?: boolean;
  assistant?: boolean;
} = {}): Settings {
  const enabledModules: string[] = [];
  if (options.ai) enabledModules.push("ai_refinement");
  if (options.voice) enabledModules.push("output_voice_tts");
  if (options.video) enabledModules.push("output_video_generation");
  if (options.assistant) enabledModules.push("assistant_core");
  return {
    product_mode: "transcribe",
    accent_color: "#00ff00",
    module_settings: {
      enabled_modules: enabledModules,
      consented_permissions: {},
      module_overrides: {},
    },
    workflow_agent: {
      enabled: options.assistant ?? false,
      online_enabled: false,
    },
    ai_fallback: {
      enabled: options.ai ?? false,
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
    voice_output_settings: {
      enabled: options.voice ?? false,
      output_device: "default",
    },
  } as unknown as Settings;
}

async function setup(options: Parameters<typeof makeSettings>[0] = {}) {
  vi.resetModules();
  mountDom();
  const state = await import("../state");
  const appChrome = await import("../wiring/app-chrome.wire");
  state.setSettings(makeSettings(options));
  return { state, appChrome };
}

function click(id: string): void {
  document.getElementById(id)?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

function activeTabId(): string | null {
  return document.querySelector("[id^='tab-btn-'].active")?.id ?? null;
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  localStorage.clear();
  Object.values(mocks).forEach((mock) => {
    if (typeof mock === "function" && "mockReset" in mock) mock.mockReset();
  });
  mocks.invoke.mockResolvedValue(undefined);
  mocks.persistSettings.mockResolvedValue(undefined);
  mocks.refreshOllamaRuntimeState.mockResolvedValue(undefined);
  mocks.refreshOllamaInstalledModels.mockResolvedValue(undefined);
  mocks.getOllamaRuntimeCardState.mockReturnValue({ healthy: true });
});

describe("wireAppChrome - main tabs", () => {
  it("initMainTab defaults to transcription", async () => {
    const { appChrome } = await setup();
    appChrome.initMainTab();
    expect(activeTabId()).toBe("tab-btn-transcription");
    expect(localStorage.getItem("trispr-active-tab")).toBe("transcription");
  });

  it("initMainTab restores a persisted settings tab", async () => {
    const { appChrome } = await setup();
    localStorage.setItem("trispr-active-tab", "settings");
    appChrome.initMainTab();
    expect(activeTabId()).toBe("tab-btn-settings");
  });

  it("openMainTab activates modules", async () => {
    const { appChrome } = await setup();
    appChrome.openMainTab("modules");
    expect(activeTabId()).toBe("tab-btn-modules");
  });

  it("tab buttons switch tabs", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    click("tab-btn-settings");
    expect(activeTabId()).toBe("tab-btn-settings");
  });

  it("hides unavailable AI tab", async () => {
    const { appChrome } = await setup({ ai: false });
    appChrome.initMainTab();
    const button = document.getElementById("tab-btn-ai-refinement") as HTMLButtonElement;
    expect(button.hidden).toBe(true);
    expect(button.getAttribute("aria-hidden")).toBe("true");
    expect(button.getAttribute("tabindex")).toBe("-1");
  });

  it("shows available AI tab", async () => {
    const { appChrome } = await setup({ ai: true });
    appChrome.initMainTab();
    const button = document.getElementById("tab-btn-ai-refinement") as HTMLButtonElement;
    expect(button.hidden).toBe(false);
    expect(button.getAttribute("aria-hidden")).toBe("false");
    expect(button.hasAttribute("tabindex")).toBe(false);
  });

  it("falls back when opening disabled Voice Output", async () => {
    const { appChrome } = await setup({ voice: false });
    appChrome.openMainTab("voice-output");
    expect(activeTabId()).toBe("tab-btn-transcription");
  });

  it("falls back when opening disabled video", async () => {
    const { appChrome } = await setup({ video: false });
    appChrome.openMainTab("video");
    expect(activeTabId()).toBe("tab-btn-transcription");
  });

  it("reconciles away from active video when video becomes unavailable", async () => {
    const { appChrome, state } = await setup({ video: true });
    appChrome.openMainTab("video");
    state.setSettings(makeSettings({ video: false }));
    appChrome.reconcileMainTabVisibility();
    expect(activeTabId()).toBe("tab-btn-transcription");
  });

  it("shows Agent tab only when assistant core is available", async () => {
    const { appChrome } = await setup({ assistant: true });
    appChrome.initMainTab();
    expect((document.getElementById("tab-btn-agent") as HTMLButtonElement).hidden).toBe(false);
  });

  it("reconciles away from active agent when assistant becomes unavailable", async () => {
    const { appChrome, state } = await setup({ assistant: true });
    appChrome.openMainTab("agent");
    state.setSettings(makeSettings({ assistant: false }));
    appChrome.reconcileMainTabVisibility();
    expect(activeTabId()).toBe("tab-btn-transcription");
  });

  it("refreshes Ollama state when opening AI tab", async () => {
    const { appChrome } = await setup({ ai: true });
    appChrome.openMainTab("ai-refinement");
    await flush();
    expect(mocks.refreshOllamaRuntimeState).toHaveBeenCalledWith({ force: true });
    expect(mocks.refreshOllamaInstalledModels).toHaveBeenCalled();
    expect(mocks.renderAIFallbackSettingsUi).toHaveBeenCalled();
    expect(mocks.renderOllamaModelManager).toHaveBeenCalled();
  });

  it("skips installed model refresh when Ollama runtime is unhealthy", async () => {
    mocks.getOllamaRuntimeCardState.mockReturnValue({ healthy: false });
    const { appChrome } = await setup({ ai: true });
    appChrome.openMainTab("ai-refinement");
    await flush();
    expect(mocks.refreshOllamaRuntimeState).toHaveBeenCalled();
    expect(mocks.refreshOllamaInstalledModels).not.toHaveBeenCalled();
  });
});

describe("wireAppChrome - app controls", () => {
  it("assistant mode button falls back when assistant is unavailable", async () => {
    const { appChrome, state } = await setup({ assistant: false });
    appChrome.wireAppChrome();
    click("product-mode-assistant-btn");
    await flush();
    expect(state.settings!.product_mode).toBe("transcribe");
    expect(mocks.persistSettings).not.toHaveBeenCalled();
  });

  it("assistant mode button persists when assistant is available", async () => {
    const { appChrome, state } = await setup({ assistant: true });
    appChrome.wireAppChrome();
    click("product-mode-assistant-btn");
    await flush();
    expect(state.settings!.product_mode).toBe("assistant");
    expect(mocks.syncWorkflowAgentConsoleState).toHaveBeenCalled();
    expect(mocks.persistSettings).toHaveBeenCalled();
    expect(mocks.renderHero).toHaveBeenCalled();
  });

  it("transcribe mode button persists transcribe mode", async () => {
    const { appChrome, state } = await setup({ assistant: true });
    state.settings!.product_mode = "assistant";
    appChrome.wireAppChrome();
    click("product-mode-transcribe-btn");
    await flush();
    expect(state.settings!.product_mode).toBe("transcribe");
    expect(mocks.persistSettings).toHaveBeenCalled();
  });

  it("global offline button persists offline mode", async () => {
    const { appChrome, state } = await setup({ assistant: true });
    state.settings!.workflow_agent!.online_enabled = true;
    appChrome.wireAppChrome();
    click("global-online-offline-btn");
    await flush();
    expect(state.settings!.workflow_agent!.online_enabled).toBe(false);
    expect(mocks.persistSettings).toHaveBeenCalled();
  });

  it("global online button persists online mode", async () => {
    const { appChrome, state } = await setup({ assistant: true });
    appChrome.wireAppChrome();
    click("global-online-enabled-btn");
    await flush();
    expect(state.settings!.workflow_agent!.online_enabled).toBe(true);
    expect(mocks.syncWorkflowAgentConsoleState).toHaveBeenCalled();
  });
});

describe("wireAppChrome - panels and shortcuts", () => {
  it("panel collapse button toggles a valid panel", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    document.querySelector<HTMLButtonElement>(".panel-collapse-btn")?.click();
    expect(mocks.togglePanel).toHaveBeenCalledWith("panel-a");
  });

  it("ignores invalid panel collapse buttons", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    document.querySelectorAll<HTMLButtonElement>(".panel-collapse-btn")[1]?.click();
    expect(mocks.togglePanel).not.toHaveBeenCalled();
  });

  it("panel header toggles its panel", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    document.getElementById("panel-header-label")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(mocks.togglePanel).toHaveBeenCalledWith("panel-a");
  });

  it("panel header ignores action controls", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    click("panel-action-btn");
    click("panel-header-input");
    expect(mocks.togglePanel).not.toHaveBeenCalled();
  });

  it("analyse button opens modules and dispatches focus event", async () => {
    const { appChrome } = await setup();
    const focusListener = vi.fn();
    window.addEventListener("modules:focus", focusListener);
    appChrome.wireAppChrome();
    click("analyse-button");
    expect(activeTabId()).toBe("tab-btn-modules");
    expect(focusListener).toHaveBeenCalledWith(expect.objectContaining({ detail: "analysis" }));
    window.removeEventListener("modules:focus", focusListener);
  });

  it("open modules button opens modules", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    click("open-modules-btn");
    expect(activeTabId()).toBe("tab-btn-modules");
  });

  it("open recordings button invokes backend command", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    click("open-recordings-btn");
    expect(mocks.invoke).toHaveBeenCalledWith("open_recordings_directory");
  });

  it("initializes hotkey status and recorders", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    expect(mocks.initHotkeyStatusListener).toHaveBeenCalled();
    expect(mocks.setupHotkeyRecorder).toHaveBeenCalledWith(
      "productModeToggle",
      expect.any(HTMLInputElement),
      expect.any(HTMLButtonElement),
      expect.any(HTMLSpanElement),
    );
    expect(mocks.setupHotkeyRecorder).toHaveBeenCalledWith(
      "ttsStop",
      expect.any(HTMLInputElement),
      expect.any(HTMLButtonElement),
      expect.any(HTMLSpanElement),
    );
  });

});

describe("wireAppChrome - accent color", () => {
  it("accent input applies live preview", async () => {
    const { appChrome, state } = await setup();
    const accent = document.getElementById("accent-color") as HTMLInputElement;
    appChrome.wireAppChrome();
    accent.value = "#123456";
    accent.dispatchEvent(new Event("input", { bubbles: true }));
    expect(state.settings!.accent_color).toBe("#123456");
    expect(mocks.applyAccentColor).toHaveBeenCalledWith("#123456");
  });

  it("accent change persists settings", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    document.getElementById("accent-color")?.dispatchEvent(new Event("change", { bubbles: true }));
    await flush();
    expect(mocks.persistSettings).toHaveBeenCalled();
  });

  it("accent reset restores default and persists", async () => {
    const { appChrome, state } = await setup();
    appChrome.wireAppChrome();
    click("accent-color-reset");
    await flush();
    expect(state.settings!.accent_color).toBe("#14b8a6");
    expect((document.getElementById("accent-color") as HTMLInputElement).value).toBe("#14b8a6");
    expect(mocks.applyAccentColor).toHaveBeenCalledWith("#14b8a6");
    expect(mocks.persistSettings).toHaveBeenCalled();
  });
});