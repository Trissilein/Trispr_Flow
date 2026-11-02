/**
 * "Download selected" as the app wires it: tick boxes on the not-installed catalog rows, the bar
 * under the list, and the pulls of the ticked models one after the other. `pull_ollama_model`
 * only STARTS a pull (the real command returns at once), so "one after the other" has to wait for
 * the end event of each pull; a failed model must not stop the rest.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  bulkButton,
  downloadSelected,
  load,
  modelBox,
  runtime,
  runtimeHandlers,
  settle,
  toastNamed,
  toastTitles,
  type Ctx,
  type InstalledRow,
} from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));
vi.mock("../toast", () => ({ showToast: vi.fn() }));

const version = { current: "0.40.7" };
const E4B = "gemma4:e4b";
const E2B = "gemma4:e2b";
const QWEN = "qwen3.5:4b";

afterEach(() => {
  document.body.innerHTML = "";
});

function handlers(extra: Record<string, (args?: Record<string, unknown>) => unknown> = {}) {
  return {
    ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true })]),
    fetch_ollama_models_with_size: () => [] as InstalledRow[],
    pull_ollama_model: () => undefined, // returns as soon as the pull is spawned, like the real command
    ...extra,
  };
}

const pulled = (ctx: Ctx) => ctx.callsOf("pull_ollama_model").map((call) => (call.args as { model: string }).model);
const bar = (ctx: Ctx) => bulkButton(ctx.container())!;
const summary = (ctx: Ctx) => ctx.container().querySelector(".ollama-model-bulk-summary")!.textContent;
const chipsOf = (ctx: Ctx, name: string) =>
  Array.from(ctx.cardOf(name)!.querySelectorAll(".ollama-model-chips .qwen-card-badge")).map((chip) => chip.textContent);

/** Nothing installed: gemma4:e4b is the selected model, so it is listed as missing, on top. */
async function boot(extra: Record<string, (args?: Record<string, unknown>) => unknown> = {}) {
  const ctx = await load(handlers(extra));
  await ctx.boot();
  return ctx;
}

describe("catalog rows", () => {
  it("have a tick box instead of a Download button, and one status chip", async () => {
    const ctx = await boot();
    for (const name of [E4B, E2B, QWEN]) {
      expect(modelBox(ctx.cardOf(name))).not.toBeNull();
      expect(ctx.cardOf(name)!.querySelector('button[data-action="download"]')).toBeNull();
    }
    // Missing is the one status chip of the selected model; the others carry no runtime label.
    expect(chipsOf(ctx, E4B)).toEqual(["Missing", "Recommended"]);
    expect(chipsOf(ctx, E2B)).toEqual([]);
    expect(chipsOf(ctx, QWEN)).toEqual([]);
  });

  it("show the runtime chip and a disabled box only for a real conflict", async () => {
    const ctx = await boot({
      check_ollama_model_requirements: (args) =>
        ((args?.models as string[]) ?? []).map((name) => ({
          name,
          requires: name === E2B ? "0.99.0" : "0.17.0",
          runtime_version: "0.40.7",
          compatible: name !== E2B,
        })),
    });
    expect(chipsOf(ctx, E2B)).toEqual(["Needs Ollama ≥ 0.99.0"]);
    expect(modelBox(ctx.cardOf(E2B))!.disabled).toBe(true);
    expect(chipsOf(ctx, QWEN)).toEqual([]);
    expect(modelBox(ctx.cardOf(QWEN))!.disabled).toBe(false);
  });

  it("show the bar with the summed catalog size of what is ticked", async () => {
    const ctx = await boot();
    expect(summary(ctx)).toBe("0 selected · 0.0 GB");
    expect(bar(ctx).disabled).toBe(true);
    modelBox(ctx.cardOf(E4B))!.click();
    modelBox(ctx.cardOf(QWEN))!.click();
    expect(summary(ctx)).toBe("2 selected · 9.9 GB"); // 6.58 + 3.32
    expect(bar(ctx).disabled).toBe(false);
  });

  it("keep the ticks through a full re-render of the manager", async () => {
    const ctx = await boot();
    modelBox(ctx.cardOf(QWEN))!.click();
    await ctx.render();
    await ctx.render();
    expect(modelBox(ctx.cardOf(QWEN))!.checked).toBe(true);
    expect(summary(ctx)).toBe("1 selected · 3.3 GB");
  });

  it("lose the tick of a model that got installed, also when it is deleted again", async () => {
    let installed: InstalledRow[] = [];
    const ctx = await boot({ fetch_ollama_models_with_size: () => installed });
    modelBox(ctx.cardOf(QWEN))!.click();
    modelBox(ctx.cardOf(E2B))!.click();
    expect(summary(ctx)).toBe("2 selected · 7.9 GB");

    installed = [{ name: QWEN, size_bytes: 3_320_000_000, digest: "a".repeat(64) }];
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    expect(modelBox(ctx.cardOf(QWEN))).toBeNull(); // an installed row has no box
    expect(summary(ctx)).toBe("1 selected · 4.6 GB");

    installed = [];
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    expect(modelBox(ctx.cardOf(QWEN))!.checked).toBe(false);
    expect(modelBox(ctx.cardOf(E2B))!.checked).toBe(true);
  });

  it("do not open or close when the box is clicked", async () => {
    const ctx = await boot();
    modelBox(ctx.cardOf(QWEN))!.click();
    expect(ctx.cardOf(QWEN)!.dataset.expanded).toBe("false");
    expect(ctx.callsOf("get_ollama_model_details")).toHaveLength(0);
  });
});

