import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ModuleDescriptor, ModuleHealthStatus, Settings } from "../types";

const invokeMock = vi.fn();
const showToastMock = vi.fn();
const openMainTabMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("../toast", () => ({
  showToast: showToastMock,
}));

vi.mock("../wiring/app-chrome.wire", () => ({
  openMainTab: openMainTabMock,
}));

vi.mock("../modal-focus", () => ({
  focusFirstElement: vi.fn(),
}));

function moduleDefaults(overrides: Partial<ModuleDescriptor>): ModuleDescriptor {
  return {
    id: "ai_refinement",
    name: "AI Refinement",
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
  enabledModules: string[] = []
): Settings {
  return {
    module_settings: {
      enabled_modules: enabledModules,
      consented_permissions: consents,
      module_overrides: {},
    },
  } as unknown as Settings;
}

function bootstrapDom(): void {
  document.body.innerHTML = `
    <span id="modules-status"></span>
    <div id="modules-list"></div>
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
        id: "example_pending",
        name: "Example Pending",
        state: "installed",
        permissions: ["example_permission"],
      }),
      moduleDefaults({
        id: "example_granted",
        name: "Example Granted",
        state: "active",
        permissions: ["example_permission"],
      }),
    ];

    const state = await import("../state");
    state.setSettings(
      makeSettings({
        example_granted: ["example_permission"],
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
      "[data-module-card='example_pending'] .module-row-feedback"
    ) as HTMLElement | null;
    expect(feedback?.textContent).toContain("Consent required: example_permission");
  });

  it("renders setup health as a warning instead of descriptor error", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "example_module",
        name: "Example Module",
        state: "error",
        permissions: ["example_permission"],
        last_error: "Enabled module needs setup before it can run.",
      }),
    ];
    const health: ModuleHealthStatus[] = [
      {
        module_id: "example_module",
        state: "needs_setup",
        detail: "Consent required before this module is available: example_permission.",
      },
    ];

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_modules") return modules;
      if (command === "get_module_health") return health;
      return null;
    });

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const badge = document.querySelector(
      "[data-module-card='example_module'] .model-status"
    ) as HTMLElement | null;
    expect(badge?.textContent).toBe("Needs setup");
    expect(badge?.className).toContain("model-status--warning");
    expect(badge?.className).not.toContain("is-error");

    const feedback = document.querySelector(
      "[data-module-card='example_module'] .module-row-feedback"
    ) as HTMLElement | null;
    expect(feedback?.className).toContain("is-warning");
    expect(feedback?.className).not.toContain("is-error");
  });

  it("uses detailed consent text on enable and forwards grant permissions", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "example_module",
        name: "Example Module",
        state: "installed",
        permissions: ["example_permission"],
      }),
    ];

    invokeMock.mockImplementation(async (command: string, payload?: Record<string, unknown>) => {
      if (command === "list_modules") return modules;
      if (command === "enable_module") {
        modules[0] = { ...modules[0], state: "active" };
        return {
          message: "Module enabled.",
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
      "[data-module-action='enable'][data-module-id='example_module']"
    );
    expect(enableBtn).not.toBeNull();
    expect(enableBtn?.dataset.settingsVisibilityKey).toBe("module.example_module.state");
    expect(enableBtn?.dataset.settingsVisibilityDefault).toBe("expert");

    enableBtn?.click();
    await flushAsync();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    const confirmText = confirmSpy.mock.calls[0][0];
    expect(confirmText).toContain("example_permission");
    expect(confirmText).toContain("Permission required by this module.");

    expect(invokeMock).toHaveBeenCalledWith("enable_module", {
      moduleId: "example_module",
      grantPermissions: ["example_permission"],
    });

    confirmSpy.mockRestore();
  });

  it("routes AI Refinement configure action to the ai-refinement main tab", async () => {
    const modules: ModuleDescriptor[] = [
      moduleDefaults({
        id: "ai_refinement",
        name: "AI Refinement",
        state: "active",
      }),
    ];
    const state = await import("../state");
    state.setSettings(makeSettings({}, ["ai_refinement"]));

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "list_modules") return modules;
      return null;
    });

    const modulesHub = await import("../modules-hub");
    modulesHub.initModulesHub();
    await flushAsync();

    const configureBtn = document.querySelector<HTMLButtonElement>(
      "[data-module-action='open-config'][data-module-id='ai_refinement']"
    );
    expect(configureBtn).not.toBeNull();

    configureBtn?.click();
    await flushAsync();

    expect(openMainTabMock).toHaveBeenCalledWith("ai-refinement");
  });
});
