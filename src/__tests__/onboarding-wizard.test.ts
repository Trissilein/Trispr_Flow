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
  openDialog: vi.fn(),
  writeText: vi.fn(async (_text: string) => {}),
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));

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

const MANUAL_INFO = {
  release_page_url: "https://github.com/Trissilein/Trispr_Flow/releases/tag/whisper-runtime-v1.9.5",
  whisper_cpp_version: "v1.9.5",
  variants: [
    {
      variant: "cuda",
      label: "NVIDIA CUDA",
      asset: "whisper-runtime-cuda-win64.zip",
      url: "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-cuda-win64.zip",
      size_mb: 486,
      size_bytes: 509_246_371,
      sha256: "7eb328951a8362db0213feecede812928165eb2cafa331a85fe5c41af465f67c",
      which_gpu_hint: "NVIDIA GeForce RTX, or GTX 16 series and newer, with a current driver (version 580 or newer).",
    },
    {
      variant: "vulkan",
      label: "Vulkan",
      asset: "whisper-runtime-vulkan-win64.zip",
      url: "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-vulkan-win64.zip",
      size_mb: 15,
      size_bytes: 15_711_052,
      sha256: "af6ced49ace46690ef890d63cd6324a1e832533995f941f86b80b7b6379c07d9",
      which_gpu_hint: "Everything else: Intel and AMD graphics, older NVIDIA cards, or when CUDA does not work on your PC.",
    },
  ],
};

