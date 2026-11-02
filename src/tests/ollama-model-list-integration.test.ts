/**
 * The Models section as the user sees it: one zebra list of collapsible rows, built from the
 * installed models, the catalog, the registry status and the runtime gate. The component itself is
 * covered by ollama-model-list.test.ts; this file checks what the list is fed.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  CURRENT,
  NEW_DIGEST,
  OLD,
  OLD_DIGEST,
  load,
  modelBox,
  runtime,
  runtimeHandlers,
  settle,
  toastNamed,
  type Ctx,
  type InstalledRow,
} from "./ollama-test-harness";
import { resetModelListState } from "../ollama-model-list";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));
vi.mock("../toast", () => ({ showToast: vi.fn() }));

const version = { current: "0.40.13" };
const HF = "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M";

afterEach(() => {
  resetModelListState();
  document.body.innerHTML = "";
});

const installed = (): InstalledRow[] => [
  { name: CURRENT, size_bytes: 6_583_656_505, digest: NEW_DIGEST, modified_at: "2026-11-02T11:02:26+01:00", parameter_size: "7.5B", quantization_level: "Q4_K_M" },
  { name: OLD, size_bytes: 9_608_350_718, digest: OLD_DIGEST, modified_at: "2026-07-23T10:12:00+02:00" },
  { name: HF, size_bytes: 5_970_000_000, digest: "487eacf5e94b".padEnd(64, "0"), modified_at: "2026-10-30T18:20:00+01:00" },
  { name: "my-private:latest", size_bytes: 2_100_000_000, digest: "abcdef012345".padEnd(64, "1") },
];

const DETAILS = {
  model: CURRENT,
  local: { license: "apache-2.0", capabilities: ["completion", "tools"], context_length: 131072, parameter_size: "7.5B" },
  remote: { source: "ollama", updated_at: "2026-10-12T12:00:00Z", downloads_text: "28.4M", description: "From the library page." },
};

function handlers(extra: Record<string, (args?: Record<string, unknown>) => unknown> = {}) {
  return {
    ...runtimeHandlers(version, () => [runtime("0.40.13", { installed: true, recommended: true, latest: true })]),
    fetch_ollama_models_with_size: installed,
    check_ollama_model_updates: (args?: Record<string, unknown>) =>
      ((args?.models as Array<{ name: string }>) ?? []).map((model) => {
        if (model.name === "my-private:latest") return { name: model.name, status: "local" };
        if (model.name === OLD) return { name: model.name, status: "outdated", remote_digest: NEW_DIGEST, remote_size_bytes: 6_583_658_527 };
        return { name: model.name, status: "current", remote_digest: model.name === CURRENT ? NEW_DIGEST : "ee".repeat(32) };
      }),
    check_ollama_model_requirements: (args?: Record<string, unknown>) =>
      ((args?.models as string[]) ?? []).map((name) => ({
        name,
        requires: name === "gemma4:e2b" ? "0.99.0" : null,
        runtime_version: "0.40.13",
        compatible: name === "gemma4:e2b" ? false : null,
        detail: null,
      })),
    get_ollama_model_details: () => DETAILS,
    open_model_page: () => undefined,
    ...extra,
  };
}

const names = (ctx: Ctx) =>
  Array.from(ctx.container().querySelectorAll<HTMLElement>(".ollama-model-row")).map((row) => row.dataset.model!);
const actionsOf = (ctx: Ctx, name: string) =>
  Array.from(ctx.cardOf(name)!.querySelectorAll<HTMLButtonElement>("button[data-action]")).map((b) => b.dataset.action);
const toggle = (ctx: Ctx, name: string) => ctx.cardOf(name)!.querySelector<HTMLButtonElement>(".ollama-model-toggle")!;

describe("one list", () => {
  it("replaces the two card sections by a single zebra list", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    expect(ctx.container().querySelectorAll(".ollama-model-rows")).toHaveLength(1);
    expect(ctx.container().textContent).not.toContain("Your Models");
    expect(ctx.container().textContent).not.toContain("Available Models");
    const rows = Array.from(ctx.container().querySelectorAll<HTMLElement>(".ollama-model-row"));
    expect(rows.map((_, i) => rows[i].classList.contains(i % 2 === 0 ? "is-odd" : "is-even"))).not.toContain(false);
  });

  it("orders: active first, then installed, then downloadable; recommended before the rest, then alphabetical", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    const order = names(ctx);
    expect(order[0]).toBe(CURRENT);
    const states = Array.from(ctx.container().querySelectorAll<HTMLElement>(".ollama-model-row")).map((row) => row.dataset.state);
    const firstAvailable = states.indexOf("available");
    expect(states.slice(0, firstAvailable)).toEqual(["active", ...Array(firstAvailable - 1).fill("installed")]);
    expect(states.slice(firstAvailable).every((state) => state === "available")).toBe(true);
    // Installed group: all four installed models, none of them twice.
    expect(order.slice(1, firstAvailable).sort()).toEqual([HF, OLD, "my-private:latest"].sort());
    // Downloadable group: the lean catalog without the installed gemma4:e4b, alphabetical by title
    // (neither is recommended).
    expect(order.slice(firstAvailable)).toEqual(["gemma4:e2b", "qwen3.5:4b"]);
    const titles = Array.from(ctx.container().querySelectorAll<HTMLElement>(".ollama-model-row"))
      .slice(firstAvailable + 1)
      .map((row) => row.querySelector(".qwen-card-name")!.textContent!);
    expect(titles).toEqual([...titles].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" })));
    // No model is listed twice (an installed catalog model is not offered again).
    expect(new Set(order.map((n) => n.toLowerCase())).size).toBe(order.length);
  });

  it("offers Activate and Delete for installed, a tick box for the rest and nothing but chips for the active model", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    expect(actionsOf(ctx, CURRENT)).toEqual([]);
    expect(ctx.cardOf(CURRENT)!.querySelector(".qwen-card-badge--active")!.textContent).toBe("Active");
    expect(actionsOf(ctx, HF)).toEqual(["activate", "delete"]);
    expect(actionsOf(ctx, "my-private:latest")).toEqual(["activate", "delete"]);
    // Not installed: no per-row button, a native checkbox instead; installed rows have none.
    expect(actionsOf(ctx, "qwen3.5:4b")).toEqual([]);
    const box = modelBox(ctx.cardOf("qwen3.5:4b"))!;
    expect(box.type).toBe("checkbox");
    expect(box.getAttribute("aria-label")).toBe("Select Qwen 3.5 4B — Standard for download");
    expect(modelBox(ctx.cardOf(CURRENT))).toBeNull();
    expect(modelBox(ctx.cardOf(HF))).toBeNull();
    expect(modelBox(ctx.cardOf("my-private:latest"))).toBeNull();
    expect(ctx.cardOf(HF)!.querySelector('button[data-action="delete"]')!.textContent).toBe("Delete");
  });

  it("keeps Update/Replace, the Outdated chip and its hint on the compact row", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    const old = ctx.cardOf(OLD)!;
    expect(old.querySelector(".qwen-card-badge--update-outdated")!.textContent).toBe("Outdated");
    expect(old.querySelector(".qwen-card-hint--outdated")!.textContent).toContain("newer build available");
    const replace = old.querySelector<HTMLButtonElement>('button[data-action="replace"]')!;
    expect(replace.dataset.target).toBe(CURRENT);
    expect(replace.textContent).toBe(`Replace with ${CURRENT}`);
    expect(old.dataset.expanded).toBe("false");
  });

  it("shows the runtime requirement as a chip only for a real conflict, blocks the tick box and says why", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    const card = ctx.cardOf("gemma4:e2b")!;
    expect(card.querySelector(".qwen-card-badge--needs-runtime")!.textContent).toBe("Needs Ollama ≥ 0.99.0");
    expect(modelBox(card)!.disabled).toBe(true);
    expect(modelBox(card)!.title).toContain("update the runtime first");
    expect(card.querySelector(".qwen-card-hint--runtime")!.textContent).toBeTruthy();
    expect(card.querySelector<HTMLButtonElement>(".ollama-model-toggle")).not.toBeNull();
    // A model without a conflict carries no runtime chip at all.
    expect(ctx.cardOf("qwen3.5:4b")!.querySelector(".qwen-card-badge--runtime-ok, .qwen-card-badge--needs-runtime")).toBeNull();
  });

  it("marks the stable build as Recommended and the proven one as Verified working", async () => {
    const catalogJson = [
      {
        name: "qwen3.5:4b", label: "Qwen", source: "ollama", family: "q", status: "recommended", profile: "p", size_gb: 3.3, description: "d",
        functional: { verified_at: "2026-11-02", runtime: "0.40.7", latency_p50_ms: 900 },
      },
      { name: "other:1b", label: "Other", source: "ollama", family: "o", status: "supported", profile: "p", size_gb: 1, description: "d" },
    ];
    const ctx = await load(handlers({ fetch_ollama_models_with_size: () => [] }), { catalogJson });
    await ctx.boot();
    const first = ctx.cardOf("qwen3.5:4b")!;
    expect(first.querySelector(".qwen-card-badge--curated")!.textContent).toBe("Recommended");
    expect(first.querySelector(".qwen-card-badge--functional")!.textContent).toBe("Verified working");
    expect(ctx.cardOf("other:1b")!.textContent).not.toContain("Recommended");
    expect(names(ctx)).toEqual(["qwen3.5:4b", "other:1b"]);
  });
});

describe("details", () => {
  it("are not requested at start, only when a row is opened, once per model", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    expect(ctx.callsOf("get_ollama_model_details")).toHaveLength(0);

    toggle(ctx, CURRENT).click();
    await settle();
    expect(ctx.callsOf("get_ollama_model_details")).toHaveLength(1);
    expect(ctx.callsOf("get_ollama_model_details")[0].args).toEqual({ model: CURRENT });

    toggle(ctx, CURRENT).click();
    toggle(ctx, CURRENT).click();
    await settle();
    expect(ctx.callsOf("get_ollama_model_details")).toHaveLength(1);
    expect(ctx.cardOf(CURRENT)!.textContent).toContain("131,072 tokens (128K)");
    expect(ctx.cardOf(CURRENT)!.textContent).toContain("Updated on ollama.com");
  });

  it("show Downloaded and the digest from the installed list, and the catalog's release evidence", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    toggle(ctx, CURRENT).click();
    await settle();
    const text = ctx.cardOf(CURRENT)!.textContent!;
    expect(text).toMatch(/Downloaded2 Nov 2026/);
    expect(text).toContain("Digestdc35e8d9c606");
    expect(text).toContain("Released2 Mar 2026");
    expect(text).toContain("Size on disk6.6 GB");
  });

  it("keep a row open and its details through a refresh of the installed models and a re-render", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    toggle(ctx, HF).click();
    await settle();
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    await ctx.render();
    expect(ctx.cardOf(HF)!.dataset.expanded).toBe("true");
    expect(ctx.cardOf(HF)!.textContent).toContain("131,072 tokens (128K)");
    expect(ctx.callsOf("get_ollama_model_details")).toHaveLength(1);
  });

  it("keep keyboard focus on the same control through a re-render", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    toggle(ctx, HF).focus();
    await ctx.render();
    expect(document.activeElement).toBe(toggle(ctx, HF));
    const activate = ctx.cardOf(HF)!.querySelector<HTMLButtonElement>('button[data-action="activate"]')!;
    activate.focus();
    await ctx.render();
    expect(document.activeElement).toBe(ctx.cardOf(HF)!.querySelector('button[data-action="activate"]'));
  });

  it("still list a model with only its known facts when the backend has no details command", async () => {
    const ctx = await load(
      handlers({
        get_ollama_model_details: () => {
          throw new Error("unknown command");
        },
      })
    );
    await ctx.boot();
    toggle(ctx, HF).click();
    await settle();
    const text = ctx.cardOf(HF)!.textContent!;
    expect(text).toContain("More details could not be loaded");
    expect(text).toContain("Size on disk6.0 GB");
    expect(ctx.showToast).not.toHaveBeenCalled();
  });

  it("an empty answer (older backend) is not an error", async () => {
    const ctx = await load(handlers({ get_ollama_model_details: () => ({}) }));
    await ctx.boot();
    toggle(ctx, HF).click();
    await settle();
    expect(ctx.cardOf(HF)!.textContent).not.toContain("could not be loaded");
    expect(ctx.cardOf(HF)!.textContent).toContain("Downloaded");
  });
});

describe("links", () => {
  it("open the model page through the backend and report a failure as a toast", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    toggle(ctx, CURRENT).click();
    await settle();
    const link = ctx.cardOf(CURRENT)!.querySelector<HTMLButtonElement>("[data-open-url]")!;
    expect(link.textContent).toBe("Open on Ollama");
    link.click();
    await settle();
    expect(ctx.callsOf("open_model_page")[0].args).toEqual({ url: `https://ollama.com/library/${CURRENT}` });

    const failing = await load(
      handlers({
        open_model_page: () => {
          throw new Error("Failed to open the page");
        },
      })
    );
    await failing.boot();
    toggle(failing, HF).click();
    await settle();
    const hfLink = failing.cardOf(HF)!.querySelector<HTMLButtonElement>("[data-open-url]")!;
    expect(hfLink.textContent).toBe("Open on Hugging Face");
    hfLink.click();
    await settle();
    expect(toastNamed(failing.showToast, "Could not open the page").message).toContain("Failed to open the page");
  });

  it("offer no link for a local-only model", async () => {
    const ctx = await load(handlers());
    await ctx.boot();
    toggle(ctx, "my-private:latest").click();
    await settle();
    expect(ctx.cardOf("my-private:latest")!.querySelector("[data-open-url]")).toBeNull();
  });
});

describe("modelPageLinks", () => {
  it("builds the Ollama and Hugging Face pages from a pull reference", async () => {
    const ctx = await load(handlers());
    const links = ctx.mod.modelPageLinks;
    expect(links("gemma4:e4b")).toEqual([{ label: "Open on Ollama", url: "https://ollama.com/library/gemma4:e4b" }]);
    expect(links("gemma4")).toEqual([{ label: "Open on Ollama", url: "https://ollama.com/library/gemma4" }]);
    expect(links("someone/custom:q4")).toEqual([{ label: "Open on Ollama", url: "https://ollama.com/someone/custom:q4" }]);
    expect(links("registry.ollama.ai/library/qwen3.5:4b")[0].url).toBe("https://ollama.com/library/qwen3.5:4b");
    expect(links(HF)).toEqual([{ label: "Open on Hugging Face", url: "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF" }]);
    expect(links("HF.co/a/b")[0].url).toBe("https://huggingface.co/a/b");
  });

  it("builds nothing for local models and for names that could change the URL", async () => {
    const ctx = await load(handlers());
    const links = ctx.mod.modelPageLinks;
    expect(links("mine:latest", "local")).toEqual([]);
    for (const bad of ["", "a b", "a/b/c/d:e", "../x", "x:../y", "a?b=c", "a#b", "hf.co/onlyone", "hf.co/a/b/c", "hf.co/a/../b", "a@b.c/d", "a/b:c\nd"]) {
      expect(links(bad), bad).toEqual([]);
    }
  });
});
