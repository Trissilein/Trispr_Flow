import { describe, expect, it } from "vitest";
import rawCatalog from "../model-catalog.json";
import {
  MODEL_CATALOG,
  catalogSourceLabel,
  isHuggingFaceModelRef,
  parseModelCatalog,
} from "../ollama-model-catalog";
import { normalizeModelTag } from "../ollama-tag-utils";
// @ts-expect-error plain .mjs script without type declarations
import * as verify from "../../scripts/verify-model-catalog.mjs";

const valid = {
  name: "gemma4:e4b",
  label: "Gemma 4 E4B",
  source: "ollama",
  family: "gemma4",
  status: "recommended",
  profile: "Recommended",
  size_gb: 6.58,
  description: "x",
};

describe("model-catalog.json (shipped data)", () => {
  it("is an array and every entry survives validation", () => {
    expect(Array.isArray(rawCatalog)).toBe(true);
    expect(MODEL_CATALOG).toHaveLength((rawCatalog as unknown[]).length);
  });

  it("has unique pull refs (case-insensitive)", () => {
    const keys = MODEL_CATALOG.map((entry) => normalizeModelTag(entry.name));
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("only uses refs the backend accepts (charset, length) and the verifier can parse", () => {
    for (const entry of MODEL_CATALOG) {
      expect(entry.name).toMatch(/^[A-Za-z0-9:._/-]+$/);
      expect(entry.name.length).toBeLessThanOrEqual(200);
      expect(() => verify.parseModelRef(entry.name)).not.toThrow();
    }
  });

  it("separates Ollama library refs from hf.co refs by source", () => {
    const hf = MODEL_CATALOG.filter((entry) => entry.source === "huggingface");
    const ollama = MODEL_CATALOG.filter((entry) => entry.source === "ollama");
    expect(ollama.length).toBeGreaterThanOrEqual(8);
    expect(hf.length).toBeGreaterThanOrEqual(6);
    for (const entry of hf) expect(entry.name).toMatch(/^hf\.co\/[^/:]+\/[^/:]+:[^/:]+$/);
    for (const entry of ollama) expect(entry.name.startsWith("hf.co/")).toBe(false);
  });

  it("contains the verified Hugging Face refs and not the unresolvable ggml-org one", () => {
    const names = MODEL_CATALOG.map((entry) => entry.name);
    for (const ref of [
      "hf.co/google/gemma-4-E4B-it-qat-q4_0-gguf:latest",
      "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
      "hf.co/lmstudio-community/gemma-4-E4B-it-GGUF:Q4_K_M",
      "hf.co/unsloth/gemma-4-E2B-it-GGUF:Q4_K_M",
      "hf.co/unsloth/Qwen3.5-4B-GGUF:Q4_K_M",
      "hf.co/unsloth/Qwen3.5-9B-GGUF:Q4_K_M",
    ]) {
      expect(names).toContain(ref);
    }
    expect(names.some((name) => name.toLowerCase().includes("ggml-org"))).toBe(false);
  });

  it("keeps the Bonsai-2 27B fork model (needs a patched llama.cpp, not Ollama) out of the list", () => {
    // The small bonsai-2 tags (2b/4b/9b) are regular Ollama library models; only the 27B needs the fork.
    expect(MODEL_CATALOG.some((entry) => /bonsai[-_ ]?2[-_ ]?27b/i.test(entry.name + entry.label))).toBe(false);
  });

  it("keeps every hand-entered `functional` proof (a typo would silently remove the 'Verified working' chip)", () => {
    const withProof = (rawCatalog as Array<Record<string, unknown>>).filter((entry) => "functional" in entry);
    for (const raw of withProof) {
      const loaded = MODEL_CATALOG.find((entry) => normalizeModelTag(entry.name) === normalizeModelTag(String(raw.name)));
      expect(loaded?.functional, `functional of ${String(raw.name)}`).toBeDefined();
    }
  });

  it("carries verification dates in YYYY-MM-DD and decimal sizes from the registry check", () => {
    for (const entry of MODEL_CATALOG) {
      expect(entry.verified_at).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      expect(entry.size_gb).toBeGreaterThan(0);
      expect(entry.size_gb).toBeLessThan(40);
    }
  });

  it("only asserts a release date or license together with the Hugging Face repo that proves it", () => {
    for (const entry of rawCatalog as Array<Record<string, unknown>>) {
      if ("released" in entry || "license" in entry) {
        expect(entry.released_from, String(entry.name)).toMatch(/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/);
      }
      if ("released" in entry) expect(entry.released).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    }
    // Everything the shipped data claims survives the loader.
    for (const entry of rawCatalog as Array<Record<string, unknown>>) {
      const loaded = MODEL_CATALOG.find((item) => item.name === entry.name)!;
      expect(loaded.released).toBe(entry.released);
      expect(loaded.released_from).toBe(entry.released_from);
      expect(loaded.license).toBe(entry.license);
    }
  });

  it("drops a release date or license that has no (valid) source", () => {
    const [noSource] = parseModelCatalog([{ ...valid, released: "2026-03-02", license: "apache-2.0" }]);
    expect(noSource.released).toBeUndefined();
    expect(noSource.license).toBeUndefined();
    const [badSource] = parseModelCatalog([
      { ...valid, released: "2026-03-02", released_from: "https://evil.example/x", license: "mit" },
    ]);
    expect(badSource.released).toBeUndefined();
    expect(badSource.released_from).toBeUndefined();
    const [badDate] = parseModelCatalog([{ ...valid, released: "March 2nd", released_from: "google/gemma-4-E4B-it" }]);
    expect(badDate.released).toBeUndefined();
    expect(badDate.released_from).toBe("google/gemma-4-E4B-it");
    const [good] = parseModelCatalog([
      { ...valid, released: "2026-03-02", released_from: "google/gemma-4-E4B-it", license: "apache-2.0" },
    ]);
    expect(good).toMatchObject({ released: "2026-03-02", released_from: "google/gemma-4-E4B-it", license: "apache-2.0" });
  });

  it("never calls the dense E2B/E4B models MoE (only 'not MoE' is allowed)", () => {
    for (const entry of MODEL_CATALOG.filter((item) => /e[24]b/i.test(item.name))) {
      expect(entry.description).not.toMatch(/(?<!not )\bMoE\b|mixture/i);
    }
  });

  it("labels sources for the UI", () => {
    expect(catalogSourceLabel("ollama")).toBe("Ollama library");
    expect(catalogSourceLabel("huggingface")).toBe("Hugging Face");
    expect(isHuggingFaceModelRef("HF.CO/unsloth/x:Q4")).toBe(true);
    expect(isHuggingFaceModelRef("gemma4:e4b")).toBe(false);
  });
});

describe("parseModelCatalog", () => {
  it("accepts a minimal valid entry and drops unknown fields", () => {
    const [entry] = parseModelCatalog([{ ...valid, extra: 1 }]);
    expect(entry).toMatchObject({ name: "gemma4:e4b", source: "ollama", status: "recommended", size_gb: 6.58 });
    expect(entry).not.toHaveProperty("extra");
    expect(entry.verified_at).toBeUndefined();
  });

  it("keeps optional vram_gb, verified_at and note when valid", () => {
    const [entry] = parseModelCatalog([{ ...valid, vram_gb: 5, verified_at: "2026-11-02", note: " hint " }]);
    expect(entry.vram_gb).toBe(5);
    expect(entry.verified_at).toBe("2026-11-02");
    expect(entry.note).toBe("hint");
  });

  it("drops malformed entries instead of crashing", () => {
    const entries = parseModelCatalog([
      valid,
      null,
      "string",
      { ...valid, name: "" },
      { ...valid, name: "a:b", size_gb: 0 },
      { ...valid, name: "c:d", size_gb: "6" },
      { ...valid, name: "e:f", source: "github" },
      { ...valid, name: "g:h", status: "great" },
      { ...valid, name: "i:j", description: "" },
    ]);
    expect(entries.map((entry) => entry.name)).toEqual(["gemma4:e4b"]);
  });

  it("rejects a source that disagrees with the ref (wrong pull path)", () => {
    const entries = parseModelCatalog([
      { ...valid, name: "hf.co/a/b:Q4_K_M", source: "ollama" },
      { ...valid, name: "gemma4:e2b", source: "huggingface" },
    ]);
    expect(entries).toEqual([]);
  });

  it("ignores a malformed verified_at and keeps the first of duplicate refs", () => {
    const entries = parseModelCatalog([
      { ...valid, verified_at: "yesterday", label: "first" },
      { ...valid, name: "GEMMA4:E4B", label: "duplicate" },
    ]);
    expect(entries).toHaveLength(1);
    expect(entries[0].label).toBe("first");
    expect(entries[0].verified_at).toBeUndefined();
  });

  it("returns an empty list for non-array input", () => {
    expect(parseModelCatalog({})).toEqual([]);
    expect(parseModelCatalog(null)).toEqual([]);
  });
});

describe("scripts/verify-model-catalog.mjs helpers", () => {
  it("builds manifest URLs for Ollama library, user and Hugging Face refs", () => {
    const url = (name: string) => verify.manifestUrl(verify.parseModelRef(name));
    expect(url("gemma4:e4b")).toBe("https://registry.ollama.ai/v2/library/gemma4/manifests/e4b");
    expect(url("qwen3.5")).toBe("https://registry.ollama.ai/v2/library/qwen3.5/manifests/latest");
    expect(url("someone/custom:q4")).toBe("https://registry.ollama.ai/v2/someone/custom/manifests/q4");
    expect(url("registry.ollama.ai/library/qwen3.5:4b")).toBe("https://registry.ollama.ai/v2/library/qwen3.5/manifests/4b");
    expect(url("hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M")).toBe(
      "https://huggingface.co/v2/unsloth/gemma-4-E4B-it-GGUF/manifests/Q4_K_M"
    );
    expect(url("hf.co/google/gemma-4-E4B-it-qat-q4_0-gguf")).toBe(
      "https://huggingface.co/v2/google/gemma-4-E4B-it-qat-q4_0-gguf/manifests/latest"
    );
  });

  it("rejects refs it cannot resolve safely", () => {
    for (const bad of ["", "../etc/passwd", "hf.co/onlyone:Q4", "evil host/x:y", "a/b/c/d:e", "model:ta g"]) {
      expect(() => verify.parseModelRef(bad)).toThrow();
    }
  });

  it("sums all layers in decimal GB", () => {
    const manifest = {
      layers: [{ size: 5_490_000_000 }, { size: 990_000_000 }, { size: 100_000_000 }, { size: 3_658_527 }],
    };
    expect(verify.sumLayerBytes(manifest)).toBe(6_583_658_527);
    expect(verify.bytesToGb(6_583_658_527)).toBe(6.58);
    expect(verify.sumLayerBytes({})).toBe(0);
    expect(verify.sumLayerBytes(null)).toBe(0);
  });

  it("flags size drift beyond the rounding tolerance only", () => {
    expect(verify.hasDrift(6.58, 6.58)).toBe(false);
    expect(verify.hasDrift(6.6, 6.58)).toBe(false);
    expect(verify.hasDrift(9.6, 6.58)).toBe(true);
    expect(verify.hasDrift(2.5, 3.32)).toBe(true);
  });
});

describe("scripts/verify-model-catalog.mjs release evidence", () => {
  it("builds the Hugging Face evidence URL asking only for creation date and tags", () => {
    expect(verify.evidenceUrl("google/gemma-4-E4B-it")).toBe(
      "https://huggingface.co/api/models/google/gemma-4-E4B-it?expand%5B%5D=createdAt&expand%5B%5D=tags"
    );
  });

  it("reads release day and license tag from the answer", () => {
    expect(
      verify.readEvidence({ createdAt: "2026-03-02T19:57:40.000Z", tags: ["gguf", "license:apache-2.0", "region:us"] })
    ).toEqual({ released: "2026-03-02", license: "apache-2.0" });
    expect(verify.readEvidence({ createdAt: "soon", tags: "x" })).toEqual({ released: null, license: null });
    expect(verify.readEvidence(null)).toEqual({ released: null, license: null });
  });

  it("reports claims the source does not confirm", () => {
    const evidence = { released: "2026-03-02", license: "apache-2.0" };
    expect(verify.evidenceDrift({ released: "2026-03-02", license: "apache-2.0" }, evidence)).toEqual([]);
    expect(verify.evidenceDrift({}, evidence)).toEqual([]);
    expect(verify.evidenceDrift({ released: "2026-03-03" }, evidence)).toEqual(["released 2026-03-03 != 2026-03-02"]);
    expect(verify.evidenceDrift({ license: "mit" }, evidence)).toEqual(["license mit != apache-2.0"]);
    expect(verify.evidenceDrift({ released: "2026-03-02" }, { released: null, license: null })).toEqual([
      "released 2026-03-02 != ?",
    ]);
  });
});