const DIAGNOSTICS = {
  app_version: "0.10.0",
  os: "Windows 10.0.26300",
  arch: "x86_64",
  runtime_dir: "%LOCALAPPDATA%\\Trispr Flow\\runtime",
  whisper_cpp_version: "v1.9.5",
  adapters: [{ vendor_id: "0x10DE", name: "NVIDIA GeForce RTX 5070 Ti" }],
  variants: [
    { variant: "cuda", bundled: false, downloaded: false },
    { variant: "vulkan", bundled: false, downloaded: false },
  ],
};

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
  mocks.openDialog.mockReset();
  mocks.writeText.mockReset();
  mocks.writeText.mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText: mocks.writeText }, configurable: true });

  const wizard = new DOMParser().parseFromString(indexHtml, "text/html").getElementById("onboarding-wizard");
  document.body.innerHTML = wizard!.outerHTML;

  const handlers: Record<string, Handler> = {
    get_whisper_runtime_status: () => scenario.runtime,
    list_models: () => [{ id: "m", installed: Boolean(scenario.modelInstalled) }],
    detect_recommended_runtime: () => NVIDIA,
    verify_whisper_runtime: (args) => ({ variant: args?.variant, ok: true, detail: "ok", fallback_variant: null }),
    download_whisper_runtime: () => scenario.runtime,
    download_model: () => undefined,
    get_whisper_runtime_manual_info: () => MANUAL_INFO,
    get_whisper_runtime_diagnostics: () => DIAGNOSTICS,
    open_runtime_download_page: () => undefined,
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
  return { state, wizardModule };
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

  it("a failed download shows the troubleshooting panel with Retry, keeps Next locked and works on the second try", async () => {
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
    expect($("wizard-troubleshoot").hidden).toBe(false);
    expect($("wizard-troubleshoot-cause").textContent).toContain("The connection was closed");
    expect($("wizard-runtime-status").textContent).toBe("Speech engine setup failed.");
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);

    await click("wizard-troubleshoot-retry");
    expect(attempts).toBe(2);
    expect($("wizard-troubleshoot").hidden).toBe(true);
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

describe("setup wizard troubleshooting", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  const OFFLINE = {
    code: "offline",
    message: "Could not reach the download server (github.com).",
    hint: "Check your internet connection, VPN and firewall.",
    detail: "Dns Failed",
  };
  const failDownload = () => {
    throw OFFLINE;
  };
  const cards = () => Array.from(document.querySelectorAll<HTMLElement>(".wizard-package"));
  const buttonOf = (card: HTMLElement, label: string) =>
    Array.from(card.querySelectorAll<HTMLButtonElement>("button")).find((b) => b.textContent?.startsWith(label))!;

  async function failFirst(handlers: Scenario["handlers"] = {}, detect: unknown = NVIDIA) {
    const result = await boot({
      runtime: status({}),
      handlers: { download_whisper_runtime: failDownload, detect_recommended_runtime: () => detect, ...handlers },
    });
    await click("wizard-runtime-btn");
    await vi.waitFor(() => expect(cards().length).toBe(2));
    return result;
  }

  it("shows what went wrong and what to do from the structured error", async () => {
    await failFirst();
    expect($("wizard-troubleshoot").hidden).toBe(false);
    expect($("wizard-troubleshoot-cause").textContent).toBe(OFFLINE.message);
    expect($("wizard-troubleshoot-hint").textContent).toBe(OFFLINE.hint);
    expect($("wizard-troubleshoot-retry").textContent).toBe("Retry");
    expect($("wizard-runtime-btn").hidden).toBe(true);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);
    // The user can still pick the other engine before retrying.
    expect(document.querySelector<HTMLInputElement>('input[name="wizard-engine"]')!.disabled).toBe(false);
  });

  it("explains which package to take and lists both with name, size and SHA-256, the recommended one first", async () => {
    await failFirst();
    expect($("wizard-troubleshoot-which").textContent).toContain("NVIDIA RTX/GTX 16xx or newer");
    expect($("wizard-troubleshoot-which").textContent).toContain("Vulkan package");
    expect(cards().map((c) => c.dataset.variant)).toEqual(["cuda", "vulkan"]);
    const [cuda, vulkan] = cards();
    expect(cuda.textContent).toContain("NVIDIA CUDA package");
    expect(cuda.textContent).toContain("whisper-runtime-cuda-win64.zip");
    expect(cuda.textContent).toContain("486 MB");
    expect(cuda.querySelector("code")!.textContent).toBe(MANUAL_INFO.variants[0].sha256);
    expect(cuda.textContent).toContain("Recommended for this PC");
    expect(vulkan.textContent).not.toContain("Recommended for this PC");
    expect(vulkan.querySelector("code")!.textContent).toBe(MANUAL_INFO.variants[1].sha256);
    expect($("wizard-troubleshoot-packages").textContent).toContain("releases/tag/whisper-runtime-v1.9.5");
  });

  it("lists Vulkan first on a PC without an NVIDIA card", async () => {
    await failFirst(
      {},
      { recommended: "vulkan", gpu_name: "AMD Radeon RX 7800 XT", nvidia: false, reason: "No NVIDIA GPU detected." }
    );
    expect(cards().map((c) => c.dataset.variant)).toEqual(["vulkan", "cuda"]);
    expect(cards()[0].textContent).toContain("Recommended for this PC");
  });

  it("copies the SHA-256 of a package", async () => {
    await failFirst();
    buttonOf(cards()[1], "Copy").click();
    await flush();
    expect(mocks.writeText).toHaveBeenCalledWith(MANUAL_INFO.variants[1].sha256);
    expect($("wizard-troubleshoot-note").textContent).toBe("SHA-256 copied.");
  });

  it("opens the download page of the chosen package in the browser", async () => {
    await failFirst();
    buttonOf(cards()[1], "Open download page").click();
    await flush();
    expect(mocks.invoke).toHaveBeenCalledWith("open_runtime_download_page", { variant: "vulkan" });
    expect($("wizard-troubleshoot-note").textContent).toContain("Use downloaded file");
  });

  it("imports the picked .zip and runs the same self-test as after a download", async () => {
    const imported = vi.fn(() => status({ cuda: true }));
    await failFirst({ import_whisper_runtime_zip: imported });
    mocks.openDialog.mockResolvedValue("C:/Users/Max/Downloads/whisper-runtime-cuda-win64.zip");
    buttonOf(cards()[0], "Use downloaded file").click();
    await vi.waitFor(() => expect($("wizard-runtime-status").textContent).toBe("Speech engine ready (CUDA)."));

    expect(mocks.openDialog).toHaveBeenCalledWith(
      expect.objectContaining({
        directory: false,
        multiple: false,
        filters: [expect.objectContaining({ extensions: ["zip"] })],
      })
    );
    expect(imported).toHaveBeenCalledWith({
      variant: "cuda",
      path: "C:/Users/Max/Downloads/whisper-runtime-cuda-win64.zip",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("verify_whisper_runtime", { variant: "cuda" });
    expect($("wizard-troubleshoot").hidden).toBe(true);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(false);
  });

  it("cancelling the file dialog changes nothing", async () => {
    const imported = vi.fn();
    await failFirst({ import_whisper_runtime_zip: imported });
    mocks.openDialog.mockResolvedValue(null);
    buttonOf(cards()[0], "Use downloaded file").click();
    await flush();
    expect(imported).not.toHaveBeenCalled();
    expect($("wizard-troubleshoot").hidden).toBe(false);
  });

  it("a wrong file is explained and the user can try again", async () => {
    const wrong = {
      code: "wrong_package",
      message: "This is the Vulkan package, but you selected NVIDIA CUDA.",
      hint: "Use this file with the Vulkan package, or download the NVIDIA CUDA package.",
      detail: "",
    };
    let calls = 0;
    await failFirst({
      import_whisper_runtime_zip: () => {
        calls += 1;
        if (calls === 1) throw wrong;
        return status({ vulkan: true });
      },
    });
    mocks.openDialog.mockResolvedValue("C:/dl/whisper-runtime-vulkan-win64.zip");
    buttonOf(cards()[0], "Use downloaded file").click();
    await vi.waitFor(() => expect($("wizard-troubleshoot-cause").textContent).toBe(wrong.message));
    expect($("wizard-troubleshoot").hidden).toBe(false);
    expect($<HTMLButtonElement>("wizard-next-btn").disabled).toBe(true);

    // Same file with the package it belongs to.
    buttonOf(cards()[1], "Use downloaded file").click();
    await vi.waitFor(() => expect($("wizard-runtime-status").textContent).toBe("Speech engine ready (Vulkan)."));
    expect($("wizard-troubleshoot").hidden).toBe(true);
  });

  it("an imported CUDA engine that fails its self-test falls back to Vulkan; if that fails too, the Vulkan package comes first", async () => {
    const downloads: string[] = [];
    const { state } = await failFirst({
      download_whisper_runtime: (args) => {
        downloads.push(String(args?.variant));
        throw OFFLINE;
      },
      import_whisper_runtime_zip: () => status({ cuda: true }),
      verify_whisper_runtime: (args) =>
        args?.variant === "cuda"
          ? { variant: "cuda", ok: false, detail: "The NVIDIA driver is too old.", fallback_variant: "vulkan" }
          : { variant: "vulkan", ok: true, detail: "ok", fallback_variant: null },
    });
    mocks.openDialog.mockResolvedValue("C:/dl/whisper-runtime-cuda-win64.zip");
    buttonOf(cards()[0], "Use downloaded file").click();
    await vi.waitFor(() => expect($("wizard-troubleshoot-context").hidden).toBe(false));

    expect(downloads).toEqual(["cuda", "vulkan"]);
    expect($("wizard-troubleshoot-context").textContent).toContain(
      "CUDA does not work on this PC: The NVIDIA driver is too old."
    );
    await vi.waitFor(() => expect(cards().map((c) => c.dataset.variant)).toEqual(["vulkan", "cuda"]));
    expect(cards()[0].textContent).toContain("Recommended for this PC");
    expect(state.settings!.local_backend_preference).toBe("vulkan");
  });

  it("copies diagnostics with version, OS, GPU, engine and error, without the user name", async () => {
    await failFirst();
    $("wizard-troubleshoot-copy-diagnostics").click();
    await vi.waitFor(() => expect(mocks.writeText).toHaveBeenCalled());
    const text = String(mocks.writeText.mock.calls[0][0]);
    for (const expected of [
      "App version: 0.10.0",
      "OS: Windows 10.0.26300 (x86_64)",
      "Engine folder: %LOCALAPPDATA%",
      "GPU detection: recommends cuda; NVIDIA GeForce RTX 5070 Ti",
      "Engine in question: cuda",
      "Error code: offline",
      "Error: Could not reach the download server (github.com).",
    ]) {
      expect(text).toContain(expected);
    }
    expect(text).not.toContain("Max");
    await vi.waitFor(() => expect($("wizard-troubleshoot-note").textContent).toContain("Diagnostics copied"));
  });

  it("shows the diagnostics for copying by hand when the clipboard is not available", async () => {
    await failFirst();
    mocks.writeText.mockRejectedValue(new Error("denied"));
    $("wizard-troubleshoot-copy-diagnostics").click();
    await vi.waitFor(() => expect($("wizard-troubleshoot-diagnostics").hidden).toBe(false));
    expect($<HTMLTextAreaElement>("wizard-troubleshoot-diagnostics").value).toContain("Error code: offline");
  });

  it("says so when the package info cannot be read, and keeps Retry", async () => {
    await boot({
      runtime: status({}),
      handlers: {
        download_whisper_runtime: failDownload,
        get_whisper_runtime_manual_info: () => {
          throw new Error("unknown command");
        },
      },
    });
    await click("wizard-runtime-btn");
    await vi.waitFor(() => expect($("wizard-troubleshoot-packages").textContent).toContain("could not be read"));
    expect(cards().length).toBe(0);
    expect($("wizard-troubleshoot-retry")).toBeTruthy();
  });

  it("Open setup: a missing engine long after the first start reopens the wizard with the engine step only", async () => {
    let present = true;
    const { wizardModule } = await boot({
      runtime: status({}),
      modelInstalled: true,
      completed: true,
      handlers: {
        get_whisper_runtime_status: () => (present ? status({ vulkan: true, bundled: true }) : status({})),
      },
    });
    expect($("onboarding-wizard").hidden).toBe(true);

    present = false;
    await wizardModule.reopenSpeechEngineSetup();
    await flush();
    expect($("onboarding-wizard").hidden).toBe(false);
    expect(visibleStep()).toEqual(["runtime"]);
    expect($("wizard-step-total").textContent).toBe("1");
    expect($<HTMLButtonElement>("wizard-finish-btn").disabled).toBe(true);
    expect($("wizard-step-runtime").textContent!.match(/\(recommended\)/g)).toHaveLength(1);

    // The button works although the wizard was not shown at startup.
    await click("wizard-runtime-btn");
    expect(mocks.invoke).toHaveBeenCalledWith("download_whisper_runtime", { variant: "cuda" });
    await wizardModule.reopenSpeechEngineSetup();
    await flush();
    expect($("wizard-step-runtime").textContent!.match(/\(recommended\)/g)).toHaveLength(1);
  });
});
