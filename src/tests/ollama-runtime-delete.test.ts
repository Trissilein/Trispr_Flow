/**
 * Row buttons of the runtime version list in the runtime flow: Delete (confirmation, backend answer,
 * locks) and Install / Switch to (a pick in the list is an explicit target).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  load,
  runtime,
  runtimeHandlers,
  settle,
  toastNamed,
  toastTitles,
  hang,
  type Ctx,
  type InstalledRow,
} from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

const version = { current: "0.40.13" };

const catalog = () => [
  runtime("0.40.13", { source: "pinned", recommended: true, latest: true, installed: true, size_bytes: 1_600_000_000 }),
  runtime("0.30.4", { installed: true, installable: false, size_bytes: 1_300_000_000 }),
  runtime("0.35.1"),
];

function handlers(extra: Record<string, (args?: Record<string, unknown>) => unknown> = {}) {
  return {
    ...runtimeHandlers(version, catalog),
    fetch_ollama_models_with_size: () => [] as InstalledRow[],
    delete_ollama_runtime_version: () => ({ deleted: true, freed_bytes: 3_400_000_000, detail: null }),
    ...extra,
  };
}

beforeEach(() => {
  version.current = "0.40.13";
});

async function open(ctx: Ctx, target: string, action: "install" | "switch" | "delete") {
  const run = ctx.mod.runRuntimeVersionAction(target, action);
  await settle();
  return run;
}

describe("delete a runtime version", () => {
  it("asks first (version, size, 'install it again later'), then deletes, reports the freed size and refreshes the list", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    const listCallsBefore = ctx.callsOf("list_ollama_runtime_versions").length;
    const updated = vi.fn();
    window.addEventListener(ctx.mod.RUNTIME_CATALOG_UPDATED_EVENT, updated);

    const run = open(ctx, "0.30.4", "delete");
    await settle();
    expect(ctx.callsOf("delete_ollama_runtime_version")).toHaveLength(0);
    expect(ctx.dialogs()).toHaveLength(1);
    const text = await ctx.confirmDialog("confirm");
    await run;
    await settle();

    expect(text).toContain("Delete runtime 0.30.4?");
    expect(text).toContain("1.3 GB");
    expect(text).toContain("install it again later");
    expect(ctx.callsOf("delete_ollama_runtime_version").map((call) => call.args)).toEqual([{ version: "0.30.4" }]);
    expect(ctx.callsOf("list_ollama_runtime_versions").length).toBeGreaterThan(listCallsBefore);
    expect(updated).toHaveBeenCalled();
    const toast = toastNamed(ctx.showToast, "Runtime 0.30.4 deleted");
    expect(toast.type).toBe("success");
    expect(toast.message).toBe("Freed 3.4 GB.");
    window.removeEventListener(ctx.mod.RUNTIME_CATALOG_UPDATED_EVENT, updated);
  });

  it("the dialog defaults to Cancel (data goes away)", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    const run = open(ctx, "0.30.4", "delete");
    await settle();
    expect(document.activeElement).toBe(
      document.querySelector('.ollama-confirm-dialog button[data-dialog-action="cancel"]'),
    );
    await ctx.confirmDialog("cancel");
    await run;
  });

  it("cancel deletes nothing and says nothing", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    const run = open(ctx, "0.30.4", "delete");
    await settle();
    await ctx.confirmDialog("cancel");
    await run;
    expect(ctx.callsOf("delete_ollama_runtime_version")).toHaveLength(0);
    expect(toastTitles(ctx.showToast).filter((title) => /deleted|kept/.test(title))).toEqual([]);
  });

  it("shows the reason when the backend refuses and keeps the runtime", async () => {
    const ctx = await load(
      handlers({
        delete_ollama_runtime_version: () => ({
          deleted: false,
          freed_bytes: 0,
          detail: "Runtime 0.30.4 is the selected target. Select another version first, then delete it.",
        }),
      }),
      { target: "0.40.13" },
    );
    await ctx.boot();
    const run = open(ctx, "0.30.4", "delete");
    await settle();
    await ctx.confirmDialog("confirm");
    await run;
    await settle();

    const toast = toastNamed(ctx.showToast, "Runtime 0.30.4 kept");
    expect(toast.type).toBe("warning");
    expect(toast.message).toContain("Select another version first");
    expect(toastTitles(ctx.showToast)).not.toContain("Runtime 0.30.4 deleted");
  });

  it("a partly locked folder is reported as a warning, not as clean success", async () => {
    const ctx = await load(
      handlers({
        delete_ollama_runtime_version: () => ({
          deleted: true,
          freed_bytes: 100,
          detail: "Runtime 0.30.4 was removed, but some files are still locked (access denied). They are cleaned up later.",
        }),
      }),
      { target: "0.40.13" },
    );
    await ctx.boot();
    const run = open(ctx, "0.30.4", "delete");
    await settle();
    await ctx.confirmDialog("confirm");
    await run;
    await settle();
    const toast = toastNamed(ctx.showToast, "Runtime 0.30.4 deleted");
    expect(toast.type).toBe("warning");
    expect(toast.message).toContain("still locked");
  });

  it("a failing backend call gives an error toast and unlocks the runtime controls", async () => {
    const ctx = await load(
      handlers({
        delete_ollama_runtime_version: () => {
          throw new Error("boom");
        },
      }),
      { target: "0.40.13" },
    );
    await ctx.boot();
    const run = open(ctx, "0.30.4", "delete");
    await settle();
    await ctx.confirmDialog("confirm");
    await run;
    await settle();

    expect(toastTitles(ctx.showToast).some((title) => /failed/i.test(title))).toBe(true);
    expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(false);
  });

  it("locks the runtime controls while it runs and ignores a second click", async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const ctx = await load(
      handlers({
        delete_ollama_runtime_version: async () => {
          await gate;
          return { deleted: true, freed_bytes: 1, detail: null };
        },
      }),
      { target: "0.40.13" },
    );
    await ctx.boot();

    const first = open(ctx, "0.30.4", "delete");
    await settle();
    await ctx.confirmDialog("confirm");
    await settle();
    expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(true);
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Deleting runtime...");

    // a second delete while one runs: no dialog, no second backend call
    await ctx.mod.runRuntimeVersionAction("0.35.1", "delete");
    expect(ctx.dialogs()).toHaveLength(0);

    release();
    await first;
    await settle();
    expect(ctx.callsOf("delete_ollama_runtime_version")).toHaveLength(1);
    expect(ctx.mod.getOllamaRuntimeCardState().busy).toBe(false);
  });

  it("two quick clicks open one confirmation", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    const first = ctx.mod.runRuntimeVersionAction("0.30.4", "delete");
    const second = ctx.mod.runRuntimeVersionAction("0.30.4", "delete");
    await settle();
    expect(ctx.dialogs()).toHaveLength(1);
    await ctx.confirmDialog("cancel");
    await Promise.all([first, second]);
  });

  it("nothing is deleted while another runtime operation is busy", async () => {
    const ctx = await load(
      handlers({ start_ollama_runtime: () => hang() }),
      { target: "0.40.13" },
    );
    await ctx.boot();
    // an Install/Switch that never finishes starting keeps the runtime busy
    void ctx.mod.updateOllamaRuntimeTo("0.30.4", { explicit: true });
    await settle();
    await ctx.mod.runRuntimeVersionAction("0.35.1", "delete");
    expect(ctx.dialogs()).toHaveLength(0);
    expect(ctx.callsOf("delete_ollama_runtime_version")).toHaveLength(0);
  });

  it("a blank version does nothing", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    await ctx.mod.runRuntimeVersionAction("  ", "delete");
    expect(ctx.dialogs()).toHaveLength(0);
    expect(ctx.callsOf("delete_ollama_runtime_version")).toHaveLength(0);
  });
});

describe("Install and Switch to from the list", () => {
  const savedExplicit = (ctx: Ctx) =>
    ctx.callsOf("save_settings").map(
      (call) => ((call.args?.settings as { providers: { ollama: { runtime_target_explicit?: boolean; runtime_target_version: string } } }).providers.ollama),
    );

  it("a pick in the list is saved as an explicit target, then the normal runtime flow runs", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();

    await ctx.mod.runRuntimeVersionAction("0.30.4", "switch");
    await settle();

    const ollama = ctx.state.settings!.providers.ollama;
    expect(ollama.runtime_target_version).toBe("0.30.4");
    expect(ollama.runtime_target_explicit).toBe(true);
    expect(savedExplicit(ctx)[savedExplicit(ctx).length - 1]).toMatchObject({ runtime_target_version: "0.30.4", runtime_target_explicit: true });
    expect(ctx.callsOf("download_ollama_runtime")).toHaveLength(0); // on disk: no download
    expect(ctx.callsOf("start_ollama_runtime")).toHaveLength(1);
  });

  it("an install pick of a version that is not on disk downloads it and is explicit as well", async () => {
    const ctx = await load(
      handlers({
        download_ollama_runtime: () => ({ sha256_ok: true, archive_path: "a.zip", version: "0.35.1" }),
        install_ollama_runtime: () => ({ runtime_path: "x", version: "0.35.1" }),
      }),
      { target: "0.40.13" },
    );
    await ctx.boot();

    await ctx.mod.runRuntimeVersionAction("0.35.1", "install");
    await settle();

    expect(ctx.state.settings!.providers.ollama.runtime_target_explicit).toBe(true);
    expect(ctx.callsOf("download_ollama_runtime")[0].args).toEqual({ version: "0.35.1" });
  });

  it("the 'update the runtime for this model' path stays an ordinary (healable) target", async () => {
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    ctx.state.settings!.providers.ollama.runtime_target_explicit = true; // left over from an earlier pick

    await ctx.mod.updateOllamaRuntimeTo("0.40.13");
    await settle();

    expect(ctx.state.settings!.providers.ollama.runtime_target_explicit).toBe(false);
  });

  it("an older explicit pick is never lifted back by the refresh", async () => {
    version.current = "0.40.13";
    const ctx = await load(handlers(), { target: "0.40.13" });
    await ctx.boot();
    await ctx.mod.runRuntimeVersionAction("0.30.4", "switch");
    await settle();

    await ctx.mod.refreshOllamaRuntimeState({ force: true });

    const ollama = ctx.state.settings!.providers.ollama;
    expect(ollama.runtime_target_version).toBe("0.30.4");
    expect(ollama.runtime_target_explicit).toBe(true);
  });

  it("an implicit older target is lifted by the refresh and no downgrade is offered", async () => {
    const ctx = await load(handlers(), { target: "0.30.4" });
    await ctx.boot();
    const ollama = ctx.state.settings!.providers.ollama;
    expect(ollama.runtime_target_version).toBe("0.40.13");
    expect(ctx.mod.getOllamaRuntimeCardState().primaryLabel).toBe("Runtime ready");
  });
});
