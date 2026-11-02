/**
 * Everything the Ollama UI shows comes from outside: model names from the user and registries,
 * digests and sizes from the backend, descriptions from the catalog file, errors from servers.
 * Hostile values must render as text, never as elements or attributes.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  NEW_DIGEST,
  OLD_DIGEST,
  downloadSelected,
  load,
  modelBox,
  runtime,
  runtimeHandlers,
  settle,
  type InstalledRow,
} from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

// The real toast module is used on purpose: it is one of the places that show server text.

declare global {
  interface Window {
    __pwn?: number;
  }
}

const IMG = '<img src=x onerror="window.__pwn=1">';
const SCRIPT = '"><script>window.__pwn=2</script>';
const version = { current: "0.40.7" };

afterEach(() => {
  delete window.__pwn;
  document.body.innerHTML = "";
});

/** No element, handler attribute or script made it out of the text. */
function expectInert(root: ParentNode) {
  expect(root.querySelector("img, script, iframe, svg")).toBeNull();
  const handlers = Array.from(root.querySelectorAll("*")).flatMap((element) =>
    element.getAttributeNames().filter((name) => name.startsWith("on"))
  );
  expect(handlers).toEqual([]);
  expect(window.__pwn).toBeUndefined();
}

const baseHandlers = (extra: Record<string, (args?: Record<string, unknown>) => unknown> = {}) => ({
  ...runtimeHandlers(version, () => [runtime("0.40.7", { installed: true, recommended: true }), runtime("0.40.13", { latest: true })]),
  fetch_ollama_models_with_size: () => [] as InstalledRow[],
  ...extra,
});

describe("catalog cards", () => {
  const entry = (overrides: Record<string, unknown>) => ({
    name: "ok:1b",
    label: "Ok",
    source: "ollama",
    family: "f",
    status: "supported",
    profile: "p",
    size_gb: 1,
    description: "Fine.",
    ...overrides,
  });

  it("renders hostile name, label, description, note and profile as text", async () => {
    const hostile = `evil${SCRIPT}:1b`;
    const hf = `hf.co/${IMG}/repo:Q4_K_M`;
    const ctx = await load(baseHandlers(), {
      catalogJson: [
        entry({ name: hostile, label: IMG, description: SCRIPT, note: IMG, verified_at: "2026-11-02" }),
        entry({ name: hf, label: `HF ${IMG}`, source: "huggingface", description: IMG }),
      ],
    });
    await ctx.boot();

    expectInert(ctx.container());
    expect(ctx.cardOf(hostile)!.querySelector(".qwen-card-name")!.textContent).toBe(IMG);
    // The description lives in the expanded panel of the row (opening the row rebuilds the list).
    ctx.cardOf(hostile)!.querySelector<HTMLElement>(".ollama-model-toggle")!.click();
    const card = ctx.cardOf(hostile)!;
    expect(card.querySelector(".qwen-card-desc")!.textContent).toContain(SCRIPT);
    expectInert(ctx.container());
    // The pull reference survives the round trip untouched: ticked rows are pulled by their exact name.
    expect(card.dataset.model).toBe(hostile);
    expect(ctx.cardOf(hf)!.dataset.model).toBe(hf);
    expect(modelBox(card)!.getAttribute("aria-label")).toBe(`Select ${IMG} for download`);
    expect(modelBox(ctx.cardOf(hf))!.getAttribute("aria-label")).toBe(`Select HF ${IMG} for download`);
    downloadSelected(ctx.cardOf(hostile));
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model").map((call) => call.args)).toEqual([{ model: hostile }]));
  });

  it("renders hostile pull status lines as text and keeps the bar a plain width", async () => {
    const ctx = await load(baseHandlers(), { catalogJson: [entry({})] });
    await ctx.boot();
    ctx.state.ollamaPullProgress.set("ok:1b", { model: "ok:1b", status: IMG, digest: SCRIPT, total: 100, completed: 50 });
    ctx.emit("ollama:pull-progress", { model: "ok:1b", status: IMG, digest: SCRIPT, total: 100, completed: 50 });
    await settle();
    expectInert(ctx.container());
    expect(ctx.container().querySelector(".qwen-card-progress-text")!.textContent).toContain(IMG);
    expect(ctx.container().querySelector<HTMLElement>(".qwen-card-progress-fill")!.getAttribute("style")).toMatch(/^width: \d+%$/);
  });

  it("renders a hostile requirement and runtime hint as text", async () => {
    const ctx = await load(
      baseHandlers({
        check_ollama_model_requirements: (args) =>
          ((args?.models as string[]) ?? []).map((name) => ({ name, requires: SCRIPT, runtime_version: IMG, compatible: false })),
      }),
      { catalogJson: [entry({})] }
    );
    await ctx.boot();
    expectInert(ctx.container());
    expect(ctx.cardOf("ok:1b")!.textContent).toContain(SCRIPT);
  });
});

