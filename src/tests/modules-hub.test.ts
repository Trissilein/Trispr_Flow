import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ModuleDescriptor, Settings } from "../types";

const invokeMock = vi.fn();
const showToastMock = vi.fn();
const syncWorkflowAgentConsoleStateMock = vi.fn();
const focusWorkflowAgentConsoleMock = vi.fn();
const syncVoiceOutputConsoleStateMock = vi.fn();
const focusVoiceOutputConsoleMock = vi.fn();
const openMainTabMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("../toast", () => ({
  showToast: showToastMock,
}));

vi.mock("../gdd-flow", () => ({
  openGddFlow: vi.fn(),
}));

vi.mock("../workflow-agent-console", () => ({
  syncWorkflowAgentConsoleState: syncWorkflowAgentConsoleStateMock,
  focusWorkflowAgentConsole: focusWorkflowAgentConsoleMock,
}));

vi.mock("../event-listeners", () => ({
  openMainTab: openMainTabMock,
}));

vi.mock("../voice-output-console", () => ({
  syncVoiceOutputConsoleState: syncVoiceOutputConsoleStateMock,
  focusVoiceOutputConsole: focusVoiceOutputConsoleMock,
}));

vi.mock("../modal-focus", () => ({
  focusFirstElement: vi.fn(),
}));

function moduleDefaults(overrides: Partial<ModuleDescriptor>): ModuleDescriptor {
  return {
    id: "workflow_agent",
    name: "Workflow Agent",
    version: "0.1.0",
    state: "installed",
    dependencies: [],
    permissions: [],
    restart_required: false,
    last_error: null,
    bundled: true,
    core: false,
    toggleable: true,
    ...overrides,
  };
}

function makeSettings(
  consents: Record<string, string[]> = {},
  enabledModules: string[] = [],
  workflowAgentEnabled = false
): Settings {
  return {
    module_settings: {
      enabled_modules: enabledModules,
      consented_permissions: consents,
      module_overrides: {},
    },
    workflow_agent: {
      enabled: workflowAgentEnabled,
      wakewords: ["trispr"],
      intent_keywords: { gdd_generate_publish: ["gdd"] },
      model: "qwen3.5:4b",
      temperature: 0.2,
      max_tokens: 512,
      session_gap_minutes: 20,
      max_candidates: 3,
      hands_free_enabled: false,
      confirm_timeout_sec: 45,
      reply_mode: "rule_only",
      voice_feedback_enabled: false,
    },
  } as unknown as Settings;
}

function bootstrapDom(): void {
  document.body.innerHTML = `
    <span id="modules-status"></span>
    <div id="modules-list"></div>
    <div id="workflow-agent-console" hidden></div>
    <div id="voice-output-console" hidden></div>
    <div id="module-config-modal" hidden></div>
    <button id="module-config-modal-close" type="button"></button>
    <div id="module-config-modal-backdrop"></div>
    <h3 id="module-config-modal-name"></h3>
    <p id="module-config-modal-meta"></p>
    <p id="module-config-modal-desc"></p>
    <p id="module-config-modal-usage"></p>
    <div id="module-config-modal-deps"></div>
    <div id="module-config-modal-feedback"></div>
  `;
}

async function flushAsync(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("modules-hub consent messaging", () => {
  beforeEach(async () => {
    vi.resetModules();
    invokeMock.mockReset();
    showToastMock.mockReset();
    syncWorkflowAgentConsoleStateMock.mockReset();
    focusWorkflowAgentConsoleMock.mockReset();
    syncVoiceOutputConsoleStateMock.mockReset();
    focusVoiceOutputConsoleMock.mockReset();
    openMainTabMock.mockReset();
    bootstrapDom();
    const state = await import("../state");
    state.setSettings(makeSettings());
  });

  afterEach(async () => {
    const state = await import("../state");
    state.setSettings(null);
    document.body.innerHTML = "";
  });

  it("renders consent-specific feedback and status count for pending permissions", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "input_vision",
        name: "Screen Vision Input",
        state: "installed",
        permissions: ["screen_capture"],
      }),
      moduleDefaults({
        id: "output_voice_tts",
        name: "Voice Output (TTS)",
        state: "active",
        permissions: ["audio_output"],
      }),
    ];

    const state = await import("../state");
    state.setSettings(
      makeSettings({
        output_voice_tts: ["audio_output"],
      })
    );

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_modules") return modules;
      return null;
    });

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const status = document.getElementById("modules-status");
    expect(status?.textContent).toBe("1/2 active · 1 consent pending");

    const feedback = document.querySelector(
      "[data-module-card='input_vision'] .module-card-feedback"
    ) as HTMLElement | null;
    expect(feedback?.textContent).toContain("Screen capture consent missing");
  });

  it("uses detailed consent text on enable and forwards grant permissions", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "output_voice_tts",
        name: "Voice Output (TTS)",
        state: "installed",
        permissions: ["audio_output"],
      }),
    ];

    invokeMock.mockImplementation(async (command: string, payload?: Record<string, unknown>) => {
      if (command === "list_modules") return modules;
      if (command === "enable_module") {
        modules[0] = { ...modules[0], state: "active" };
        return {
          message: "Voice output module enabled.",
          restart_required: false,
          payload,
        };
      }
      return null;
    });

    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const enableBtn = document.querySelector<HTMLButtonElement>(
      "[data-module-action='enable'][data-module-id='output_voice_tts']"
    );
    expect(enableBtn).not.toBeNull();

    enableBtn?.click();
    await flushAsync();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    const confirmText = confirmSpy.mock.calls[0][0];
    expect(confirmText).toContain("Audio output");
    expect(confirmText).toContain("TTS playback only");

    expect(invokeMock).toHaveBeenCalledWith("enable_module", {
      moduleId: "output_voice_tts",
      grantPermissions: ["audio_output"],
    });

    confirmSpy.mockRestore();
  });

  it("routes TTS configure action to voice-output main tab", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "output_voice_tts",
        name: "Voice Output (TTS)",
        state: "active",
        permissions: ["audio_output"],
      }),
    ];
    const state = await import("../state");
    state.setSettings(makeSettings({}, ["output_voice_tts"]));

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_modules") return modules;
      return null;
    });

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const configureBtn = document.querySelector<HTMLButtonElement>(
      "[data-module-action='open-config'][data-module-id='output_voice_tts']"
    );
    expect(configureBtn).not.toBeNull();

    configureBtn?.click();
    await flushAsync();

    expect(openMainTabMock).toHaveBeenCalledWith("voice-output");
    expect(focusVoiceOutputConsoleMock).toHaveBeenCalledTimes(1);

    const modal = document.getElementById("module-config-modal");
    expect(modal?.hasAttribute("hidden")).toBe(true);
  });

  it("routes Workflow Agent configure action to agent main tab", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "workflow_agent",
        name: "Workflow Agent",
        state: "active",
      }),
    ];
    const state = await import("../state");
    state.setSettings(makeSettings({}, ["workflow_agent"], true));

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_modules") return modules;
      return null;
    });

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const configureBtn = document.querySelector<HTMLButtonElement>(
      "[data-module-action='open-config'][data-module-id='workflow_agent']"
    );
    expect(configureBtn).not.toBeNull();

    configureBtn?.click();
    await flushAsync();

    expect(openMainTabMock).toHaveBeenCalledWith("agent");
    expect(focusWorkflowAgentConsoleMock).toHaveBeenCalledTimes(1);
  });
});
