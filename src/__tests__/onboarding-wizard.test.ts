/**
 * Setup wizard wiring against the real wizard markup of index.html: which steps show, the
 * speech engine download with its self-test (CUDA failing falls back to Vulkan), background
 * model download, refinement default, and what finishing saves.
 *
 * dom-refs.ts resolves elements at import time, so every test sets the DOM up first and then
 * imports the wizard fresh (vi.resetModules).
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import indexHtml from "../../index.html?raw";
import type { Settings } from "../types";

type Handler = (args: Record<string, unknown> | undefined) => unknown;

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, Array<(event: { payload: unknown }) => unknown>>(),
  persistSettings: vi.fn(async () => {}),
  setupRefinementInBackground: vi.fn(async () => {}),
  showToast: vi.fn(),
  setupHotkeyRecorder: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (event: { payload: unknown }) => unknown) => {
    const list = mocks.listeners.get(name) ?? [];
    list.push(cb);
    mocks.listeners.set(name, list);
    return () => {
      mocks.listeners.set(name, (mocks.listeners.get(name) ?? []).filter((x) => x !== cb));
    };
  }),
}));
vi.mock("../settings-persist", () => ({ persistSettings: mocks.persistSettings }));
vi.mock("../ollama-models", () => ({ setupRefinementInBackground: mocks.setupRefinementInBackground }));
vi.mock("../toast", () => ({ showToast: mocks.showToast }));
vi.mock("../hotkeys", () => ({ setupHotkeyRecorder: mocks.setupHotkeyRecorder }));

function emit(name: string, payload: unknown): void {
  for (const cb of mocks.listeners.get(name) ?? []) void cb({ payload });
}

const flush = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};

function status(opts: { cuda?: boolean; vulkan?: boolean; bundled?: boolean }) {
  const variant = (name: "cuda" | "vulkan", ready: boolean) => ({
    variant: name,
    bundled: Boolean(opts.bundled) && ready,
    downloaded: !opts.bundled && ready,
    ready,
    zip_size: name === "cuda" ? 509_246_371 : 15_711_052,
  });
  const variants = [variant("cuda", Boolean(opts.cuda)), variant("vulkan", Boolean(opts.vulkan))];
  return { variants, any_ready: variants.some((v) => v.ready), whisper_cpp_version: "v1.9.5", runtime_dir: "C:\\r" };
}

const NVIDIA = {
  recommended: "cuda",
  gpu_name: "NVIDIA GeForce RTX 5070 Ti",
  nvidia: true,
  reason: "NVIDIA GeForce RTX 5070 Ti detected: CUDA is the fastest engine for it.",
};

function baseSettings(completed = false): Settings {
  return {
    mode: "ptt",
    hotkey_ptt: "Ctrl+Shift+Space",
    model: "",
    ai_fallback: { enabled: true, provider: "ollama", model: "gemma4:e4b" },
    setup: { local_ai_wizard_completed: completed, local_ai_wizard_pending: !completed },
  } as unknown as Settings;
}

interface Scenario {
  runtime: ReturnType<typeof status>;
  modelInstalled?: boolean;
  completed?: boolean;
  handlers?: Record<string, Handler>;
}

async function boot(scenario: Scenario) {
  vi.resetModules();
  mocks.listeners.clear();
  mocks.invoke.mockReset();
  mocks.persistSettings.mockClear();
  mocks.setupRefinementInBackground.mockClear();

  const wizard = new DOMParser().parseFromString(indexHtml, "text/html").getElementById("onboarding-wizard");
  document.body.innerHTML = wizard!.outerHTML;

  const handlers: Record<string, Handler> = {
    get_whisper_runtime_status: () => scenario.runtime,
    list_models: () => [{ id: "m", installed: Boolean(scenario.modelInstalled) }],
    detect_recommended_runtime: () => NVIDIA,
    verify_whisper_runtime: (args) => ({ variant: args?.variant, ok: true, detail: "ok", fallback_variant: null }),
    download_whisper_runtime: () => scenario.runtime,
    download_model: () => undefined,
    ...scenario.handlers,
  };
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    const handler = handlers[command];
    if (!handler) throw new Error(`unexpected invoke ${command}`);
    return handler(args);
  });

  const state = await import("../state");
  state.setSettings(baseSettings(scenario.completed));
  const wizardModule = await import("../onboarding-wizard");
  await wizardModule.initOnboardingWizard();
  await flush();
  return { state };
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const visibleStep = () =>
  Array.from(document.querySelectorAll<HTMLElement>(".wizard-step"))
    .filter((el) => !el.hidden)
    .map((el) => el.id.replace("wizard-step-", ""));
const click = async (id: string) => {
  $(id).click();
  await flush();
};

describe("setup wizard", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("stays hidden for someone who already has a model and a speech engine", async () => {
    await boot({ runtime: status({ vulkan: true, bundled: true }), modelInstalled: true });
    expect($("onboarding-wizard").hidden).toBe(true);
  });

  it("mini first run: engine step first, GPU recommendation shown, Next locked until the engine works", async () => {
    await boot({ runtime: status({}) });
    expect($("onboarding-wizard").hidden).toBe(false);
    expect(visibleStep()).toEqual(["runtime"]);
    expect($("wizard-step-total").textContent).toBe("5");
    expect($("wizard-gpu-line").textContent).toContain("RTX 5070 Ti");
    expect(document.querySelector<HTMLInputElement>('input[name="wizard-engine"][value="cuda"]')!.checked).toBe(true);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);
    expect($("wizard-engine-size-cuda").textContent).toBe("~486 MB");
  });

  it("downloads the chosen engine with progress, self-tests it and unlocks Next", async () => {
    const download = vi.fn(() => status({ cuda: true }));
    await boot({ runtime: status({}), handlers: { download_whisper_runtime: download } });
    await click("wizard-runtime-btn");

    expect(download).toHaveBeenCalledWith({ variant: "cuda" });
    expect(mocks.invoke).toHaveBeenCalledWith("verify_whisper_runtime", { variant: "cuda" });
    expect($("wizard-runtime-status").textContent).toBe("Speech engine ready (CUDA).");
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(false);

    await click("wizard-next-btn");
    expect(visibleStep()).toEqual(["model"]);
  });

  it("shows download progress from the backend events", async () => {
    let finish!: () => void;
    const download = vi.fn(
      () => new Promise((resolve) => (finish = () => resolve(status({ cuda: true }))))
    );
    await boot({ runtime: status({}), handlers: { download_whisper_runtime: download } });
    await click("wizard-runtime-btn");
    emit("whisper-runtime:progress", {
      variant: "cuda",
      stage: "download",
      message: "Downloading speech engine...",
      downloaded: 243_269_632,
      total: 509_246_371,
    });
    expect($("wizard-runtime-status").textContent).toContain("232 MB / 486 MB (48%)");
    expect($("wizard-runtime-progress-bar").style.width).toBe("48%");
    expect($<HTMLButtonElement>("wizard-runtime-btn").disabled).toBe(true);
    finish();
    await flush();
    expect($("wizard-runtime-status").textContent).toContain("ready");
  });

  it("CUDA failing its self-test loads Vulkan automatically and keeps the backend on Vulkan", async () => {
    const download = vi.fn((args?: Record<string, unknown>) =>
      args?.variant === "cuda" ? status({ cuda: true }) : status({ cuda: true, vulkan: true })
    );
    const { state } = await boot({
      runtime: status({}),
      handlers: {
        download_whisper_runtime: download,
        verify_whisper_runtime: (args) =>
          args?.variant === "cuda"
            ? { variant: "cuda", ok: false, detail: "The NVIDIA driver is too old.", fallback_variant: "vulkan" }
            : { variant: "vulkan", ok: true, detail: "ok", fallback_variant: null },
      },
    });
    await click("wizard-runtime-btn");

    expect(download.mock.calls.map((call) => (call[0] as { variant: string }).variant)).toEqual(["cuda", "vulkan"]);
    expect($("wizard-runtime-status").textContent).toBe("Speech engine ready (Vulkan).");
    expect(state.settings!.local_backend_preference).toBe("vulkan");
    expect(mocks.persistSettings).toHaveBeenCalled();
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(false);
  });

  it("a failed download offers Retry, keeps Next locked and works on the second try", async () => {
    let attempts = 0;
    await boot({
      runtime: status({}),
      handlers: {
        download_whisper_runtime: () => {
          attempts += 1;
          if (attempts === 1) throw new Error("The connection was closed before the download finished.");
          return status({ cuda: true });
        },
      },
    });
    await click("wizard-runtime-btn");
    expect($("wizard-runtime-status").textContent).toContain("The connection was closed");
    expect($("wizard-runtime-btn").textContent).toBe("Retry");
    expect($<HTMLButtonElement>("wizard-runtime-btn").disabled).toBe(false);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);

    await click("wizard-runtime-btn");
    expect(attempts).toBe(2);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(false);
  });

  it("bundled runtime: starts at the model step; the model downloads in the background while later steps go on", async () => {
    await boot({ runtime: status({ cuda: true, vulkan: true, bundled: true }) });
    expect(visibleStep()).toEqual(["model"]);
    expect($("wizard-step-total").textContent).toBe("4");
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);

    await click("wizard-download-model-btn");
    expect(mocks.invoke).toHaveBeenCalledWith("download_model", { model_id: "whisper-large-v3-turbo" });
    // Not finished, but on its way: the user may continue.
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(false);

    emit("model:download-progress", { id: "whisper-large-v3-turbo", downloaded: 725_000_000, total: 1_450_000_000 });
    await click("wizard-next-btn");
    expect(visibleStep()).toEqual(["mode"]);
    expect($("wizard-downloads").hidden).toBe(false);
    expect($("wizard-downloads").textContent).toContain("50%");
  });

  it("refinement is on by default; finishing starts Ollama + gemma4:e4b in the background", async () => {
    const { state } = await boot({ runtime: status({ cuda: true, bundled: true }) });
    await click("wizard-download-model-btn");
    for (let i = 0; i < 3; i++) await click("wizard-next-btn");
    expect(visibleStep()).toEqual(["refinement"]);
    expect($<HTMLInputElement>("wizard-refinement-check").checked).toBe(true);

    await click("wizard-finish-btn");
    expect(mocks.setupRefinementInBackground).toHaveBeenCalledWith("gemma4:e4b");
    expect(state.settings!.ai_fallback.enabled).toBe(true);
    expect(state.settings!.setup.local_ai_wizard_completed).toBe(true);
    expect(state.settings!.model).toBe("whisper-large-v3-turbo");
    expect($("onboarding-wizard").hidden).toBe(true);
  });

  it("unticking refinement skips the Ollama download and switches refinement off", async () => {
    const { state } = await boot({ runtime: status({ cuda: true, bundled: true }) });
    await click("wizard-download-model-btn");
    for (let i = 0; i < 3; i++) await click("wizard-next-btn");
    $<HTMLInputElement>("wizard-refinement-check").checked = false;

    await click("wizard-finish-btn");
    expect(mocks.setupRefinementInBackground).not.toHaveBeenCalled();
    expect(state.settings!.ai_fallback.enabled).toBe(false);
  });

  it("speech engine gone after a finished wizard: reopens with that step only and keeps the other settings", async () => {
    const { state } = await boot({ runtime: status({}), modelInstalled: true, completed: true });
    state.updateSettings({ mode: "vad", hotkey_ptt: "Alt+F9", model: "ggml-base" });
    expect(visibleStep()).toEqual(["runtime"]);
    expect($("wizard-step-total").textContent).toBe("1");
    expect($<HTMLButtonElement>("wizard-finish-btn").disabled).toBe(true);

    await click("wizard-runtime-btn");
    expect($<HTMLButtonElement>("wizard-finish-btn").disabled).toBe(false);
    await click("wizard-finish-btn");

    expect(state.settings!.mode).toBe("vad");
    expect(state.settings!.hotkey_ptt).toBe("Alt+F9");
    expect(state.settings!.model).toBe("ggml-base");
    expect(mocks.setupRefinementInBackground).not.toHaveBeenCalled();
  });

  it("an unreadable engine status does not trap the user in the engine step", async () => {
    await boot({
      runtime: status({}),
      handlers: {
        get_whisper_runtime_status: () => {
          throw new Error("unknown command");
        },
      },
    });
    expect(visibleStep()).toEqual(["model"]);
  });
});