describe("installed models", () => {
  const hostile = `evil${IMG}:latest`;
  const rows = (): InstalledRow[] => [
    {
      name: hostile,
      size_bytes: 1_000_000_000,
      digest: IMG + OLD_DIGEST,
      modified_at: SCRIPT,
      parameter_size: IMG,
      quantization_level: SCRIPT,
      family: IMG,
    },
  ];
  const outdated = (args?: Record<string, unknown>) =>
    ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({
      name: model.name,
      status: "outdated",
      remote_digest: IMG + NEW_DIGEST,
      remote_size_bytes: 6_583_658_527,
      detail: SCRIPT,
    }));

  it("renders name, digest, metadata and registry hint as text", async () => {
    const ctx = await load(baseHandlers({ fetch_ollama_models_with_size: rows, check_ollama_model_updates: outdated }), {
      settingsModel: "other:1b",
    });
    await ctx.boot();
    expectInert(ctx.container());
    const card = ctx.cardOf(hostile)!;
    expect(card.querySelector(".qwen-card-tag")!.textContent).toBe(hostile);
    expect(card.textContent).toContain("Outdated");
    expect(card.querySelector<HTMLButtonElement>('button[data-action="activate"]')!.dataset.model).toBe(hostile);
  });

  it("shows a hostile name in the confirmation dialog as text and passes it on unchanged", async () => {
    const ctx = await load(
      baseHandlers({
        fetch_ollama_models_with_size: rows,
        check_ollama_model_updates: outdated,
        backup_ollama_model: () => `${hostile}.bak`,
        pull_ollama_model: () => undefined,
        finalize_ollama_model_update: () => ({ verified: true, restored: false }),
      }),
      { settingsModel: "other:1b" }
    );
    await ctx.boot();
    ctx.cardOf(hostile)!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await settle();

    const dialog = document.querySelector<HTMLElement>(".ollama-confirm-dialog")!;
    expect(dialog.textContent).toContain(hostile);
    expectInert(dialog);
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("backup_ollama_model")).toHaveLength(1));
    expect(ctx.callsOf("backup_ollama_model")[0].args).toEqual({ model: hostile });
    ctx.emit("ollama:pull-complete", { model: hostile });
    await vi.waitFor(() => expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(1));
  });

  it("shows server error text in toasts as text", async () => {
    const ctx = await load(
      baseHandlers({
        pull_ollama_model: () => {
          throw `pull failed ${IMG}${SCRIPT}`;
        },
      }),
      { catalogJson: [{ name: "ok:1b", label: "Ok", source: "ollama", family: "f", status: "supported", profile: "p", size_gb: 1, description: "Fine." }] }
    );
    await ctx.boot();
    downloadSelected(ctx.cardOf("ok:1b"));
    await vi.waitFor(() => expect(document.querySelector("#toast-container .toast")).not.toBeNull());
    const toast = document.querySelector<HTMLElement>("#toast-container")!;
    expect(toast.querySelector(".toast-message")!.textContent).toContain(IMG);
    expectInert(toast);
  });

  it("shows the inconclusive notice (backend detail) as text", async () => {
    const ctx = await load(
      baseHandlers({
        fetch_ollama_models_with_size: () => [{ name: "m:1b", size_bytes: 1_000_000_000, digest: OLD_DIGEST }],
        check_ollama_model_updates: (args) =>
          ((args?.models as Array<{ name: string }>) ?? []).map((model) => ({ name: model.name, status: "outdated", remote_digest: NEW_DIGEST })),
        backup_ollama_model: () => `m:1b.${IMG}`,
        pull_ollama_model: () => undefined,
        finalize_ollama_model_update: () => ({ verified: false, restored: false, inconclusive: true, detail: `${IMG}${SCRIPT}` }),
      }),
      { settingsModel: "other:1b" }
    );
    await ctx.boot();
    ctx.cardOf("m:1b")!.querySelector<HTMLButtonElement>('button[data-action="update"]')!.click();
    await settle();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: "m:1b" });
    await vi.waitFor(() => expect(ctx.cardOf("m:1b")?.querySelector(".qwen-card-hint--check")).toBeTruthy());
    expectInert(ctx.container());
    expect(ctx.cardOf("m:1b")!.querySelector(".qwen-card-hint--check")!.textContent).toContain(IMG);
  });
});

describe("download popup", () => {
  async function popup() {
    document.body.insertAdjacentHTML("beforeend", '<div id="download-progress-popup" hidden></div>');
    const module = await import("../download-progress-popup");
    return { render: module.renderDownloadProgressPopup, element: () => document.getElementById("download-progress-popup")! };
  }

  it("renders model names, status lines, file names and runtime messages as text", async () => {
    const ctx = await load(baseHandlers());
    const { render, element } = await popup();
    ctx.state.ollamaPullProgress.set(`evil${IMG}:1b`, { model: `evil${IMG}:1b`, status: SCRIPT });
    ctx.state.quantizeProgress.set(`file${IMG}.gguf`, { file_name: `file${IMG}.gguf`, phase: "running", message: SCRIPT } as never);
    ctx.state.modelProgress.set(`whisper${IMG}`, { id: `whisper${IMG}`, downloaded: 10, total: 100 } as never);
    ctx.mod.setOllamaRuntimeInstallProgress({ stage: "download_runtime", message: IMG, version: SCRIPT });

    render();
    expectInert(element());
    const text = element().textContent ?? "";
    expect(text).toContain(`evil${IMG}:1b`);
    expect(text).toContain(`file${IMG}.gguf`);
    expect(text).toContain(`Runtime ${SCRIPT}`);
    expect(element().querySelectorAll(".dpp-item")).toHaveLength(4);
  });
});