describe("Download selected", () => {
  it("pulls the ticked models one after the other, in list order, each after the end of the one before", async () => {
    const ctx = await boot();
    downloadSelected(ctx.cardOf(QWEN), ctx.cardOf(E4B), ctx.cardOf(E2B));
    await settle();
    // List order: the missing selected model first, then alphabetical by title. Only the first runs.
    expect(pulled(ctx)).toEqual([E4B]);
    expect(ctx.cardOf(E4B)!.dataset.state).toBe("downloading");
    expect(chipsOf(ctx, E2B)).toEqual(["Queued"]);
    expect(modelBox(ctx.cardOf(E2B))).toBeNull(); // queued rows cannot be ticked again
    expect(chipsOf(ctx, QWEN)).toEqual(["Queued"]);

    // Progress of the first one does not start the next.
    ctx.emit("ollama:pull-progress", { model: E4B, status: "pulling aa", digest: "sha256:aa", total: 10, completed: 5 });
    await settle();
    expect(pulled(ctx)).toEqual([E4B]);

    ctx.emit("ollama:pull-complete", { model: E4B });
    await settle();
    expect(pulled(ctx)).toEqual([E4B, E2B]);
    expect(chipsOf(ctx, QWEN)).toEqual(["Queued"]);

    ctx.emit("ollama:pull-complete", { model: E2B });
    await settle();
    expect(pulled(ctx)).toEqual([E4B, E2B, QWEN]);

    ctx.emit("ollama:pull-complete", { model: QWEN });
    await settle();
    expect(pulled(ctx)).toEqual([E4B, E2B, QWEN]); // nothing twice
    expect(chipsOf(ctx, QWEN)).not.toContain("Queued");
  });

  it("spends the ticks and disables the button once the downloads are under way", async () => {
    const ctx = await boot();
    downloadSelected(ctx.cardOf(E2B), ctx.cardOf(QWEN));
    await settle();
    expect(summary(ctx)).toBe("0 selected · 0.0 GB");
    expect(bar(ctx).disabled).toBe(true);
  });

  it("an error event of one model does not stop the others", async () => {
    const ctx = await boot();
    downloadSelected(ctx.cardOf(E2B), ctx.cardOf(QWEN));
    await settle();
    expect(pulled(ctx)).toEqual([E2B]);

    ctx.emit("ollama:pull-error", { model: E2B, error: "disk full" });
    await settle();
    expect(pulled(ctx)).toEqual([E2B, QWEN]);
    ctx.emit("ollama:pull-complete", { model: QWEN });
    await settle();
    expect(pulled(ctx)).toEqual([E2B, QWEN]);
  });

  it("a pull command that fails right away (toast) does not stop the others", async () => {
    let call = 0;
    const ctx = await boot({
      pull_ollama_model: () => {
        call += 1;
        if (call === 1) throw "Connection failed: refused";
        return undefined;
      },
    });
    downloadSelected(ctx.cardOf(E2B), ctx.cardOf(QWEN));
    await vi.waitFor(() => expect(pulled(ctx)).toEqual([E2B, QWEN]));
    expect(toastNamed(ctx.showToast, "Download failed").message).toBe("Connection failed: refused");
    // The failed model is free again: it can be ticked once more.
    await settle();
    expect(modelBox(ctx.cardOf(E2B))).not.toBeNull();
    expect(ctx.cardOf(QWEN)!.dataset.state).toBe("downloading");
  });

  it("a second 'Download selected' while the first runs queues behind it instead of pulling in parallel", async () => {
    const ctx = await boot();
    downloadSelected(ctx.cardOf(E2B));
    await settle();
    expect(pulled(ctx)).toEqual([E2B]);

    downloadSelected(ctx.cardOf(QWEN));
    await settle();
    expect(pulled(ctx)).toEqual([E2B]); // still one at a time
    expect(chipsOf(ctx, QWEN)).toEqual(["Queued"]);

    ctx.emit("ollama:pull-complete", { model: E2B });
    await settle();
    expect(pulled(ctx)).toEqual([E2B, QWEN]);
  });

  it("starts nothing while the runtime is being changed (button disabled, ticks kept)", async () => {
    const catalog = [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13")];
    const ctx = await load(
      {
        ...handlers(),
        ...runtimeHandlers(version, () => catalog),
        download_ollama_runtime: () => new Promise<never>(() => {}),
      },
      { target: "0.40.13" }
    );
    await ctx.boot();
    modelBox(ctx.cardOf(QWEN))!.click();
    void ctx.mod.ensureLocalRuntimeReady();
    await settle();
    await ctx.render();

    expect(modelBox(ctx.cardOf(QWEN))!.checked).toBe(true);
    expect(bar(ctx).disabled).toBe(true);
    expect(bar(ctx).title).toContain("runtime");
    bar(ctx).click();
    await settle();
    expect(pulled(ctx)).toHaveLength(0);
    expect(toastTitles(ctx.showToast)).not.toContain("Download not started");
  });
});
