/**
 * Adversarial review of the Ollama/model manager frontend (task S2): races, double clicks, hanging
 * backends, listener and timer hygiene, degraded or malformed backend answers, runtime change state.
 * Every test here exists because the behaviour was provoked at the object first.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CURRENT,
  NEW_DIGEST,
  OLD,
  OLD_DIGEST,
  bulkButton,
  downloadSelected,
  flush,
  hang,
  load,
  modelBox,
  runtime,
  runtimeHandlers,
  settle,
  startUpdate,
  tick,
  toastNamed,
  toastTitles,
  updateHandlers,
  useFakeClock,
  type Ctx,
  type InstalledRow,
} from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

const MINUTE = 60_000;

const version = { current: "0.40.7" };
const NEEDY = "gemma4:e2b";

beforeEach(() => {
  version.current = "0.40.7";
});

afterEach(() => {
  vi.useRealTimers();
});

const updateButton = (ctx: Ctx) => ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!;

// ---------------------------------------------------------------------------------------------
// 1. Races and double clicks
// ---------------------------------------------------------------------------------------------

describe("update flow cannot leave the UI locked", () => {
  it("frees everything when the pull-complete event never arrives (stall watchdog)", async () => {
    useFakeClock();
    const ctx = await load(updateHandlers(version), { settingsModel: OLD });
    await ctx.boot();
    await startUpdate(ctx);
    await tick(1_000);
    expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(true);

    await tick(11 * MINUTE); // no progress, no completion, no error
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false);
    expect(toastTitles(ctx.showToast).some((title) => /^Update (failed|could not be finished)$/.test(title))).toBe(true);
    expect(ctx.cardOf(OLD)!.querySelector('button[data-action="update"]')).not.toBeNull();
    expect(ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="activate"]')?.disabled ?? false).toBe(false);
  });

  it("does not give up while progress keeps arriving, however long the download takes", async () => {
    useFakeClock();
    const ctx = await load(updateHandlers(version), { settingsModel: OLD });
    await ctx.boot();
    await startUpdate(ctx);
    await tick(1_000);

    for (let minute = 0; minute < 30; minute += 1) {
      ctx.emit("ollama:pull-progress", { model: OLD, status: "pulling aa", digest: "sha256:aa", total: 1000, completed: minute });
      await tick(MINUTE);
    }
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(true);
    expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(0);

    ctx.emit("ollama:pull-complete", { model: OLD });
    await tick(1_000);
    expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(1);
    expect(toastTitles(ctx.showToast)).toContain("Model updated");
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false);
  });

  it("leaves no timer behind once an update finished", async () => {
    useFakeClock();
    const ctx = await load(updateHandlers(version), { settingsModel: OLD });
    await ctx.boot();
    await startUpdate(ctx);
    await tick(1_000);
    ctx.emit("ollama:pull-progress", { model: OLD, status: "pulling aa", digest: "sha256:aa", total: 10, completed: 5 });
    ctx.emit("ollama:pull-complete", { model: OLD });
    await tick(2_000);
    expect(toastTitles(ctx.showToast)).toContain("Model updated");
    expect(vi.getTimerCount()).toBe(0);
  });

  const hangs: Array<{ name: string; command: string; flow: "update" | "replace"; title: RegExp }> = [
    { name: "backup_ollama_model", command: "backup_ollama_model", flow: "update", title: /^Update not started$/ },
    { name: "pull_ollama_model", command: "pull_ollama_model", flow: "update", title: /^Update (failed|could not be finished)$/ },
    { name: "finalize_ollama_model_update", command: "finalize_ollama_model_update", flow: "update", title: /^Update could not be finished$/ },
    { name: "replace_ollama_model", command: "replace_ollama_model", flow: "replace", title: /^Replace failed$/ },
  ];

  it.each(hangs)("a hanging $name is given up on and the UI is released", async ({ command, flow, title }) => {
    useFakeClock();
    const twin = [
      { name: OLD, size_bytes: 9_608_350_718, digest: OLD_DIGEST },
      { name: CURRENT, size_bytes: 6_583_658_527, digest: NEW_DIGEST },
    ] as InstalledRow[];
    const ctx = await load(
      updateHandlers(version, {
        [command]: hang,
        ...(flow === "replace"
          ? {
              fetch_ollama_models_with_size: () => twin,
              check_ollama_model_updates: (args?: Record<string, unknown>) =>
                ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({
                  name: model.name,
                  status: model.name === OLD ? "outdated" : "current",
                  remote_digest: NEW_DIGEST,
                })),
            }
          : {}),
      }),
      { settingsModel: CURRENT }
    );
    await ctx.boot();
    if (flow === "replace") {
      ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
      await settle();
      await ctx.confirmDialog("confirm");
    } else {
      await startUpdate(ctx);
    }
    await tick(1_000);
    if (command === "finalize_ollama_model_update") ctx.emit("ollama:pull-complete", { model: OLD });
    await tick(1_000);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(true);

    await tick(12 * MINUTE);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false);
    expect(toastTitles(ctx.showToast).some((value) => title.test(value))).toBe(true);
    await tick(1_000);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("double clicks", () => {
  it("two quick clicks on Update open one confirmation, not two", async () => {
    const ctx = await load(updateHandlers(version), { settingsModel: OLD });
    await ctx.boot();
    const button = updateButton(ctx);
    button.click();
    button.click();
    await settle();
    expect(ctx.dialogs()).toHaveLength(1);

    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    expect(ctx.callsOf("backup_ollama_model")).toHaveLength(1);
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
  });

  it("two quick clicks on Replace open one confirmation, not two", async () => {
    const ctx = await load(
      updateHandlers(version, {
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
          })),
        replace_ollama_model: () => ({ verified: true, switched_active: false, deleted_old: true, latency_ms: 5, detail: null }),
      }),
      { settingsModel: CURRENT }
    );
    await ctx.boot();
    const button = ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="replace"]')!;
    button.click();
    button.click();
    await settle();
    expect(ctx.dialogs()).toHaveLength(1);
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model replaced"));
    expect(ctx.callsOf("replace_ollama_model")).toHaveLength(1);
  });

  it("a confirmed update that finds the UI busy says so instead of silently doing nothing", async () => {
    const ctx = await load(updateHandlers(version, { start_ollama_runtime: hang }), { settingsModel: OLD });
    await ctx.boot();
    updateButton(ctx).click();
    await settle();
    // While the dialog is open a runtime action starts (and does not finish).
    void ctx.mod.ensureLocalRuntimeReady();
    await settle();
    await ctx.confirmDialog("confirm");
    await settle();
    expect(ctx.callsOf("backup_ollama_model")).toHaveLength(0);
    expect(toastTitles(ctx.showToast)).toContain("Update not started");
  });

  it("two quick clicks on 'Download selected' start one pull and the card keeps saying 'downloading' until the first event", async () => {
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
      pull_ollama_model: () => undefined, // the real command returns as soon as the pull is spawned
    });
    await ctx.boot();
    const model = "qwen3.5:4b";
    modelBox(ctx.cardOf(model))!.click();
    const bar = bulkButton()!;
    bar.click();
    bar.click();
    await settle();
    expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1);

    // Command returned, first progress line not here yet: still a running download, not a free button.
    expect(ctx.cardOf(model)!.dataset.state).toBe("downloading");
    expect(modelBox(ctx.cardOf(model))).toBeNull();

    ctx.state.ollamaPullProgress.set(model, { model, status: "pulling manifest" });
    ctx.emit("ollama:pull-progress", { model, status: "pulling manifest" });
    await settle();
    expect(ctx.cardOf(model)!.dataset.state).toBe("downloading");

    ctx.state.ollamaPullProgress.delete(model);
    ctx.emit("ollama:pull-complete", { model });
    await settle();
    expect(ctx.cardOf(model)!.dataset.state).toBe("available"); // not in the mocked inventory
  });

  it("a catalog download whose events never come frees the card again (stall watchdog)", async () => {
    useFakeClock();
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
      pull_ollama_model: () => undefined,
    });
    await ctx.boot();
    const model = "qwen3.5:4b";
    downloadSelected(ctx.cardOf(model));
    await tick(500);
    expect(ctx.cardOf(model)!.dataset.state).toBe("downloading");

    ctx.state.ollamaPullProgress.set(model, { model, status: "pulling manifest" });
    ctx.emit("ollama:pull-progress", { model, status: "pulling manifest" });
    await tick(11 * MINUTE);
    expect(ctx.cardOf(model)!.dataset.state).toBe("available");
    expect(ctx.state.ollamaPullProgress.has(model)).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("'Get versions' twice in a row asks GitHub once; a hung request is given up on", async () => {
    useFakeClock();
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_online_versions: hang,
    });
    const status = vi.fn();
    const first = ctx.mod.fetchOnlineVersionCatalog(status);
    const second = ctx.mod.fetchOnlineVersionCatalog(status);
    await tick(10);
    expect(ctx.callsOf("fetch_ollama_online_versions")).toHaveLength(1);
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(true);

    await tick(3 * MINUTE);
    await Promise.all([first, second]);
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(false);
    expect(status.mock.calls.map((call) => String(call[0])).join("|")).toContain("Could not fetch the version list");
  });

  it("'Update runtime' twice in a row downloads once and reports no failure", async () => {
    const catalog = [runtime("0.40.7", { source: "pinned", recommended: true, installed: true }), runtime("0.40.13", { latest: true })];
    const ctx = await load({
      ...runtimeHandlers(version, () => catalog),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
      download_ollama_runtime: () => ({ sha256_ok: true, archive_path: "a.zip", version: "0.40.13" }),
      install_ollama_runtime: () => ({ runtime_path: "x", version: "0.40.13" }),
    });
    await ctx.boot();
    await Promise.all([ctx.mod.updateOllamaRuntimeTo("0.40.13"), ctx.mod.updateOllamaRuntimeTo("0.40.13")]);
    await settle();
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(1);
    expect(ctx.callsOf("install_ollama_runtime")).toHaveLength(1);
    expect(toastTitles(ctx.showToast).filter((title) => /failed/i.test(title))).toEqual([]);
  });

  it("'Switch to X' twice in a row starts the runtime once", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      { ...runtimeHandlers(version, () => catalog), fetch_ollama_models_with_size: () => [] as InstalledRow[] },
      { target: "0.40.13" }
    );
    await ctx.boot();
    await Promise.all([ctx.mod.ensureLocalRuntimeReady(), ctx.mod.ensureLocalRuntimeReady()]);
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(1);
  });
});

describe("a runtime change never runs under a model operation", () => {
  it("the 'Switch to X' main action is refused while a model update runs", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      updateHandlers(version, { list_ollama_runtime_versions: () => catalog, fetch_ollama_online_versions: () => catalog }),
      { settingsModel: OLD, target: "0.40.13" }
    );
    await ctx.boot();
    await startUpdate(ctx);
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    const before = ctx.calls.length;

    await ctx.mod.ensureLocalRuntimeReady();
    await settle();

    const after = ctx.calls.slice(before).map((call) => call.command);
    expect(after).not.toContain("start_ollama_runtime");
    expect(after).not.toContain("download_ollama_runtime");
    expect(after).not.toContain("install_ollama_runtime");
    expect(toastTitles(ctx.showToast)).toContain("Runtime change postponed");

    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
  });

  it("is refused while a catalog download runs as well", async () => {
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
    });
    await ctx.boot();
    ctx.state.ollamaPullProgress.set("qwen3.5:4b", { model: "qwen3.5:4b", status: "pulling x" });
    await ctx.mod.ensureLocalRuntimeReady();
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(0);
    expect(toastTitles(ctx.showToast)).toContain("Runtime change postponed");
  });

  it("'Download selected' is disabled while the runtime is being changed and starts nothing", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => catalog),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        download_ollama_runtime: hang,
      },
      { target: "0.40.13" }
    );
    await ctx.boot();
    void ctx.mod.ensureLocalRuntimeReady();
    await settle();
    await ctx.render();
    // The box can still be ticked (the choice is not lost), the button waits for the runtime.
    modelBox(ctx.cardOf("qwen3.5:4b"))!.click();
    const bar = bulkButton()!;
    expect(bar.disabled).toBe(true);
    expect(bar.title).toContain("runtime");
    bar.click();
    await settle();
    expect(ctx.callsOf("pull_ollama_model")).toHaveLength(0);
  });

  it.each([
    ["the old runtime is still serving", true],
    ["the old runtime is down", false],
  ])("does not release the busy flag in the middle of a long runtime download that keeps reporting progress (%s)", async (_label, serving) => {
    useFakeClock();
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => catalog),
        detect_ollama_runtime: () => ({ found: true, is_serving: serving, source: "per_user_zip", path: "p", version: "0.40.7" }),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        download_ollama_runtime: hang,
      },
      { target: "0.40.13" }
    );
    await ctx.boot();
    void ctx.mod.ensureLocalRuntimeReady();
    await tick(1_000);
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(1);

    for (let i = 0; i < 14; i += 1) {
      ctx.mod.setOllamaRuntimeInstallProgress({ stage: "download_runtime", message: "Downloading runtime", downloaded: i, total: 100, version: "0.40.13" });
      await tick(5_000);
      ctx.mod.getOllamaRuntimeCardState(); // what every render does
    }
    void ctx.mod.ensureLocalRuntimeReady();
    await tick(1_000);
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(1);
    if (!serving) expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(true);
  });

  it("an outdated model cannot be updated while the runtime is being switched, even though the old runtime still answers", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(
      updateHandlers(version, {
        list_ollama_runtime_versions: () => catalog,
        fetch_ollama_online_versions: () => catalog,
        download_ollama_runtime: hang,
      }),
      { settingsModel: OLD, target: "0.40.13" }
    );
    await ctx.boot();
    void ctx.mod.ensureLocalRuntimeReady();
    await settle();
    await ctx.render();
    const button = updateButton(ctx);
    expect(button.disabled).toBe(true);
    button.click();
    await settle();
    expect(ctx.dialogs()).toHaveLength(0);
    expect(ctx.callsOf("backup_ollama_model")).toHaveLength(0);
  });

  it("still releases a start that has been silent for too long (the original safety net)", async () => {
    useFakeClock();
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
      start_ollama_runtime: hang,
      verify_ollama_runtime: () => ({ ok: false, endpoint: "x", models_count: 0 }),
      detect_ollama_runtime: () => ({ found: true, is_serving: false, source: "per_user_zip", path: "p", version: "0.40.7" }),
    });
    await ctx.boot();
    void ctx.mod.ensureLocalRuntimeReady();
    await tick(1_000);
    expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(true);
    await tick(60_000);
    expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(false);
  });
});

describe("one operation per model, whatever else is going on", () => {
  it("activating the model of a running update is refused", async () => {
    const ctx = await load(updateHandlers(version), { settingsModel: "other:1b" });
    await ctx.boot();
    await startUpdate(ctx);
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    const before = ctx.calls.length;

    await ctx.mod.activateOllamaModel(OLD);
    const after = ctx.calls.slice(before).map((call) => call.command);
    expect(after).not.toContain("save_settings");
    expect(after).not.toContain("warmup_ollama_model");
    expect(ctx.state.settings!.ai_fallback.model).toBe("other:1b");

    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
  });

  it("an update of a model that is being downloaded right now is refused", async () => {
    const ctx = await load(updateHandlers(version), { settingsModel: OLD });
    await ctx.boot();
    ctx.state.ollamaPullProgress.set(OLD, { model: OLD, status: "pulling manifest" });
    ctx.emit("ollama:pull-progress", { model: OLD, status: "pulling manifest" });
    await settle();

    ctx.cardOf(OLD)!.querySelector<HTMLButtonElement>('button[data-action="update"]')?.click();
    await settle();
    expect(ctx.dialogs()).toHaveLength(0);
    expect(ctx.callsOf("backup_ollama_model")).toHaveLength(0);
    expect(ctx.callsOf("pull_ollama_model")).toHaveLength(0);
  });
});

describe("a catalog model that is being updated", () => {
  it("shows up once (in 'Your Models'), not a second time as a download in 'Available Models'", async () => {
    const ctx = await load(
      updateHandlers(version, {
        fetch_ollama_models_with_size: () => [{ name: CURRENT, size_bytes: 6_583_658_527, digest: OLD_DIGEST }] as InstalledRow[],
        backup_ollama_model: () => `${CURRENT}.bak-1`,
      }),
      { settingsModel: "other:1b" }
    );
    await ctx.boot();
    ctx.cardOf(CURRENT)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await settle();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.state.ollamaPullProgress.set(CURRENT, { model: CURRENT, status: "pulling aa", digest: "sha256:aa", total: 100, completed: 40 });
    ctx.emit("ollama:pull-progress", { model: CURRENT, status: "pulling aa", digest: "sha256:aa", total: 100, completed: 40 });
    await settle();

    const cards = Array.from(ctx.container().querySelectorAll<HTMLElement>(".qwen-model-card")).filter(
      (card) => card.dataset.model === CURRENT || card.querySelector(".qwen-card-tag")?.textContent === CURRENT
    );
    expect(cards).toHaveLength(1);
    expect(ctx.container().querySelector('[data-state="downloading"]')).toBeNull();

    ctx.state.ollamaPullProgress.delete(CURRENT);
    ctx.emit("ollama:pull-complete", { model: CURRENT });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
  });
});

describe("overlapping refreshes of the model list", () => {
  it("an older, slower answer never overwrites a newer one", async () => {
    let call = 0;
    let releaseFirst: () => void = () => {};
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
      fetch_ollama_models_with_size: () => {
        call += 1;
        if (call === 1) {
          return new Promise((resolve) => {
            releaseFirst = () => resolve([{ name: "stale-model:1b", size_bytes: 1_000_000_000 }]);
          });
        }
        return [{ name: "fresh-model:1b", size_bytes: 1_000_000_000 }];
      },
    });
    const slow = ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaInstalledModels();
    releaseFirst();
    await slow;
    await ctx.render();
    const text = ctx.container().textContent ?? "";
    expect(text).toContain("fresh-model:1b");
    expect(text).not.toContain("stale-model:1b");
  });
});

// ---------------------------------------------------------------------------------------------
// 2. Listeners, timers, caches
// ---------------------------------------------------------------------------------------------

describe("pull event listeners", () => {
  async function withFlakyListen(failEvents: (event: string, attempt: number) => boolean) {
    const { listen } = await import("@tauri-apps/api/event");
    const base = vi.mocked(listen).getMockImplementation()!;
    let attempt = 0;
    vi.mocked(listen).mockImplementation((async (event: string, callback: never) => {
      attempt += 1;
      if (failEvents(event, attempt)) throw new Error("event bridge down");
      return (base as (e: string, c: never) => Promise<() => void>)(event, callback);
    }) as never);
    return { listen: vi.mocked(listen), attempts: () => attempt };
  }
  const active = (ctx: Ctx, event: string) => (ctx.listeners.get(event) ?? []).length;

  it("a partly failed registration is rolled back, so a retry does not double every event", async () => {
    const ctx = await load();
    let failed = false;
    await withFlakyListen((event) => {
      if (event === "ollama:pull-error" && !failed) {
        failed = true;
        return true;
      }
      return false;
    });
    expect(await ctx.mod.ensureOllamaPullTracking(true)).toBe(false);
    expect(active(ctx, "ollama:pull-progress")).toBe(0);
    expect(active(ctx, "ollama:pull-complete")).toBe(0);

    expect(await ctx.mod.ensureOllamaPullTracking(true)).toBe(true);
    expect(active(ctx, "ollama:pull-progress")).toBe(1);
    expect(active(ctx, "ollama:pull-complete")).toBe(1);
    expect(active(ctx, "ollama:pull-error")).toBe(1);
  });

  it("is idempotent: rendering again and again registers each event once", async () => {
    const ctx = await load(runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]));
    for (let i = 0; i < 5; i += 1) await ctx.render();
    await Promise.all([ctx.mod.ensureOllamaPullTracking(), ctx.mod.ensureOllamaPullTracking()]);
    expect(active(ctx, "ollama:pull-progress")).toBe(1);
    expect(active(ctx, "ollama:pull-complete")).toBe(1);
    expect(active(ctx, "ollama:pull-error")).toBe(1);
  });

  it("dispose while the registration is still in flight unregisters what arrives afterwards", async () => {
    const ctx = await load();
    const { listen } = await import("@tauri-apps/api/event");
    const base = vi.mocked(listen).getMockImplementation()! as (e: string, c: never) => Promise<() => void>;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.mocked(listen).mockImplementation((async (event: string, callback: never) => {
      await gate;
      return base(event, callback);
    }) as never);

    const pending = ctx.mod.ensureOllamaPullTracking();
    ctx.mod.disposeOllamaPullTracking();
    release();
    await pending;
    await flush();
    expect(active(ctx, "ollama:pull-progress")).toBe(0);
    expect(active(ctx, "ollama:pull-complete")).toBe(0);
    expect(active(ctx, "ollama:pull-error")).toBe(0);
  });

  it("a permanently broken event bridge is not hammered by every render", async () => {
    const ctx = await load(runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]));
    const { listen } = await withFlakyListen(() => true);
    for (let i = 0; i < 6; i += 1) await ctx.render();
    expect(listen.mock.calls.length).toBeLessThanOrEqual(3);
  });
});

describe("caches stay bounded", () => {
  it("forgets the registry status of a model that is gone, so a re-install is checked afresh", async () => {
    const a = { name: "model-a:1b", size_bytes: 1, digest: "a".repeat(64) };
    const b = { name: "model-b:1b", size_bytes: 1, digest: "b".repeat(64) };
    let installed = [a, b];
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]),
      fetch_ollama_models_with_size: () => installed,
      check_ollama_model_updates: (args?: Record<string, unknown>) =>
        ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({ name: model.name, status: "current", remote_digest: "c".repeat(64) })),
    });
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaModelUpdateStatuses();
    expect(ctx.callsOf("check_ollama_model_updates")).toHaveLength(1);
    expect(ctx.mod.getOllamaCacheStats().updates).toBe(2);

    installed = [b];
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaModelUpdateStatuses();
    expect(ctx.mod.getOllamaCacheStats().updates).toBe(1);

    installed = [a, b];
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaModelUpdateStatuses();
    const rechecked = ctx.callsOf("check_ollama_model_updates").slice(1).flatMap((call) => (call.args!.models as Array<{ name: string }>).map((m) => m.name));
    expect(rechecked).toEqual(["model-a:1b"]);
  });

  it("a progress line that arrives after the pull ended does not leave a tracker behind", async () => {
    useFakeClock();
    const ctx = await load(runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]));
    await ctx.mod.ensureOllamaPullTracking(true);
    ctx.emit("ollama:pull-complete", { model: "late:1b" });
    ctx.emit("ollama:pull-progress", { model: "late:1b", status: "pulling aa", digest: "sha256:aa", total: 10, completed: 1 });
    expect(ctx.mod.getOllamaCacheStats().trackers).toBe(1);
    await tick(11 * MINUTE);
    expect(ctx.mod.getOllamaCacheStats().trackers).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("forgets 'runtime too old' refusals once another runtime runs", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load({ ...runtimeHandlers(version, () => catalog), fetch_ollama_models_with_size: () => [] as InstalledRow[] });
    await ctx.boot();
    ctx.emit("ollama:pull-error", { model: "qwen3.5:9b", error: "pull model manifest: 412: requires a newer version of Ollama" });
    ctx.emit("ollama:pull-error", { model: "qwen3.5:4b", error: "pull model manifest: 412: requires a newer version of Ollama" });
    expect(ctx.mod.getOllamaCacheStats().refusals).toBe(2);

    version.current = "0.40.13";
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    ctx.emit("ollama:pull-error", { model: "qwen3.5:2b", error: "pull model manifest: 412: requires a newer version of Ollama" });
    expect(ctx.mod.getOllamaCacheStats().refusals).toBe(1);
  });

  it("drops expired requirement answers instead of keeping them for the life of the app", async () => {
    useFakeClock();
    let installed: InstalledRow[] = [];
    const ctx = await load({
      ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]),
      fetch_ollama_models_with_size: () => installed,
      check_ollama_model_requirements: (args?: Record<string, unknown>) =>
        ((args?.models as string[]) ?? []).map((name) => ({ name, requires: "0.30.0", runtime_version: "0.40.7", compatible: true })),
    });
    await ctx.boot();
    expect(ctx.mod.getOllamaCacheStats().requirements).toBe(3); // the three catalog models

    // Every catalog model gets installed: nothing is a candidate any more, and the old answers expire.
    installed = ctx.mod.OLLAMA_RECOMMENDED_MODELS.map((spec) => ({ name: spec.name, size_bytes: 1, digest: "a".repeat(64) }));
    await ctx.mod.refreshOllamaInstalledModels();
    await tick(7 * 60 * MINUTE);
    await ctx.mod.refreshOllamaModelRequirements();
    expect(ctx.mod.getOllamaCacheStats().requirements).toBe(0);
  });
});

// ---------------------------------------------------------------------------------------------
// 4. Offline and degraded backends, malformed answers
// ---------------------------------------------------------------------------------------------

describe("malformed runtime catalogs", () => {
  const bad: Array<[string, unknown]> = [
    ["null", null],
    ["a string", "oops"],
    ["an object", { versions: [] }],
    ["entries that are null or lack a version", [null, 5, { version: 5 }, { source: "online" }, runtime("0.40.9")]],
  ];

  it.each(bad)("list_ollama_runtime_versions answering %s leaves a usable catalog", async (_label, answer) => {
    const ctx = await load({ ...runtimeHandlers(version, () => []), list_ollama_runtime_versions: () => answer });
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    const catalog = ctx.mod.getOllamaRuntimeVersionCatalog();
    expect(Array.isArray(catalog)).toBe(true);
    expect(catalog.length).toBeGreaterThan(0);
    expect(catalog.every((entry) => entry && typeof entry.version === "string")).toBe(true);
  });

  it.each(bad)("fetch_ollama_online_versions answering %s leaves a usable catalog and no crash", async (_label, answer) => {
    const ctx = await load({ ...runtimeHandlers(version, () => []), fetch_ollama_online_versions: () => answer });
    const status = vi.fn();
    await ctx.mod.fetchOnlineVersionCatalog(status);
    const catalog = ctx.mod.getOllamaRuntimeVersionCatalog();
    expect(catalog.every((entry) => entry && typeof entry.version === "string")).toBe(true);
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(false);
  });

  it("an empty answer does not wipe the verified catalog that was already known", async () => {
    let answer: unknown[] = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { latest: true })];
    const ctx = await load({ ...runtimeHandlers(version, () => []), fetch_ollama_online_versions: () => answer, list_ollama_runtime_versions: () => answer });
    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    expect(ctx.mod.getOllamaRuntimeVersionCatalog().map((entry) => entry.version)).toContain("0.40.13");
    answer = [];
    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    expect(ctx.mod.getOllamaRuntimeVersionCatalog().map((entry) => entry.version)).toContain("0.40.13");
  });
});

describe("malformed model data never breaks the pure helpers", () => {
  it("builds an inventory from rows with null, wrong-typed and missing fields", async () => {
    const ctx = await load();
    const rows = [
      null,
      "text",
      { name: null },
      { name: 5 },
      { name: "odd:1b", size_bytes: null, digest: 12345, parameter_size: 8, quantization_level: {}, modified_at: {} },
      { name: "fine:1b", size_bytes: 1_000_000_000, digest: "f".repeat(64) },
    ] as never;
    const inventory = ctx.mod.buildInstalledOllamaInventory(
      ctx.mod.sanitizeInstalledModels(rows),
      "fine:1b",
      new Map([["odd:1b", { name: "odd:1b", status: "outdated", remote_digest: 777 as never, remote_size_bytes: "huge" as never }]])
    );
    expect(inventory.map((item) => item.name)).toEqual(["fine:1b", "odd:1b"]);
    expect(inventory[1].size_label).toBe("Unknown size");
  });

  it("formats update hints and digests of any type", async () => {
    const ctx = await load();
    expect(() => ctx.mod.modelUpdateHint({ name: "x", status: "outdated", remote_digest: 42 as never })).not.toThrow();
    expect(() => ctx.mod.shortDigest({} as never)).not.toThrow();
    expect(ctx.mod.shortDigest(42 as never)).toBe("");
  });

  it("ignores requirement rows it cannot interpret instead of inventing a 'Needs' chip", async () => {
    const ctx = await load();
    const catalog = [runtime("0.40.13", { installed: true })];
    for (const requires of [5, "latest", "nightly", "", "  ", "<img src=x onerror=alert(1)>", {}]) {
      const gate = ctx.mod.buildModelRuntimeGate(
        { name: "x", requires: requires as never, runtime_version: "0.40.7", compatible: null },
        "0.40.7",
        catalog
      );
      expect(gate).toBeNull();
    }
    // A partial version still reads as one: informational chip, nothing is blocked.
    const partial = ctx.mod.buildModelRuntimeGate({ name: "x", requires: "0.5", runtime_version: "0.40.7", compatible: null }, "0.40.7", catalog);
    expect(partial?.chipLabel).toBe("Needs Ollama ≥ 0.5");
    expect(partial?.blocked).toBe(false);
    // Wrong-typed `requires` with a verdict is still nothing to show: there is no version to name.
    expect(ctx.mod.buildModelRuntimeGate({ name: "x", requires: 5 as never, runtime_version: "0.40.7", compatible: false }, "0.40.7", catalog)).toBeNull();
    const strange = ctx.mod.buildModelRuntimeGate(
      { name: "x", requires: "0.40.13", runtime_version: 7 as never, compatible: "false" as never },
      "0.40.7",
      catalog
    );
    expect(strange?.blocked).toBe(true);
    expect(strange?.chipLabel).toBe("Needs Ollama ≥ 0.40.13");
  });
});

describe("degraded backends through the real renderer", () => {
  it("renders every card from rows with null and wrong-typed fields", async () => {
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
        fetch_ollama_models_with_size: () => [
          null,
          { name: null },
          { name: "odd:1b", size_bytes: null, digest: 12345, parameter_size: 8, quantization_level: {} },
          { name: "fine:1b", size_bytes: 1_000_000_000, digest: "f".repeat(64) },
        ],
        check_ollama_model_updates: () => [
          null,
          { name: "fine:1b", status: "outdated", remote_digest: 999, remote_size_bytes: "x" },
          { name: "odd:1b", status: "banana" },
        ],
        check_ollama_model_requirements: () => [null, { name: NEEDY, requires: 12, compatible: "no" }, { name: "qwen3.5:4b" }],
      },
      { settingsModel: "fine:1b" }
    );
    await ctx.boot();
    expect(ctx.cardOf("fine:1b")).toBeDefined();
    expect(ctx.cardOf("odd:1b")).toBeDefined();
    expect(modelBox(ctx.cardOf(NEEDY))!.disabled).toBe(false);
    expect(ctx.cardOf(NEEDY)!.querySelector(".qwen-card-badge--needs-runtime, .qwen-card-badge--runtime-ok")).toBeNull();
    expect(ctx.showToast).not.toHaveBeenCalled();
  });

  it("every backend down: no toast, no crash, the catalog still renders", async () => {
    const down = () => {
      throw new Error("Ollama is not reachable");
    };
    const ctx = await load(
      {
        list_ollama_runtime_versions: down,
        fetch_ollama_online_versions: down,
        detect_ollama_runtime: down,
        verify_ollama_runtime: down,
        fetch_ollama_models_with_size: down,
        check_ollama_model_updates: down,
        check_ollama_model_requirements: down,
        get_ollama_model_info: down,
      },
      { settingsModel: CURRENT }
    );
    await ctx.boot();
    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    await ctx.render();
    expect(ctx.cardOf("qwen3.5:4b")).toBeDefined();
    expect(ctx.showToast).not.toHaveBeenCalled();
    expect(ctx.mod.getOllamaRuntimeCardState().detected).toBe(false);
  });

  it("a failed refresh keeps the last known models and says the list may be stale, instead of 'nothing installed'", async () => {
    let down = false;
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
        fetch_ollama_models_with_size: () => {
          if (down) throw new Error("connection refused");
          return [{ name: CURRENT, size_bytes: 6_583_658_527, digest: NEW_DIGEST }];
        },
      },
      { settingsModel: CURRENT }
    );
    await ctx.boot();
    expect(ctx.cardOf(CURRENT)).toBeDefined();

    down = true;
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    expect(ctx.cardOf(CURRENT)).toBeDefined();
    expect(ctx.container().textContent).toContain("not reachable");
    expect(ctx.container().textContent).not.toContain("No local refinement models installed yet");
    expect(ctx.showToast).not.toHaveBeenCalled();

    down = false;
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    expect(ctx.container().textContent).not.toContain("not reachable");
  });

  it("with nothing known and Ollama unreachable the empty state names the cause", async () => {
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]),
        fetch_ollama_models_with_size: () => {
          throw new Error("connection refused");
        },
      },
      { settingsModel: CURRENT }
    );
    await ctx.boot();
    expect(ctx.container().textContent).toContain("not reachable");
    expect(ctx.container().textContent).not.toContain("No local refinement models installed yet");
  });

  describe("the onboarding flag (local_ai_wizard_completed) is not flipped by what could not be read", () => {
    const completedSetup = { local_ai_wizard_completed: true, local_ai_wizard_pending: false, ollama_remote_expert_opt_in: false };
    const savedWizardStates = (ctx: Ctx) =>
      ctx.callsOf("save_settings").map((call) => (call.args as { settings: { setup?: { local_ai_wizard_completed: boolean } } }).settings.setup?.local_ai_wizard_completed);

    it("a runtime refresh before the model list was read (start-up order) saves nothing", async () => {
      const ctx = await load(
        {
          ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
          fetch_ollama_models_with_size: () => [{ name: CURRENT, size_bytes: 1, digest: NEW_DIGEST }] as InstalledRow[],
        },
        { settingsModel: CURRENT }
      );
      ctx.state.settings!.setup = { ...completedSetup };
      await ctx.mod.refreshOllamaRuntimeState({ force: true }); // bootstrap does this first
      expect(ctx.state.settings!.setup!.local_ai_wizard_completed).toBe(true);
      expect(savedWizardStates(ctx)).not.toContain(false);

      await ctx.mod.refreshOllamaInstalledModels();
      await ctx.mod.refreshOllamaRuntimeState({ force: true });
      expect(ctx.state.settings!.setup!.local_ai_wizard_completed).toBe(true);
      expect(savedWizardStates(ctx)).not.toContain(false);
    });

    it("Ollama not reachable at start leaves a completed onboarding completed", async () => {
      const down = () => {
        throw new Error("Ollama is not reachable");
      };
      const ctx = await load(
        { detect_ollama_runtime: down, verify_ollama_runtime: down, fetch_ollama_models_with_size: down, list_ollama_runtime_versions: down },
        { settingsModel: CURRENT }
      );
      ctx.state.settings!.setup = { ...completedSetup };
      await ctx.mod.refreshOllamaRuntimeState({ force: true });
      await ctx.mod.refreshOllamaInstalledModels();
      await ctx.mod.refreshOllamaRuntimeState({ force: true });
      expect(ctx.state.settings!.setup!.local_ai_wizard_completed).toBe(true);
      expect(savedWizardStates(ctx)).not.toContain(false);
    });

    it("a model list that was read and is really empty still reopens the wizard (unchanged)", async () => {
      const ctx = await load(
        { ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]), fetch_ollama_models_with_size: () => [] as InstalledRow[] },
        { settingsModel: CURRENT }
      );
      ctx.state.settings!.setup = { ...completedSetup };
      await ctx.mod.refreshOllamaInstalledModels();
      await ctx.mod.refreshOllamaRuntimeState({ force: true });
      expect(ctx.state.settings!.setup!.local_ai_wizard_completed).toBe(false);
    });
  });

  it("catalog entries with missing fields never become empty cards", async () => {
    const good = {
      name: "good:1b", label: "Good", source: "ollama", family: "g", status: "supported", profile: "p", size_gb: 1, description: "Fine.",
    };
    const ctx = await load(
      { ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]), fetch_ollama_models_with_size: () => [] as InstalledRow[] },
      { catalogJson: [good, { name: "no-label:1b", source: "ollama" }, null, { ...good, name: "no-size:1b", size_gb: null }, { ...good, name: "no-desc:1b", description: "" }] }
    );
    await ctx.boot();
    const cards = Array.from(ctx.container().querySelectorAll<HTMLElement>(".qwen-model-card"));
    expect(cards.map((card) => card.dataset.model)).toEqual(["good:1b"]);
    expect(cards.every((card) => (card.textContent ?? "").trim().length > 10)).toBe(true);
  });
});

// ---------------------------------------------------------------------------------------------
// 5. State after a runtime change
// ---------------------------------------------------------------------------------------------

describe("runtime state after a change", () => {
  it("does not offer 'Switch to X' for a version just picked in the dropdown that is not on disk", async () => {
    // detect_ollama_runtime reports `target_installed` for the target that was selected when it ran.
    let targetOnDisk = true; // 0.40.7, the target at the time of the detect
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { latest: true })];
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => catalog),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        detect_ollama_runtime: () => ({
          found: true,
          is_serving: true,
          source: "per_user_zip",
          path: "p",
          version: "0.40.7",
          target_installed: targetOnDisk,
        }),
      },
      { target: "0.40.7" }
    );
    await ctx.boot();
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Runtime ready");

    // The user picks 0.40.13 (not installed). No detect has run since: its answer was about 0.40.7.
    ctx.state.settings!.providers.ollama.runtime_target_version = "0.40.13";
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Install 0.40.13");

    targetOnDisk = false; // and a fresh detect agrees
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Install 0.40.13");
  });

  it("a forced refresh that arrives while an older one is running still reads the runtime afresh", async () => {
    let releaseFirst: () => void = () => {};
    let call = 0;
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load({
      ...runtimeHandlers(version, () => catalog),
      fetch_ollama_models_with_size: () => [] as InstalledRow[],
      detect_ollama_runtime: () => {
        call += 1;
        const seen = version.current; // what this detect saw when it started
        const answer = { found: true, is_serving: true, source: "per_user_zip", path: "p", version: seen };
        if (call !== 1) return answer;
        return new Promise((resolve) => {
          releaseFirst = () => resolve(answer);
        });
      },
    });
    const older = ctx.mod.refreshOllamaRuntimeState({ force: true });
    await settle();
    version.current = "0.40.13"; // the runtime was switched while that detect was running
    const newer = ctx.mod.refreshOllamaRuntimeState({ force: true });
    releaseFirst();
    await Promise.all([older, newer]);
    expect(ctx.mod.getOllamaRuntimeCardState().version).toBe("0.40.13");
  });

  it("an old backend without `target_installed` falls back to the catalog", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load({ ...runtimeHandlers(version, () => catalog), fetch_ollama_models_with_size: () => [] as InstalledRow[] }, { target: "0.40.13" });
    await ctx.boot();
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Switch to 0.40.13");
  });

  it("no detect result at all (timeout, error): the catalog decides and nothing throws", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      {
        ...runtimeHandlers(version, () => catalog),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        detect_ollama_runtime: () => {
          throw new Error("timed out");
        },
      },
      { target: "0.40.13" }
    );
    await ctx.boot();
    const card = ctx.mod.getOllamaRuntimeCardState();
    expect(card.detected).toBe(false);
    expect(card.primaryLabel).toBe("Install local runtime"); // nothing detected: install path decides on disk truth itself
  });

  it("re-evaluates the requirement chip against the new runtime after a switch", async () => {
    const cat = () => [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    const ctx = await load(
      {
        ...runtimeHandlers(version, cat),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        check_ollama_model_requirements: (args?: Record<string, unknown>) =>
          ((args?.models as string[]) ?? []).filter((n) => n === NEEDY).map((name) => ({ name, requires: "0.40.13", runtime_version: version.current, compatible: version.current === "0.40.13" })),
        start_ollama_runtime: () => {
          version.current = "0.40.13";
          return { pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false };
        },
      },
      { target: "0.40.13" }
    );
    version.current = "0.40.7";
    await ctx.boot();
    expect(modelBox(ctx.cardOf(NEEDY))!.disabled).toBe(true);

    await ctx.mod.ensureLocalRuntimeReady();
    await ctx.render();
    expect(modelBox(ctx.cardOf(NEEDY))!.disabled).toBe(false);
    version.current = "0.40.7";
  });

  it("does not resurrect a stale 'too old' verdict when the runtime is unknown right after a switch", async () => {
    const cat = () => [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { installed: true })];
    let detectFails = false;
    const ctx = await load(
      {
        ...runtimeHandlers(version, cat),
        fetch_ollama_models_with_size: () => [] as InstalledRow[],
        detect_ollama_runtime: () => {
          if (detectFails) throw new Error("timed out");
          return { found: true, is_serving: true, source: "per_user_zip", path: "p", version: version.current, target_installed: true };
        },
        check_ollama_model_requirements: (args?: Record<string, unknown>) =>
          ((args?.models as string[]) ?? []).filter((n) => n === NEEDY).map((name) => ({ name, requires: "0.40.13", runtime_version: "0.40.7", compatible: false })),
        start_ollama_runtime: () => {
          version.current = "0.40.13";
          detectFails = true; // the restarted server answers too late for the detect timeout
          return { pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false };
        },
      },
      { target: "0.40.13" }
    );
    version.current = "0.40.7";
    await ctx.boot();
    expect(modelBox(ctx.cardOf(NEEDY))!.disabled).toBe(true);

    await ctx.mod.ensureLocalRuntimeReady();
    await ctx.render();
    expect(ctx.mod.getOllamaRuntimeCardState().detected).toBe(false);
    expect(modelBox(ctx.cardOf(NEEDY))!.disabled).toBe(false);
    version.current = "0.40.7";
  });
});

describe("the app has not loaded its settings yet (settings === null)", () => {
  it("nothing throws and nothing is changed", async () => {
    const ctx = await load(
      { ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true })]), fetch_ollama_models_with_size: () => [] as InstalledRow[] },
      { settings: null }
    );
    expect(() => ctx.mod.getOllamaRuntimeCardState()).not.toThrow();
    expect(() => ctx.mod.getOllamaRuntimeVersionCatalog()).not.toThrow();
    await ctx.mod.refreshOllamaRuntimeState({ force: true });
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaModelUpdateStatuses();
    await ctx.mod.refreshOllamaModelRequirements();
    await ctx.render();
    await ctx.mod.ensureLocalRuntimeReady();
    await ctx.mod.updateOllamaRuntimeTo("0.40.13");
    await ctx.mod.activateOllamaModel("x:1b");
    await ctx.mod.autoStartLocalRuntimeIfNeeded("bootstrap");
    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    expect(ctx.callsOf("save_settings")).toHaveLength(0);
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(0);
    expect(toastNamed(ctx.showToast, "Settings unavailable").message).toContain("not loaded");
  });
});
