import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { OllamaRuntimeVersionInfo } from "../types";
import { downloadSelected, modelBox } from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

const OLD_DIGEST = "c6eb396dbd5992bbe3f5cdb947e8bbc0ee413d7c17e2beaae69f5d569cf982eb";
const NEW_DIGEST = "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c";
const OLD = "gemma4:e4b-it-q4_K_M";
const CURRENT = "gemma4:e4b";
const NEEDY = "gemma4:e2b"; // stands in for a model that needs a newer runtime
const NEEDY_REFUSAL =
  "Model 'gemma4:e2b' needs Ollama >= 0.40.13 (running 0.40.7). Update the runtime first.";
const STREAMED_412 =
  "pull model manifest: 412: The model you are attempting to pull requires a newer version of Ollama.";

function runtime(version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo {
  return {
    version,
    source: "online",
    selected: false,
    installed: false,
    recommended: false,
    prerelease: false,
    installable: true,
    ...overrides,
  };
}

type Handler = (args?: Record<string, unknown>) => unknown;
type Listener = (event: { payload: unknown }) => void;
type InstalledRow = { name: string; size_bytes: number; digest?: string };

const flush = () => new Promise((resolve) => setTimeout(resolve, 70));

async function load(
  handlers: Record<string, Handler> = {},
  options: { catalogJson?: unknown; settingsModel?: string; target?: string; source?: string } = {}
) {
  vi.resetModules();
  if (options.catalogJson) vi.doMock("../model-catalog.json", () => ({ default: options.catalogJson }));
  else vi.doUnmock("../model-catalog.json");
  document.body.innerHTML = '<div id="ollama-model-manager"></div>';

  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const toastModule = await import("../toast");
  const showToast = vi.mocked(toastModule.showToast) as unknown as Mock<(options: ToastArgs) => unknown>;
  const state = await import("../state");
  const mod = await import("../ollama-models");

  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  vi.mocked(invoke).mockImplementation((async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    const handler = handlers[command];
    return handler ? handler(args) : {};
  }) as never);

  const listeners = new Map<string, Listener[]>();
  vi.mocked(listen).mockImplementation((async (event: string, callback: Listener) => {
    listeners.set(event, [...(listeners.get(event) ?? []), callback]);
    return () => {};
  }) as never);
  const emit = (event: string, payload: unknown) => {
    for (const callback of listeners.get(event) ?? []) callback({ payload });
  };

  const model = options.settingsModel ?? CURRENT;
  state.setSettings({
    ai_fallback: {
      provider: "ollama",
      model,
      prompt_presets: [],
      prompt_profile: "wording",
      active_prompt_preset_id: "wording",
    },
    postproc_llm_model: model,
    providers: {
      ollama: {
        preferred_model: model,
        runtime_source: options.source ?? "per_user_zip",
        runtime_target_version: options.target ?? "0.40.7",
        endpoint: "http://127.0.0.1:11434",
      },
    },
  } as never);

  const callsOf = (command: string) => calls.filter((call) => call.command === command);
  const container = () => document.getElementById("ollama-model-manager")!;
  const render = async () => {
    mod.renderOllamaModelManager();
    await flush();
  };
  const cardOf = (name: string) =>
    Array.from(container().querySelectorAll<HTMLElement>(".qwen-model-card")).find(
      (card) => card.dataset.model === name || card.querySelector(".qwen-card-tag")?.textContent === name
    );
  const button = (selector: string) => container().querySelector<HTMLButtonElement>(selector);
  const boot = async () => {
    await mod.refreshOllamaRuntimeState({ force: true });
    await mod.refreshOllamaInstalledModels();
    await mod.refreshOllamaModelUpdateStatuses();
    await mod.refreshOllamaModelRequirements();
    await render();
  };
  const confirmDialog = async (action: "confirm" | "cancel") => {
    const dialog = document.querySelector<HTMLElement>(".ollama-confirm-dialog");
    expect(dialog).not.toBeNull();
    const text = dialog!.textContent ?? "";
    dialog!.querySelector<HTMLButtonElement>(`button[data-dialog-action="${action}"]`)!.click();
    await flush();
    expect(document.querySelector(".ollama-confirm-dialog")).toBeNull();
    return text;
  };

  return { mod, state, invoke, showToast, calls, callsOf, emit, listeners, container, render, cardOf, button, boot, confirmDialog };
}

function runtimeHandlers(
  version: { current: string },
  catalog: () => OllamaRuntimeVersionInfo[]
): Record<string, Handler> {
  return {
    list_ollama_runtime_versions: () => catalog(),
    fetch_ollama_online_versions: () => catalog(),
    detect_ollama_runtime: () => ({
      found: true,
      is_serving: true,
      source: "per_user_zip",
      path: "C:/ollama/ollama.exe",
      version: version.current,
      managed_alive: true,
    }),
    start_ollama_runtime: () => ({ pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false }),
    verify_ollama_runtime: () => ({ ok: true, endpoint: "http://127.0.0.1:11434", models_count: 0 }),
  };
}

