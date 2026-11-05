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
  getOllamaRuntimeCardState: vi.fn(),
  refreshOllamaInstalledModels: vi.fn(),
  refreshOllamaRuntimeState: vi.fn(),
  renderOllamaModelManager: vi.fn(),
  applyAccentColor: vi.fn(),
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

function mountDom(): void {
  document.body.innerHTML = `
    <button id="tab-btn-transcription"></button>
    <button id="tab-btn-settings"></button>
    <button id="tab-btn-ai-refinement"></button>
    <div id="tab-transcription"></div>
    <div id="tab-settings"></div>
    <div id="tab-ai-refinement"></div>

    <div class="panel" data-panel="panel-a">
      <button class="panel-collapse-btn" data-panel-collapse="panel-a"></button>
      <div class="panel-header">
        <span id="panel-header-label">Header</span>
        <span class="panel-actions"><button id="panel-action-btn"></button></span>
        <input id="panel-header-input" />
      </div>
    </div>
    <button class="panel-collapse-btn" data-panel-collapse="invalid-panel"></button>

    <input id="accent-color" type="color" value="#00ff00" />
    <button id="accent-color-reset"></button>
  `;
}

function makeSettings(options: { ai?: boolean } = {}): Settings {
  return {
    accent_color: "#00ff00",
    ai_fallback: {
      enabled: options.ai ?? false,
      provider: "ollama",
      preserve_source_language: true,
      low_latency_mode: false,
      model: "",
      prompt_profile: "balanced",
      prompt_presets: [],
      active_prompt_preset_id: null,
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

  it("tab buttons switch tabs", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    click("tab-btn-settings");
    expect(activeTabId()).toBe("tab-btn-settings");
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

  it("initializes hotkey status listener", async () => {
    const { appChrome } = await setup();
    appChrome.wireAppChrome();
    expect(mocks.initHotkeyStatusListener).toHaveBeenCalled();
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
