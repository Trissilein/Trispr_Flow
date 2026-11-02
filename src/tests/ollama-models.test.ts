import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

import {
  OLLAMA_RECOMMENDED_MODELS,
  buildInstalledOllamaInventory,
  findInstalledModel,
  formatBytesGb,
  formatCatalogModelMeta,
  formatInstalledModelMeta,
  modelUpdateChip,
  modelUpdateHint,
  resolveCuratedModelState,
  shortDigest,
  type InstalledOllamaModel,
  type OllamaModelUpdateInfo,
} from "../ollama-models";

const OLD_DIGEST = "c6eb396dbd5992bbe3f5cdb947e8bbc0ee413d7c17e2beaae69f5d569cf982eb";
const NEW_DIGEST = "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c";
const UNSLOTH_E4B = "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M";

describe("ollama-models helpers", () => {
  describe("resolveCuratedModelState", () => {
    it("marks a selected but missing model as missing instead of active", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", false, false)).toBe("missing");
    });

    it("marks a selected installed model as active", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", true, false)).toBe("active");
    });

    it("prefers downloading state while a pull is in progress", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", false, true)).toBe("downloading");
    });
  });

  describe("buildInstalledOllamaInventory", () => {
    it("sorts active first, then curated, then custom models", () => {
      const inventory = buildInstalledOllamaInventory(
        [
          { name: "custom-import:latest", size_bytes: 3 * 1024 * 1024 * 1024 },
          { name: "qwen3.5:4b", size_bytes: 2 * 1024 * 1024 * 1024 },
          { name: "gemma4:e4b", size_bytes: 10 * 1024 * 1024 * 1024 },
        ],
        "gemma4:e4b"
      );

      expect(inventory.map((item) => item.name)).toEqual([
        "gemma4:e4b",
        "qwen3.5:4b",
        "custom-import:latest",
      ]);
      expect(inventory[0].is_active).toBe(true);
      expect(inventory[0].can_activate).toBe(false);
      expect(inventory[0].can_uninstall).toBe(false);
      expect(inventory[1].is_curated).toBe(true);
      expect(inventory[2].is_curated).toBe(false);
    });

    it("deduplicates installed tags case-insensitively", () => {
      const inventory = buildInstalledOllamaInventory(
        [
          { name: "QWEN3.5:4B", size_bytes: 2 * 1024 * 1024 * 1024 },
          { name: "qwen3.5:4b", size_bytes: 2 * 1024 * 1024 * 1024 },
        ],
        ""
      );

      expect(inventory).toHaveLength(1);
      expect(inventory[0].size_label).toContain("GB");
    });

    it("carries digest, pulled date, parameters and quantization into a meta line", () => {
      const [item] = buildInstalledOllamaInventory(
        [
          {
            name: "gemma4:e4b-it-q4_K_M",
            size_bytes: 9_608_350_718,
            digest: OLD_DIGEST,
            modified_at: "2026-07-23T12:00:00Z",
            family: "gemma4",
            parameter_size: "8.0B",
            quantization_level: "Q4_K_M",
          },
        ],
        ""
      );

      expect(item.size_label).toBe("9.6 GB");
      expect(item.digest_short).toBe("c6eb396dbd59");
      expect(item.meta_label).toBe("c6eb396dbd59 · pulled 23 Jul 2026 · 8.0B · Q4_K_M");
    });

    it("shows old and current gemma4:e4b builds apart: chip, hint and meta differ", () => {
      const installed: InstalledOllamaModel[] = [
        { name: "gemma4:e4b-it-q4_K_M", size_bytes: 9_608_350_718, digest: OLD_DIGEST, modified_at: "2026-07-23T12:00:00Z", parameter_size: "8.0B", quantization_level: "Q4_K_M" },
        { name: "gemma4:e4b", size_bytes: 6_583_658_527, digest: NEW_DIGEST, modified_at: "2026-11-02T12:00:00Z", parameter_size: "7.5B", quantization_level: "Q4_K_M" },
      ];
      const updates = new Map<string, OllamaModelUpdateInfo>([
        ["gemma4:e4b-it-q4_k_m", { name: "gemma4:e4b-it-q4_K_M", status: "outdated", remote_digest: NEW_DIGEST, remote_size_bytes: 6_583_658_527 }],
        ["gemma4:e4b", { name: "gemma4:e4b", status: "current", remote_digest: NEW_DIGEST, remote_size_bytes: 6_583_658_527 }],
      ]);

      const inventory = buildInstalledOllamaInventory(installed, "gemma4:e4b-it-q4_K_M", updates);
      const old = inventory.find((item) => item.name === "gemma4:e4b-it-q4_K_M")!;
      const current = inventory.find((item) => item.name === "gemma4:e4b")!;

      expect(old.is_active).toBe(true);
      expect(old.update_chip?.label).toBe("Outdated");
      expect(old.update_hint).toBe("newer build available (registry dc35e8d9c606, 6.6 GB)");
      expect(old.size_label).toBe("9.6 GB");
      expect(current.update_chip?.label).toBe("Current");
      expect(current.update_hint).toBeNull();
      expect(current.size_label).toBe("6.6 GB");
      expect(old.meta_label).not.toBe(current.meta_label);
    });

    it("shows no chip when no registry status is known", () => {
      const [item] = buildInstalledOllamaInventory([{ name: "qwen3.5:4b", size_bytes: 1 }], "");
      expect(item.update_chip).toBeNull();
      expect(item.update_hint).toBeNull();
      expect(item.meta_label).toBe("");
    });
  });

  describe("formatBytesGb", () => {
    it("uses decimal GB like the registries (not GiB)", () => {
      expect(formatBytesGb(9_608_350_718)).toBe("9.6 GB");
      expect(formatBytesGb(6_583_658_527)).toBe("6.6 GB");
      expect(formatBytesGb(1_000_000_000)).toBe("1.0 GB");
      expect(formatBytesGb(2 * 1024 * 1024 * 1024)).toBe("2.1 GB");
    });

    it("reports unknown sizes", () => {
      expect(formatBytesGb(0)).toBe("Unknown size");
      expect(formatBytesGb(Number.NaN)).toBe("Unknown size");
      expect(formatBytesGb(-5)).toBe("Unknown size");
    });
  });

  describe("meta lines", () => {
    it("shortens digests to 12 characters and tolerates a sha256 prefix", () => {
      expect(shortDigest(OLD_DIGEST)).toBe("c6eb396dbd59");
      expect(shortDigest(`sha256:${OLD_DIGEST}`)).toBe("c6eb396dbd59");
      expect(shortDigest(undefined)).toBe("");
    });

    it("parses Ollama's 7-digit fractional timestamps with offset", () => {
      const meta = formatInstalledModelMeta({
        name: "x",
        size_bytes: 1,
        modified_at: "2026-07-23T20:01:21.3432992+02:00",
      });
      expect(meta).toMatch(/^pulled \d{1,2} Jul 2026$/);
    });

    it("leaves out parts it does not know", () => {
      expect(formatInstalledModelMeta({ name: "x", size_bytes: 1 })).toBe("");
      expect(formatInstalledModelMeta({ name: "x", size_bytes: 1, digest: OLD_DIGEST, modified_at: "garbage" })).toBe("c6eb396dbd59");
    });

    it("describes catalog entries by source, status and verification date", () => {
      const e4b = OLLAMA_RECOMMENDED_MODELS.find((spec) => spec.name === "gemma4:e4b")!;
      expect(formatCatalogModelMeta({ ...e4b, verified_at: "2026-11-02" })).toBe(
        "Ollama library · recommended · size checked 2 Nov 2026"
      );
      // The lean catalog has no Hugging Face entry; the formatter still knows the source.
      expect(formatCatalogModelMeta({ ...e4b, source: "huggingface", status: "supported", verified_at: undefined })).toBe(
        "Hugging Face"
      );
    });
  });

  describe("registry status chips", () => {
    it("maps statuses to Current / Outdated / Local and hides unknown", () => {
      expect(modelUpdateChip({ name: "a", status: "current" })?.label).toBe("Current");
      expect(modelUpdateChip({ name: "a", status: "outdated" })?.label).toBe("Outdated");
      expect(modelUpdateChip({ name: "a", status: "local" })?.label).toBe("Local");
      expect(modelUpdateChip({ name: "a", status: "unknown" })).toBeNull();
      expect(modelUpdateChip(undefined)).toBeNull();
    });

    it("hints only for outdated models and degrades gracefully without details", () => {
      expect(modelUpdateHint({ name: "a", status: "current" })).toBeNull();
      expect(modelUpdateHint({ name: "a", status: "local" })).toBeNull();
      expect(
        modelUpdateHint({ name: "a", status: "outdated", remote_digest: NEW_DIGEST, remote_size_bytes: 6_583_658_527 })
      ).toBe("newer build available (registry dc35e8d9c606, 6.6 GB)");
      expect(modelUpdateHint({ name: "a", status: "outdated", remote_digest: NEW_DIGEST, remote_size_bytes: null })).toBe(
        "newer build available (registry dc35e8d9c606)"
      );
      expect(modelUpdateHint({ name: "a", status: "outdated" })).toBe("newer build available");
    });
  });

  describe("OLLAMA_RECOMMENDED_MODELS (fed from model-catalog.json)", () => {
    it("keeps the previous shape, with the lean list of three entries", () => {
      for (const spec of OLLAMA_RECOMMENDED_MODELS) {
        expect(typeof spec.name).toBe("string");
        expect(typeof spec.label).toBe("string");
        expect(typeof spec.profile).toBe("string");
        expect(typeof spec.description).toBe("string");
        expect(spec.size_gb).toBeGreaterThan(0);
      }
      expect(OLLAMA_RECOMMENDED_MODELS.map((spec) => spec.name)).toEqual(["gemma4:e4b", "qwen3.5:4b", "gemma4:e2b"]);
    });

    it("no longer carries the stale pre-verification sizes", () => {
      const size = (name: string) => OLLAMA_RECOMMENDED_MODELS.find((spec) => spec.name === name)!.size_gb;
      expect(size("gemma4:e4b")).toBeLessThan(7);
      expect(size("gemma4:e4b")).toBeGreaterThan(6);
      expect(size("qwen3.5:4b")).toBeGreaterThan(3);
    });
  });

  describe("Hugging Face refs", () => {
    // Ollama keeps the case of the pulled ref and compares names case-insensitively
    // (types/model/name.go), so /api/tags may list either spelling.
    it("finds an installed hf.co model whatever case /api/tags uses", () => {
      const lower: InstalledOllamaModel[] = [
        { name: "hf.co/unsloth/gemma-4-e4b-it-gguf:q4_k_m", size_bytes: 5_970_000_000 },
      ];
      const exact: InstalledOllamaModel[] = [{ name: UNSLOTH_E4B, size_bytes: 5_970_000_000 }];

      expect(findInstalledModel(lower, UNSLOTH_E4B)?.name).toBe("hf.co/unsloth/gemma-4-e4b-it-gguf:q4_k_m");
      expect(findInstalledModel(exact, UNSLOTH_E4B)?.name).toBe(UNSLOTH_E4B);
      expect(findInstalledModel(exact, "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q8_0")).toBeUndefined();
      expect(findInstalledModel(exact, "gemma4:e4b")).toBeUndefined();
      expect(findInstalledModel(exact, "")).toBeUndefined();
    });

    it("lists an installed hf.co model that is not in the catalog as a plain model, matched whatever the case", () => {
      const inventory = buildInstalledOllamaInventory(
        [
          { name: "hf.co/google/gemma-4-e4b-it-qat-q4_0-gguf:latest", size_bytes: 6_150_000_000 },
          { name: UNSLOTH_E4B, size_bytes: 5_970_000_000 },
        ],
        UNSLOTH_E4B.toLowerCase()
      );
      // The lean catalog has no hf.co entry: both are installed foreign models, still manageable.
      expect(inventory.some((item) => item.is_curated)).toBe(false);
      expect(inventory[0].name).toBe(UNSLOTH_E4B);
      expect(inventory[0].is_active).toBe(true);
      expect(inventory[1].curated_spec).toBeUndefined();
      expect(inventory[1].can_uninstall).toBe(true);
    });
  });
});