type ToastArgs = { title: string; message: string; actionLabel?: string; onAction?: () => unknown };

const toastNamed = (showToast: Mock<(options: ToastArgs) => unknown>, title: string): ToastArgs => {
  const found = showToast.mock.calls.map((call) => call[0]).find((toast) => toast.title === title);
  if (!found) throw new Error(`no toast titled "${title}"`);
  return found;
};

const toastTitles = (showToast: Mock<(options: ToastArgs) => unknown>) =>
  showToast.mock.calls.map((call) => call[0].title);

describe("runtime requirement chip, gated pull and one-click runtime update", () => {
  const version = { current: "0.40.7" };
  let catalogState: OllamaRuntimeVersionInfo[];
  const requirements = (names: string[]) =>
    names
      .filter((name) => name === NEEDY || name === "qwen3.5:4b")
      .map((name) => ({
        name,
        requires: name === NEEDY ? "0.40.13" : "0.17.0",
        runtime_version: version.current,
        compatible: name === NEEDY ? false : true,
      }));

  beforeEach(() => {
    version.current = "0.40.7";
    catalogState = [
      runtime("0.40.7", { source: "pinned", recommended: true, installed: true, selected: true }),
      runtime("0.40.13", { latest: true }),
    ];
  });

  function handlers(extra: Record<string, Handler> = {}) {
    return {
      ...runtimeHandlers(version, () => catalogState),
      fetch_ollama_models_with_size: () => [{ name: CURRENT, size_bytes: 6_583_658_527, digest: NEW_DIGEST }] as InstalledRow[],
      check_ollama_model_requirements: (args?: Record<string, unknown>) => requirements((args?.models as string[]) ?? []),
      download_ollama_runtime: () => ({ sha256_ok: true, archive_path: "a.zip", version: "0.40.13" }),
      install_ollama_runtime: () => {
        version.current = "0.40.13";
        catalogState = [
          runtime("0.40.7", { source: "pinned", recommended: true, installed: true }),
          runtime("0.40.13", { latest: true, installed: true, selected: true }),
        ];
        return { runtime_path: "x", version: "0.40.13" };
      },
      ...extra,
    };
  }

  it("shows 'Needs Ollama >= X', disables the pull and offers 'Update runtime to X' only where it is needed", async () => {
    const ctx = await load(handlers());
    await ctx.boot();

    const needy = ctx.cardOf(NEEDY)!;
    expect(needy.textContent).toContain("Needs Ollama ≥ 0.40.13");
    expect(modelBox(needy)!.disabled).toBe(true);
    const update = needy.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!;
    expect(update.textContent).toBe("Update runtime to 0.40.13");
    expect(update.dataset.version).toBe("0.40.13");
    expect(needy.textContent).toContain("too old");

    // Compatible model: no chip (the chip is for a real conflict only), tick box stays on, no runtime button.
    const fine = ctx.cardOf("qwen3.5:4b")!;
    expect(fine.textContent).not.toContain("Needs Ollama");
    expect(modelBox(fine)!.disabled).toBe(false);
    expect(fine.querySelector('button[data-action="update-runtime"]')).toBeNull();
    // The blocked box says why.
    expect(modelBox(needy)!.title).toContain("update the runtime first");
  });

  it("asks the backend once for all candidate library models (never Hugging Face refs)", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    await ctx.boot();
    const requested = ctx.callsOf("check_ollama_model_requirements");
    expect(requested).toHaveLength(1);
    const names = requested[0].args!.models as string[];
    expect(names).toContain(NEEDY);
    expect(names).not.toContain(CURRENT); // installed
    expect(names.some((name) => name.startsWith("hf.co/"))).toBe(false);
  });

  it("stays silent when the requirement check fails: no chip, no toast, pull still possible", async () => {
    const ctx = await load(
      handlers({
        check_ollama_model_requirements: () => {
          throw "strict local mode";
        },
      })
    );
    await ctx.boot();
    expect(ctx.cardOf(NEEDY)!.textContent).not.toContain("Needs Ollama");
    expect(ctx.cardOf(NEEDY)!.querySelector<HTMLInputElement>('input[data-role="select"]')!.disabled).toBe(false);
    expect(ctx.showToast).not.toHaveBeenCalled();
  });

  it("one click: sets the target, saves, downloads + installs, and the gate disappears afterwards", async () => {
    const ctx = await load(handlers());
    const catalogEvent = vi.fn();
    window.addEventListener(ctx.mod.RUNTIME_CATALOG_UPDATED_EVENT, catalogEvent);
    await ctx.boot();

    ctx.cardOf(NEEDY)!.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!.click();
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Local runtime installed"));

    const saves = ctx.callsOf("save_settings");
    expect(saves.some((call) => (call.args as any).settings.providers.ollama.runtime_target_version === "0.40.13")).toBe(true);
    expect(ctx.callsOf("download_ollama_runtime")[0].args).toEqual({ version: "0.40.13" });
    expect(ctx.callsOf("install_ollama_runtime")).toHaveLength(1);
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(1);
    expect(catalogEvent).toHaveBeenCalled();
    window.removeEventListener(ctx.mod.RUNTIME_CATALOG_UPDATED_EVENT, catalogEvent);

    await ctx.render();
    const needy = ctx.cardOf(NEEDY)!;
    expect(modelBox(needy)!.disabled).toBe(false);
    expect(needy.querySelector('button[data-action="update-runtime"]')).toBeNull();
    expect(needy.textContent).not.toContain("Needs Ollama"); // no conflict any more: no chip
  });

  it("says 'Switch runtime to X' and does not download when that runtime is already on disk", async () => {
    catalogState = [
      runtime("0.40.7", { source: "pinned", recommended: true, installed: true }),
      runtime("0.40.13", { latest: true, installed: true }),
    ];
    const ctx = await load(
      handlers({
        start_ollama_runtime: () => {
          version.current = "0.40.13";
          return { pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false };
        },
      })
    );
    await ctx.boot();
    const button = ctx.cardOf(NEEDY)!.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!;
    expect(button.textContent).toBe("Switch runtime to 0.40.13");

    button.click();
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Runtime switched"));
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(0);
    expect(ctx.callsOf("install_ollama_runtime")).toHaveLength(0);
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(1);
  });

  it("no suitable verified runtime: a hint instead of a button", async () => {
    catalogState = [runtime("0.40.7", { source: "pinned", recommended: true, installed: true })];
    const ctx = await load(handlers());
    await ctx.boot();
    const needy = ctx.cardOf(NEEDY)!;
    expect(needy.querySelector('button[data-action="update-runtime"]')).toBeNull();
    expect(modelBox(needy)!.disabled).toBe(true);
    expect(needy.textContent).toContain("No verified runtime ≥ 0.40.13");
  });

  it("refuses to restart Ollama while a download or model update runs", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    ctx.state.ollamaPullProgress.set("qwen3.5:4b", { model: "qwen3.5:4b", status: "pulling x" });
    await ctx.mod.updateOllamaRuntimeTo("0.40.13");
    expect(toastTitles(ctx.showToast)).toContain("Runtime change postponed");
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(0);
    expect(ctx.callsOf("save_settings").some((call) => (call.args as any).settings.providers.ollama.runtime_target_version === "0.40.13")).toBe(false);
  });
});

