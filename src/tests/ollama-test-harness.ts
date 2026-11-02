/**
 * Shared harness of the Ollama reliability tests: loads a fresh `ollama-models` module against a
 * mocked backend (`invoke`) and event bridge (`listen`). The test file that uses it must mock
 * `@tauri-apps/plugin-dialog` and `../toast` itself (vi.mock is hoisted per file).
 */
import { expect, vi, type Mock } from "vitest";
import type { OllamaRuntimeVersionInfo } from "../types";

export type Handler = (args?: Record<string, unknown>) => unknown;
export type Listener = (event: { payload: unknown }) => void;
export type InstalledRow = { name: string; size_bytes: number; digest?: string } & Record<string, unknown>;
export type ToastArgs = {
  type?: string;
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => unknown;
};

export const OLD_DIGEST = "c6eb396dbd5992bbe3f5cdb947e8bbc0ee413d7c17e2beaae69f5d569cf982eb";
export const NEW_DIGEST = "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c";
export const OLD = "gemma4:e4b-it-q4_K_M";
export const CURRENT = "gemma4:e4b";

export function runtime(version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo {
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

/** Real timers: let queued renders (16 ms fallback frame) and promise chains run. */
export const flush = () => new Promise((resolve) => setTimeout(resolve, 70));

/** Fake clock for timers and Date only (rAF/microtasks stay real). */
export const useFakeClock = () =>
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });

/** Fake timers: advance the clock and let promise chains run. */
export const tick = (ms: number) => vi.advanceTimersByTimeAsync(ms);

export const settle = () => (vi.isFakeTimers() ? tick(100) : flush());

export type LoadOptions = {
  catalogJson?: unknown;
  settingsModel?: string;
  target?: string;
  source?: string;
  /** `null`: the app has not loaded its settings yet. */
  settings?: null;
};

export async function load(handlers: Record<string, Handler> = {}, options: LoadOptions = {}) {
  vi.resetModules();
  if (options.catalogJson) vi.doMock("../model-catalog.json", () => ({ default: options.catalogJson }));
  else vi.doUnmock("../model-catalog.json");
  document.body.innerHTML = '<div id="ollama-model-manager"></div><div id="toast-container"></div>';

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
  const unlistened: string[] = [];
  vi.mocked(listen).mockImplementation((async (event: string, callback: Listener) => {
    listeners.set(event, [...(listeners.get(event) ?? []), callback]);
    return () => {
      unlistened.push(event);
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((entry) => entry !== callback)
      );
    };
  }) as never);
  const emit = (event: string, payload: unknown) => {
    for (const callback of [...(listeners.get(event) ?? [])]) callback({ payload });
  };

  const model = options.settingsModel ?? CURRENT;
  if (options.settings === null) {
    state.setSettings(null);
  } else {
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
  }

  const callsOf = (command: string) => calls.filter((call) => call.command === command);
  const container = () => document.getElementById("ollama-model-manager")!;
  const render = async () => {
    mod.renderOllamaModelManager();
    await settle();
  };
  const cardOf = (name: string) =>
    Array.from(container().querySelectorAll<HTMLElement>(".qwen-model-card")).find(
      (card) => card.dataset.model === name || card.querySelector(".qwen-card-tag")?.textContent === name
    );
  const boot = async () => {
    await mod.refreshOllamaRuntimeState({ force: true });
    await mod.refreshOllamaInstalledModels();
    await mod.refreshOllamaModelUpdateStatuses();
    await mod.refreshOllamaModelRequirements();
    await render();
  };
  const dialogs = () => document.querySelectorAll<HTMLElement>(".ollama-confirm-dialog");
  const confirmDialog = async (action: "confirm" | "cancel") => {
    const dialog = document.querySelector<HTMLElement>(".ollama-confirm-dialog");
    expect(dialog).not.toBeNull();
    const text = dialog!.textContent ?? "";
    dialog!.querySelector<HTMLButtonElement>(`button[data-dialog-action="${action}"]`)!.click();
    await settle();
    return text;
  };

  return {
    mod,
    state,
    invoke,
    showToast,
    calls,
    callsOf,
    emit,
    listeners,
    unlistened,
    container,
    render,
    cardOf,
    boot,
    dialogs,
    confirmDialog,
  };
}

export type Ctx = Awaited<ReturnType<typeof load>>;

export function runtimeHandlers(
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

export const toastTitles = (showToast: Mock<(options: ToastArgs) => unknown>) =>
  showToast.mock.calls.map((call) => call[0].title);

export const toastNamed = (showToast: Mock<(options: ToastArgs) => unknown>, title: string): ToastArgs => {
  const found = showToast.mock.calls.map((call) => call[0]).find((toast) => toast.title === title);
  if (!found) throw new Error(`no toast titled "${title}" (got: ${toastTitles(showToast).join(", ")})`);
  return found;
};

/** A promise that never settles: a backend call that hangs. */
export const hang = () => new Promise<never>(() => {});

/** Handlers for one outdated model (OLD) that can be updated in place. */
export function updateHandlers(
  version: { current: string },
  extra: Record<string, Handler> = {}
): Record<string, Handler> {
  return {
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
  };
}

/** Click "Update" on OLD and confirm; resolves once the dialog is gone. */
export async function startUpdate(ctx: Ctx, model = OLD) {
  ctx.cardOf(model)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
  await settle();
  await ctx.confirmDialog("confirm");
}