describe("model manager with registry status (fresh module per test)", () => {
  const HOUR = 60 * 60_000;

  type CheckInput = Array<{ name: string; digest: string }>;
  type Load = Awaited<ReturnType<typeof load>>;

  async function load(opts: { models: InstalledOllamaModel[]; check?: (models: CheckInput) => unknown }) {
    vi.resetModules();
    const { invoke } = await import("@tauri-apps/api/core");
    const { showToast } = await import("../toast");
    const state = await import("../state");
    const mod = await import("../ollama-models");
    let models = opts.models;
    const checkCalls: CheckInput[] = [];
    vi.mocked(invoke).mockImplementation((async (command: string, args?: Record<string, unknown>) => {
      if (command === "fetch_ollama_models_with_size") return models;
      if (command === "check_ollama_model_updates") {
        const requested = (args as { models: CheckInput }).models;
        checkCalls.push(requested);
        const result = opts.check ? opts.check(requested) : [];
        if (result instanceof Error) throw result;
        return result;
      }
      return {};
    }) as never);
    return {
      invoke,
      showToast,
      state,
      mod,
      checkCalls,
      setModels: (next: InstalledOllamaModel[]) => {
        models = next;
      },
    };
  }

  const status =
    (statuses: Record<string, OllamaModelUpdateInfo["status"]>) =>
    (requested: CheckInput): OllamaModelUpdateInfo[] =>
      requested.map((model) => ({
        name: model.name,
        status: statuses[model.name] ?? "unknown",
        remote_digest: NEW_DIGEST,
        remote_size_bytes: 6_583_658_527,
      }));

  const oldAndNew: InstalledOllamaModel[] = [
    { name: "gemma4:e4b-it-q4_K_M", size_bytes: 9_608_350_718, digest: OLD_DIGEST, modified_at: "2026-07-23T12:00:00Z" },
    { name: "gemma4:e4b", size_bytes: 6_583_658_527, digest: NEW_DIGEST, modified_at: "2026-11-02T12:00:00Z" },
    { name: "my-private:latest", size_bytes: 1_000_000_000, digest: "a".repeat(64) },
  ];

  async function refreshAll(ctx: Load, nowMs?: number) {
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.mod.refreshOllamaModelUpdateStatuses(nowMs);
  }

  beforeEach(() => {
    document.body.innerHTML = '<div id="ollama-model-manager"></div>';
  });

  it("checks once per refresh, caches for 6 h and re-checks afterwards", async () => {
    const ctx = await load({
      models: oldAndNew,
      check: status({ "gemma4:e4b-it-q4_K_M": "outdated", "gemma4:e4b": "current", "my-private:latest": "local" }),
    });

    await refreshAll(ctx);
    expect(ctx.checkCalls).toHaveLength(1);
    expect(ctx.checkCalls[0].map((m) => m.name)).toEqual(["gemma4:e4b-it-q4_K_M", "gemma4:e4b", "my-private:latest"]);
    const map = ctx.mod.getOllamaModelUpdateMap();
    expect(map.get("gemma4:e4b-it-q4_k_m")?.status).toBe("outdated");
    expect(map.get("gemma4:e4b")?.status).toBe("current");
    expect(map.get("my-private:latest")?.status).toBe("local");

    await refreshAll(ctx);
    await refreshAll(ctx, Date.now() + 5 * HOUR);
    expect(ctx.checkCalls).toHaveLength(1);

    await refreshAll(ctx, Date.now() + 6 * HOUR + 1_000);
    expect(ctx.checkCalls).toHaveLength(2);
    expect(ctx.showToast).not.toHaveBeenCalled();
  });

  it("re-checks a model whose digest changed (re-pulled) without waiting for the TTL", async () => {
    const ctx = await load({ models: oldAndNew, check: status({ "gemma4:e4b-it-q4_K_M": "outdated" }) });
    await refreshAll(ctx);
    expect(ctx.checkCalls).toHaveLength(1);

    ctx.setModels(oldAndNew.map((m) => (m.name === "gemma4:e4b-it-q4_K_M" ? { ...m, digest: NEW_DIGEST } : m)));
    await refreshAll(ctx);
    expect(ctx.checkCalls).toHaveLength(2);
    expect(ctx.checkCalls[1].map((m) => m.name)).toEqual(["gemma4:e4b-it-q4_K_M"]);
  });

  it("stays silent when offline: no chip, no toast, no crash, short retry back-off", async () => {
    let fail = true;
    const ctx = await load({
      models: oldAndNew,
      check: (requested) => (fail ? new Error("network unreachable") : status({ "gemma4:e4b": "current" })(requested)),
    });

    await refreshAll(ctx);
    expect(ctx.checkCalls).toHaveLength(1);
    expect(ctx.mod.getOllamaModelUpdateMap().size).toBe(0);
    expect(ctx.showToast).not.toHaveBeenCalled();

    // Inside the 5 min back-off window: no new request.
    await refreshAll(ctx, Date.now() + 4 * 60_000);
    expect(ctx.checkCalls).toHaveLength(1);

    fail = false;
    await refreshAll(ctx, Date.now() + 6 * 60_000);
    expect(ctx.checkCalls).toHaveLength(2);
    expect(ctx.mod.getOllamaModelUpdateMap().get("gemma4:e4b")?.status).toBe("current");
    expect(ctx.showToast).not.toHaveBeenCalled();
  });

  it("ignores 'unknown' and malformed answers", async () => {
    const unknown = await load({ models: oldAndNew, check: status({}) });
    await refreshAll(unknown);
    expect(unknown.mod.getOllamaModelUpdateMap().size).toBe(0);

    const malformed = await load({ models: oldAndNew, check: () => ({ not: "an array" }) });
    await refreshAll(malformed);
    expect(malformed.mod.getOllamaModelUpdateMap().size).toBe(0);
    expect(malformed.showToast).not.toHaveBeenCalled();
  });

  it("does not send models without a comparable digest", async () => {
    const ctx = await load({
      models: [
        { name: "no-digest:latest", size_bytes: 1 },
        { name: "short:latest", size_bytes: 1, digest: "abc" },
      ],
      check: status({}),
    });
    await refreshAll(ctx);
    expect(ctx.checkCalls).toHaveLength(0);
  });

  it("renders meta line, chip and hint in 'Your Models' and pulls the ticked catalog ref verbatim", async () => {
    const ctx = await load({
      models: oldAndNew,
      check: status({ "gemma4:e4b-it-q4_K_M": "outdated", "gemma4:e4b": "current", "my-private:latest": "local" }),
    });
    ctx.state.setSettings({
      ai_fallback: { provider: "ollama", model: "gemma4:e4b-it-q4_K_M" },
      providers: { ollama: {} },
    } as never);

    await refreshAll(ctx);
    ctx.mod.renderOllamaModelManager();
    await new Promise((resolve) => setTimeout(resolve, 60));

    const container = document.getElementById("ollama-model-manager")!;
    const text = container.textContent ?? "";
    expect(text).toContain("9.6 GB");
    expect(text).not.toContain("8.9 GB");
    expect(text).toContain("Outdated");
    expect(text).toContain("newer build available (registry dc35e8d9c606, 6.6 GB)");
    expect(text).toContain("Current");
    expect(text).toContain("Local");

    // Digest and download date are in the expanded panel of the row, not in the compact line.
    expect(text).not.toContain("c6eb396dbd59");
    container
      .querySelector<HTMLElement>('.ollama-model-row[data-model="gemma4:e4b-it-q4_K_M"] .ollama-model-toggle')!
      .click();
    const opened = container.querySelector<HTMLElement>('.ollama-model-row[data-model="gemma4:e4b-it-q4_K_M"]')!
      .textContent!;
    expect(opened).toContain("c6eb396dbd59");
    expect(opened).toMatch(/Downloaded\s*23 Jul 2026/);

    const box = container.querySelector<HTMLInputElement>('.ollama-model-row[data-model="gemma4:e2b"] input[data-role="select"]');
    expect(box).not.toBeNull();
    box!.click();
    container.querySelector<HTMLButtonElement>(".ollama-model-bulk-btn")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(ctx.invoke).toHaveBeenCalledWith("pull_ollama_model", { model: "gemma4:e2b" });
  });
});