describe("pull error 412 maps to the same hint", () => {
  const version = { current: "0.40.7" };
  const catalog = () => [
    runtime("0.40.7", { source: "pinned", recommended: true, installed: true }),
    runtime("0.40.13", { latest: true }),
  ];
  const base = (extra: Record<string, Handler> = {}) => ({
    ...runtimeHandlers(version, catalog),
    fetch_ollama_models_with_size: () => [{ name: CURRENT, size_bytes: 1, digest: NEW_DIGEST }] as InstalledRow[],
    ...extra,
  });

  it("a streamed 412 error turns the card into 'Needs a newer Ollama' + update button", async () => {
    const ctx = await load(base());
    await ctx.boot();
    expect(ctx.cardOf(NEEDY)!.querySelector<HTMLInputElement>('input[data-role="select"]')!.disabled).toBe(false);

    ctx.emit("ollama:pull-error", { model: NEEDY, error: STREAMED_412 });
    await flush();
    const card = ctx.cardOf(NEEDY)!;
    expect(card.textContent).toContain("Needs a newer Ollama");
    expect(card.querySelector<HTMLInputElement>('input[data-role="select"]')!.disabled).toBe(true);
    expect(card.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!.textContent).toBe(
      "Update runtime to 0.40.13"
    );
  });

  it("a newer runtime's 412 text names the version itself: no registry lookup needed", async () => {
    const ctx = await load(base());
    await ctx.boot();
    const lookups = ctx.callsOf("check_ollama_model_requirements").length;

    ctx.emit("ollama:pull-error", {
      model: NEEDY,
      error: "model requires ollama version v0.40.13 or newer (current version is v0.40.7)",
    });
    await flush();
    const card = ctx.cardOf(NEEDY)!;
    expect(card.textContent).toContain("Needs Ollama ≥ 0.40.13");
    expect(card.textContent).not.toContain("Needs a newer Ollama");
    expect(card.querySelector<HTMLInputElement>('input[data-role="select"]')!.disabled).toBe(true);
    expect(card.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!.textContent).toBe(
      "Update runtime to 0.40.13"
    );
    expect(ctx.callsOf("check_ollama_model_requirements")).toHaveLength(lookups);
  });

  it("an older runtime's 412 text has no version: the registry check is asked for exactly that model", async () => {
    const ctx = await load(base());
    await ctx.boot();
    ctx.emit("ollama:pull-error", { model: NEEDY, error: STREAMED_412 });
    await flush();
    const lookups = ctx.callsOf("check_ollama_model_requirements");
    expect(lookups[lookups.length - 1].args).toEqual({ models: [NEEDY] });
  });

  it("names the version once the registry check answers", async () => {
    const ctx = await load(
      base({
        check_ollama_model_requirements: (args) =>
          ((args?.models as string[]) ?? []).includes(NEEDY)
            ? [{ name: NEEDY, requires: "0.40.13", runtime_version: "0.40.7", compatible: false }]
            : [],
      })
    );
    ctx.emit("ollama:pull-error", { model: NEEDY, error: STREAMED_412 });
    await ctx.boot();
    ctx.emit("ollama:pull-error", { model: NEEDY, error: STREAMED_412 });
    await flush();
    expect(ctx.cardOf(NEEDY)!.textContent).toContain("Needs Ollama ≥ 0.40.13");
  });

  it("the backend's refusal on click shows a toast with the one-click fix and keeps the card usable", async () => {
    const ctx = await load(
      base({
        pull_ollama_model: () => {
          throw NEEDY_REFUSAL;
        },
      })
    );
    await ctx.boot();
    downloadSelected(ctx.cardOf(NEEDY));
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Runtime too old for this model"));

    const toast = toastNamed(ctx.showToast, "Runtime too old for this model");
    expect(toast.message).toBe(NEEDY_REFUSAL);
    expect(toast.actionLabel).toBe("Update runtime to 0.40.13");
    expect(toastTitles(ctx.showToast)).not.toContain("Download failed");

    await flush();
    expect(ctx.cardOf(NEEDY)!.textContent).toContain("Needs Ollama ≥ 0.40.13");
  });

  it("other pull failures keep the plain 'Download failed' toast", async () => {
    const ctx = await load(
      base({
        pull_ollama_model: () => {
          throw "Connection failed: refused";
        },
      })
    );
    await ctx.boot();
    downloadSelected(ctx.cardOf(NEEDY));
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Download failed"));
    await flush();
    expect(ctx.cardOf(NEEDY)!.textContent).not.toMatch(/Needs (Ollama|a newer)/);
  });

  it("the refusal is forgotten once a different runtime is running", async () => {
    const ctx = await load(base());
    await ctx.boot();
    ctx.emit("ollama:pull-error", { model: NEEDY, error: STREAMED_412 });
    await flush();
    expect(ctx.cardOf(NEEDY)!.textContent).toContain("Needs a newer Ollama");

    version.current = "0.40.13";
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    await ctx.render();
    expect(ctx.cardOf(NEEDY)!.textContent).not.toMatch(/Needs (Ollama|a newer)/);
    expect(ctx.cardOf(NEEDY)!.querySelector<HTMLInputElement>('input[data-role="select"]')!.disabled).toBe(false);
    version.current = "0.40.7";
  });
});

describe("runtime main action: Switch to X / Install X", () => {
  const version = { current: "0.40.7" };
  let catalogState: OllamaRuntimeVersionInfo[];
  beforeEach(() => {
    version.current = "0.40.7";
  });

  const handlers = (extra: Record<string, Handler> = {}) => ({
    ...runtimeHandlers(version, () => catalogState),
    fetch_ollama_models_with_size: () => [] as InstalledRow[],
    download_ollama_runtime: () => ({ sha256_ok: true, archive_path: "a.zip", version: "0.40.13" }),
    install_ollama_runtime: () => {
      version.current = "0.40.13";
      return { runtime_path: "x", version: "0.40.13" };
    },
    ...extra,
  });

  it("is 'Install X' while the target is not on disk and 'Switch to X' once it is", async () => {
    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    expect(ctx.mod.getOllamaRuntimeCardState()).toMatchObject({ primaryLabel: "Install 0.40.13", primaryAction: "install", primaryDisabled: false });

    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    expect(ctx.mod.getOllamaRuntimeCardState()).toMatchObject({ primaryLabel: "Switch to 0.40.13", primaryAction: "install", primaryDisabled: false });
  });

  it("trusts the file system over the catalog: no 'Switch to X' when the folder is missing", async () => {
    // Catalog says installed (settings recorded that version), detect says the folder is gone.
    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      handlers({
        detect_ollama_runtime: () => ({
          found: true,
          is_serving: true,
          source: "per_user_zip",
          path: "C:/ollama/ollama.exe",
          version: version.current,
          managed_alive: true,
          target_installed: false,
        }),
      }),
      { target: "0.40.13" }
    );
    await ctx.boot();
    expect(ctx.mod.getOllamaRuntimeCardState()).toMatchObject({ primaryLabel: "Install 0.40.13", primaryAction: "install" });
  });

  it("ensureLocalRuntimeReady skips download and install for a target that is on disk", async () => {
    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      handlers({
        start_ollama_runtime: () => {
          version.current = "0.40.13";
          return { pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false };
        },
      }),
      { target: "0.40.13" }
    );
    await ctx.boot();
    await ctx.mod.ensureLocalRuntimeReady();

    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(0);
    expect(ctx.callsOf("install_ollama_runtime")).toHaveLength(0);
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(1);
    expect(toastTitles(ctx.showToast)).toContain("Runtime switched");
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Runtime ready");
  });

  it("does not claim a switch that did not happen", async () => {
    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(handlers(), { target: "0.40.13" }); // start does not change the version
    await ctx.boot();
    await ctx.mod.ensureLocalRuntimeReady();
    expect(toastTitles(ctx.showToast)).toContain("Runtime not switched");
    expect(toastTitles(ctx.showToast)).not.toContain("Runtime switched");
  });

  it("still downloads and installs a target that is not on disk", async () => {
    catalogState = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    await ctx.mod.ensureLocalRuntimeReady();
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(1);
    expect(ctx.callsOf("install_ollama_runtime")).toHaveLength(1);
    expect(toastTitles(ctx.showToast)).toContain("Local runtime installed");
  });
});

describe("safe model update (backup -> pull -> check -> finalize)", () => {
  const version = { current: "0.40.7" };
  const base = (extra: Record<string, Handler> = {}) => ({
    ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { latest: true })]),
    fetch_ollama_models_with_size: () => [{ name: OLD, size_bytes: 9_608_350_718, digest: OLD_DIGEST }] as InstalledRow[],
    check_ollama_model_updates: (args?: Record<string, unknown>) =>
      ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({
        name: model.name,
        status: "outdated",
        remote_digest: NEW_DIGEST,
        remote_size_bytes: 6_583_658_527,
      })),
    backup_ollama_model: () => `${OLD}.bak-1`,
    pull_ollama_model: () => undefined,
    finalize_ollama_model_update: () => ({ verified: true, restored: false, detail: null }),
    ...extra,
  });
  const names = (calls: Array<{ command: string }>) =>
    calls.map((call) => call.command).filter((command) =>
      ["backup_ollama_model", "pull_ollama_model", "finalize_ollama_model_update"].includes(command)
    );

  it("offers 'Update' on an outdated model and explains the safety rule before doing anything", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    const card = ctx.cardOf(OLD)!;
    expect(card.textContent).toContain("Outdated");
    const update = card.querySelector<HTMLButtonElement>('button[data-action="update"]')!;
    expect(update.textContent).toBe("Update");

    update.click();
    await flush();
    const text = await ctx.confirmDialog("cancel");
    expect(text).toContain("6.6 GB");
    expect(text).toContain("only replaced after the new build passed a functional check");
    expect(text).toContain("restored");
    expect(names(ctx.calls)).toEqual([]); // cancel changes nothing
  });

  it("runs in the mandatory order and only finalizes after the pull finished", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");

    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    expect(names(ctx.calls)).toEqual(["backup_ollama_model", "pull_ollama_model"]);
    expect(ctx.callsOf("backup_ollama_model")[0].args).toEqual({ model: OLD });
    expect(ctx.callsOf("pull_ollama_model")[0].args).toEqual({ model: OLD });

    // While the download runs the card shows the operation and locks the other buttons.
    ctx.emit("ollama:pull-progress", { model: OLD, status: "pulling 8b1d2e3f4a5b", digest: "sha256:aa", total: 1000, completed: 250 });
    await flush();
    const busy = ctx.cardOf(OLD)!;
    expect(busy.textContent).toContain("Updating");
    expect(busy.querySelector('button[data-action="update"]')).toBeNull();
    expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(0);
    expect(ctx.mod.isOllamaModelOperationRunning(OLD)).toBe(true);
    expect(ctx.mod.isOllamaModelOperationRunning("qwen3.5:4b")).toBe(false);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(true);

    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(1));
    expect(ctx.callsOf("finalize_ollama_model_update")[0].args).toEqual({ model: OLD, backup: `${OLD}.bak-1` });
    expect(names(ctx.calls)).toEqual(["backup_ollama_model", "pull_ollama_model", "finalize_ollama_model_update"]);

    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
    expect(toastTitles(ctx.showToast)).not.toContain("Update rolled back");
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
  });

  it("reports a rollback when the new build fails its check", async () => {
    const ctx = await load(
      base({
        finalize_ollama_model_update: () => ({ verified: false, restored: true, detail: "Output missed 4 of 12 words." }),
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: OLD });

    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update rolled back"));
    const toast = toastNamed(ctx.showToast, "Update rolled back");
    expect(toast.message).toContain("previous build was restored");
    expect(toast.message).toContain("Output missed 4 of 12 words.");
    expect(toastTitles(ctx.showToast)).not.toContain("Model updated");
  });

  it("a failed check that could not restore names the backup copy", async () => {
    const ctx = await load(
      base({ finalize_ollama_model_update: () => ({ verified: false, restored: false, detail: null }) }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update check failed"));
    const toast = toastNamed(ctx.showToast, "Update check failed");
    expect(toast.message).toContain(`${OLD}.bak-1`);
  });

  it("a failed download still goes through finalize, which drops the backup", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-error", { model: OLD, error: "Connection failed: reset" });

    await vi.waitFor(() => expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(1));
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update failed"));
    const toast = toastNamed(ctx.showToast, "Update failed");
    expect(toast.message).toContain("Connection failed: reset");
    expect(toast.message).toContain("is unchanged");
  });

  it("does not start the download when the backup fails", async () => {
    const ctx = await load(
      base({
        backup_ollama_model: () => {
          throw "copy failed";
        },
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update not started"));
    expect(names(ctx.calls)).toEqual(["backup_ollama_model"]);
    await flush();
    expect(ctx.cardOf(OLD)!.querySelector('button[data-action="update"]')).not.toBeNull(); // operation cleared
  });

  it("retries finalize while the backend still counts the finished pull as running, then succeeds", async () => {
    let attempts = 0;
    const ctx = await load(
      base({
        finalize_ollama_model_update: () => {
          attempts += 1;
          if (attempts < 3) throw `A pull of '${OLD}' is still running.`;
          return { verified: true, restored: false, detail: null };
        },
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"), { timeout: 4000 });
    expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(3);
    expect(toastTitles(ctx.showToast)).not.toContain("Update could not be finished");
  });

  it("a finalize error keeps the backup and says so", async () => {
    const ctx = await load(
      base({
        finalize_ollama_model_update: () => {
          throw "model busy";
        },
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update could not be finished"));
    const toast = toastNamed(ctx.showToast, "Update could not be finished");
    expect(toast.message).toContain(`"${OLD}.bak-1" was kept`);
  });

  it("an update that needs a newer runtime offers the runtime fix instead of 'Update'", async () => {
    const ctx = await load(
      base({
        check_ollama_model_requirements: (args) =>
          ((args?.models as string[]) ?? []).map((name) => ({
            name,
            requires: "0.40.13",
            runtime_version: "0.40.7",
            compatible: false,
          })),
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    const card = ctx.cardOf(OLD)!;
    expect(card.querySelector('button[data-action="update"]')).toBeNull();
    expect(card.textContent).toContain("Needs Ollama ≥ 0.40.13");
    expect(card.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')!.textContent).toBe(
      "Update runtime to 0.40.13"
    );
    expect(names(ctx.calls)).toEqual([]);
  });

  it("locks update/replace/activate/uninstall on other models while one operation runs", async () => {
    const ctx = await load(
      base({
        fetch_ollama_models_with_size: () => [
          { name: OLD, size_bytes: 9_608_350_718, digest: OLD_DIGEST },
          { name: "qwen3.5:4b", size_bytes: 3_400_000_000, digest: "f".repeat(64) },
        ],
        check_ollama_model_updates: (args?: Record<string, unknown>) =>
          ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({
            name: model.name,
            status: model.name === OLD ? "outdated" : "current",
            remote_digest: model.name === OLD ? NEW_DIGEST : "f".repeat(64),
          })),
      }),
      { settingsModel: "qwen3.5:4b" }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    await flush();

    const other = ctx.cardOf("qwen3.5:4b")!;
    expect(other.querySelector<HTMLButtonElement>('button[data-action="update-runtime"]')).toBeNull();
    const operating = ctx.cardOf(OLD)!;
    expect(operating.querySelector<HTMLButtonElement>('button[data-action="activate"]')!.disabled).toBe(true);
    expect(operating.querySelector<HTMLButtonElement>('button[data-action="delete"]')!.disabled).toBe(true);
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(1));
  });
});

describe("replace a duplicate with the current build", () => {
  const version = { current: "0.40.7" };
  const base = (extra: Record<string, Handler> = {}) => ({
    ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
    fetch_ollama_models_with_size: () =>
      [
        { name: OLD, size_bytes: 9_608_350_718, digest: OLD_DIGEST },
        { name: CURRENT, size_bytes: 6_583_658_527, digest: NEW_DIGEST },
      ] as InstalledRow[],
    check_ollama_model_updates: (args?: Record<string, unknown>) =>
      ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({
        name: model.name,
        status: model.name === OLD ? "outdated" : "current",
        remote_digest: NEW_DIGEST,
        remote_size_bytes: 6_583_658_527,
      })),
    replace_ollama_model: () => ({ verified: true, switched_active: true, deleted_old: true, latency_ms: 812, detail: null }),
    ...extra,
  });

  it("offers 'Replace with <current>' instead of 'Update' for the outdated twin", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    const old = ctx.cardOf(OLD)!;
    expect(old.querySelector('button[data-action="update"]')).toBeNull();
    const replace = old.querySelector<HTMLButtonElement>('button[data-action="replace"]')!;
    expect(replace.textContent).toBe(`Replace with ${CURRENT}`);
    expect(replace.dataset.target).toBe(CURRENT);
    expect(old.textContent).toContain(`already installed as ${CURRENT}`);
    expect(ctx.cardOf(CURRENT)!.querySelector('button[data-action="replace"]')).toBeNull();
  });

  it("confirms, calls replace_ollama_model, and mirrors the automatic switch of the active model", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    await flush();
    const text = await ctx.confirmDialog("confirm");
    expect(text).toContain("checked first");
    expect(text).toContain("Only if it passes");
    expect(text).toContain("active model switches");
    expect(text).toContain("frees up to 9.6 GB");

    await vi.waitFor(() => expect(ctx.callsOf("replace_ollama_model")).toHaveLength(1));
    expect(ctx.callsOf("replace_ollama_model")[0].args).toEqual({ oldModel: OLD, newModel: CURRENT });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model replaced"));
    expect(ctx.state.settings!.ai_fallback.model).toBe(CURRENT);
    expect(ctx.state.settings!.postproc_llm_model).toBe(CURRENT);
    expect(ctx.state.settings!.providers.ollama.preferred_model).toBe(CURRENT);
  });

  it("cancel changes nothing", async () => {
    const ctx = await load(base(), { settingsModel: OLD });
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    await flush();
    await ctx.confirmDialog("cancel");
    expect(ctx.callsOf("replace_ollama_model")).toHaveLength(0);
    expect(ctx.state.settings!.ai_fallback.model).toBe(OLD);
  });

  it("a failed check keeps everything as it was and says why", async () => {
    const ctx = await load(
      base({
        replace_ollama_model: () => ({ verified: false, switched_active: false, deleted_old: false, latency_ms: 0, detail: "Empty answer." }),
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Nothing replaced"));
    const toast = toastNamed(ctx.showToast, "Nothing replaced");
    expect(toast.message).toContain("Empty answer.");
    expect(ctx.state.settings!.ai_fallback.model).toBe(OLD);
  });

  it("a verified switch whose cleanup failed is reported as a warning", async () => {
    const ctx = await load(
      base({
        replace_ollama_model: () => ({ verified: true, switched_active: true, deleted_old: false, latency_ms: 800, detail: "file in use" }),
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    await flush();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Replaced, old build still installed"));
    expect(ctx.state.settings!.ai_fallback.model).toBe(CURRENT);
  });

  it("only mentions the active switch when the old model was active", async () => {
    const ctx = await load(base(), { settingsModel: CURRENT });
    await ctx.boot();
    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    await flush();
    const text = await ctx.confirmDialog("cancel");
    expect(text).not.toContain("active model switches");
  });
});

describe("pull progress per layer in the catalog card", () => {
  const base = () => ({
    ...runtimeHandlers({ current: "0.40.7" }, () => [runtime("0.40.7", { installed: true, recommended: true })]),
    fetch_ollama_models_with_size: () => [] as InstalledRow[],
  });
  const fill = (card: HTMLElement) => card.querySelector<HTMLElement>(".qwen-card-progress-fill")!.style.width;

  it("keeps one overall bar across layers instead of jumping to 0 on every layer change", async () => {
    const ctx = await load(base(), { settingsModel: CURRENT });
    await ctx.boot();
    const model = "qwen3.5:4b"; // catalog size 3.32 GB
    const push = (payload: Record<string, unknown>) => {
      ctx.state.ollamaPullProgress.set(model, { model, ...payload } as never);
      ctx.emit("ollama:pull-progress", { model, ...payload });
    };

    push({ status: "pulling manifest" });
    await flush();
    expect(fill(ctx.cardOf(model)!)).toBe("0%");

    push({ status: "pulling aaaa", digest: "sha256:aaaa", total: 3_000_000_000 });
    push({ status: "pulling aaaa", digest: "sha256:aaaa", total: 3_000_000_000, completed: 1_500_000_000 });
    await flush();
    const half = ctx.cardOf(model)!;
    expect(fill(half)).toBe("45%");
    expect(half.textContent).toContain("45% (1.5 GB / 3.3 GB)");

    // Next layer's first line has no `completed`: the old display showed 0 here.
    push({ status: "pulling bbbb", digest: "sha256:bbbb", total: 300_000_000 });
    await flush();
    expect(fill(ctx.cardOf(model)!)).toBe("45%");

    push({ status: "pulling aaaa", digest: "sha256:aaaa", total: 3_000_000_000, completed: 3_000_000_000 });
    await flush();
    expect(fill(ctx.cardOf(model)!)).toBe("90%");

    push({ status: "verifying sha256 digest" });
    await flush();
    expect(fill(ctx.cardOf(model)!)).toBe("100%");
  });

  it("clears the layer state when the pull ends", async () => {
    const ctx = await load(base(), { settingsModel: CURRENT });
    await ctx.boot();
    const model = "qwen3.5:4b";
    ctx.emit("ollama:pull-progress", { model, status: "pulling aaaa", digest: "sha256:aaaa", total: 1000, completed: 500 });
    expect(ctx.mod.getOllamaPullOverall(model)?.completed).toBe(500);
    ctx.emit("ollama:pull-complete", { model });
    expect(ctx.mod.getOllamaPullOverall(model)).toBeNull();
    ctx.emit("ollama:pull-progress", { model, status: "pulling aaaa", digest: "sha256:aaaa", total: 1000, completed: 200 });
    ctx.emit("ollama:pull-error", { model, error: "boom" });
    expect(ctx.mod.getOllamaPullOverall(model)).toBeNull();
  });

  it("computeOllamaPercent (used by the download popup) follows the overall progress", async () => {
    const ctx = await load(base(), { settingsModel: CURRENT });
    await ctx.boot();
    const model = "qwen3.5:4b";
    ctx.emit("ollama:pull-progress", { model, status: "pulling a", digest: "sha256:a", total: 3_000_000_000, completed: 3_000_000_000 });
    ctx.emit("ollama:pull-progress", { model, status: "pulling b", digest: "sha256:b", total: 320_000_000 });
    // newest line alone says 0 %, the model is at 90 %
    expect(ctx.mod.computeOllamaPercent({ model, status: "pulling b", digest: "sha256:b", total: 320_000_000 })).toBe(90);
    // untracked model: falls back to the single line
    expect(ctx.mod.computeOllamaPercent({ model: "other", status: "x", total: 100, completed: 50 })).toBe(50);
  });
});

describe("'Verified working' chip", () => {
  const catalog = [
    {
      name: "gemma4:e4b",
      label: "Gemma 4 E4B",
      source: "ollama",
      family: "gemma4",
      status: "recommended",
      profile: "Recommended",
      size_gb: 6.58,
      description: "Proven.",
      functional: { verified_at: "2026-11-02", runtime: "0.40.13", latency_p50_ms: 840, note: "12 German sentences." },
    },
    {
      name: "qwen3.5:4b",
      label: "Qwen 3.5 4B",
      source: "ollama",
      family: "qwen3.5",
      status: "supported",
      profile: "x",
      size_gb: 3.32,
      description: "Unproven.",
      functional: { verified_at: "yesterday", runtime: "0.40.13", latency_p50_ms: 840 },
    },
  ];
  const base = (models: InstalledRow[]) => ({
    ...runtimeHandlers({ current: "0.40.7" }, () => [runtime("0.40.7", { installed: true, recommended: true })]),
    fetch_ollama_models_with_size: () => models,
  });
  afterEach(() => {
    vi.doUnmock("../model-catalog.json");
  });

  it("shows on catalog cards with the proof as tooltip, and not for a malformed proof", async () => {
    const ctx = await load(base([]), { catalogJson: catalog, settingsModel: "something-else" });
    await ctx.boot();
    const chip = ctx.cardOf("gemma4:e4b")!.querySelector<HTMLElement>(".qwen-card-badge--functional")!;
    expect(chip.textContent).toBe("Verified working");
    expect(chip.title).toBe(
      "Passed a functional check on Ollama 0.40.13 (2 Nov 2026): typical response 840 ms. 12 German sentences."
    );
    expect(ctx.cardOf("qwen3.5:4b")!.querySelector(".qwen-card-badge--functional")).toBeNull();
  });

  it("shows on the installed card of that model too", async () => {
    const ctx = await load(base([{ name: "gemma4:e4b", size_bytes: 6_583_658_527, digest: NEW_DIGEST }]), {
      catalogJson: catalog,
    });
    await ctx.boot();
    expect(ctx.cardOf("gemma4:e4b")!.querySelector(".qwen-card-badge--functional")?.textContent).toBe("Verified working");
  });
});
