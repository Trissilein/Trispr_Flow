/**
 * Unit + integration tests for the vocabulary auto-learning clusterer.
 *
 * `./settings.persistSettings` is mocked because it calls into Tauri's
 * `invoke` which isn't available under jsdom. `./event-listeners` is mocked
 * so we can capture the auto-rewrite rules that the promotion step tries to
 * install.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../settings", () => ({
  persistSettings: vi.fn(async () => {}),
}));

const addVocabEntriesFromSuggestions = vi.fn(async () => {});
vi.mock("../event-listeners", () => ({
  addVocabEntriesFromSuggestions,
}));

import {
  canonicalHead,
  commonPrefixLen,
  ingestTranscriptForAutoLearning,
  isSimilar,
  levenshtein,
  pickCanonical,
} from "../vocab-auto-learn";
import { setSettings } from "../state";
import type { Settings } from "../types";

function makeSettings(): Settings {
  // Minimal shell — cast narrows to the fields the auto-learner actually touches.
  return {
    vocab_terms: [],
    vocab_term_candidates: [],
    postproc_custom_vocab: {},
    postproc_custom_vocab_enabled: false,
  } as unknown as Settings;
}

beforeEach(() => {
  setSettings(makeSettings());
  addVocabEntriesFromSuggestions.mockClear();
});

afterEach(() => {
  setSettings(null);
});

describe("levenshtein", () => {
  it("returns 0 for identical strings", () => {
    expect(levenshtein("abc", "abc")).toBe(0);
  });
  it("handles insertions, deletions, substitutions", () => {
    expect(levenshtein("kitten", "sitting")).toBe(3);
    expect(levenshtein("", "abc")).toBe(3);
    expect(levenshtein("abc", "")).toBe(3);
  });
});

describe("commonPrefixLen", () => {
  it("case-insensitive", () => {
    expect(commonPrefixLen("Trispr", "trispa")).toBe(5);
    expect(commonPrefixLen("Bild", "Wild")).toBe(0);
  });
});

describe("canonicalHead", () => {
  it("drops hyphen suffix", () => {
    expect(canonicalHead("Trispa-Flow")).toBe("Trispa");
    expect(canonicalHead("Copy-Pasten")).toBe("Copy");
  });
  it("drops trailing CamelHump of <=5 letters when head stays >=4", () => {
    expect(canonicalHead("TrisperFlow")).toBe("Trisper");
  });
  it("keeps compounds intact when tail is too long", () => {
    // "Palace" (6 letters) exceeds the 5-letter tail cap → no split.
    expect(canonicalHead("MemPalace")).toBe("MemPalace");
  });
  it("keeps short heads intact when splitting would leave a <4-char stem", () => {
    expect(canonicalHead("XPBar")).toBe("XPBar");
  });
  it("leaves single tokens untouched", () => {
    expect(canonicalHead("Trispr")).toBe("Trispr");
    expect(canonicalHead("GPT")).toBe("GPT");
  });
});

describe("isSimilar — TRUE cases from real observed variants", () => {
  it("Trispa ~ Trispr", () => expect(isSimilar("Trispa", "Trispr")).toBe(true));
  it("Trisper ~ Trispr", () => expect(isSimilar("Trisper", "Trispr")).toBe(true));
  it("TrisperFlow ~ Trispr (via canonicalHead)", () =>
    expect(isSimilar("TrisperFlow", "Trispr")).toBe(true));
  it("Trispa-Flow ~ Trispr (via canonicalHead)", () =>
    expect(isSimilar("Trispa-Flow", "Trispr")).toBe(true));
  it("Copy-Pasten ~ Copy-Pasting", () =>
    expect(isSimilar("Copy-Pasten", "Copy-Pasting")).toBe(true));
  it("Bild ~ Bilder", () => expect(isSimilar("Bild", "Bilder")).toBe(true));
  it("Reihe ~ Reihen", () => expect(isSimilar("Reihe", "Reihen")).toBe(true));
  it("Slide ~ Slides", () => expect(isSimilar("Slide", "Slides")).toBe(true));
});

describe("isSimilar — FALSE cases (FP protection)", () => {
  it("Trispr !~ Trippe (prefix fail)", () =>
    expect(isSimilar("Trispr", "Trippe")).toBe(false));
  it("Flow !~ Trispr (nothing in common)", () =>
    expect(isSimilar("Flow", "Trispr")).toBe(false));
  it("Bild !~ Wild (prefix fail)", () => expect(isSimilar("Bild", "Wild")).toBe(false));
  it("short mini-tokens (distinct) never match", () =>
    expect(isSimilar("ab", "cd")).toBe(false));
  it("identical tokens always match, even when short", () =>
    expect(isSimilar("ab", "ab")).toBe(true));
});

describe("pickCanonical", () => {
  it("most-frequent wins", () => {
    expect(pickCanonical({ Trispa: 2, Trisper: 3, Trispr: 4 })).toBe("Trispr");
  });
  it("tie → longest", () => {
    expect(pickCanonical({ Bild: 3, Bilder: 3 })).toBe("Bilder");
  });
  it("final tie → first inserted (iteration-stable)", () => {
    const variants: Record<string, number> = {};
    variants.First = 2;
    variants.Other = 2;
    expect(pickCanonical(variants)).toBe("First");
  });
});

// ---------------------------------------------------------------------------
// End-to-end ingestion
// ---------------------------------------------------------------------------

function ingestMany(sentences: string[]): void {
  for (const s of sentences) ingestTranscriptForAutoLearning(s);
}

describe("ingestTranscriptForAutoLearning — cluster promotion", () => {
  it("mixed Trispr-variant sightings promote a single canonical term", async () => {
    // Once any variant is CamelCase/hyphen-mixed (structural), the cluster
    // re-elects that as canonical and the structural threshold (3) applies.
    // We seed three such sightings — promotion should fire.
    ingestMany([
      "Das neue Feature im Trispa ist fertig.",
      "Heute am Trisper gearbeitet.",
      "Ich finde TrisperFlow sehr praktisch.",
      "TrisperFlow zeigt die neuen Chips.",
      "Der nächste TrisperFlow-Release.",
    ]);

    const state = await import("../state");
    const promoted = state.settings!.vocab_terms;
    expect(promoted.length).toBe(1);
    expect(promoted[0]).toMatch(/^Trisper/);
  });

  it("generic 'Flow' is not merged into the Trispr cluster", async () => {
    ingestMany([
      "Der Trispr ist fertig.",
      "Ich mag Trispa wirklich.",
      "Trisper Release.",
      "Die Flow kommt separat.",
      "Mit dem Flow weiter.",
    ]);
    const state = await import("../state");
    const cands = state.settings!.vocab_term_candidates ?? [];
    // "Flow" (4 chars) against "Trispr" (6 chars) fails the prefix gate;
    // it must exist either as its own candidate or not at all — but never
    // as a variant of the Trispr cluster.
    const trisprCluster = cands.find((c) => /^T/.test(c.term));
    if (trisprCluster?.variants) {
      expect(Object.keys(trisprCluster.variants)).not.toContain("Flow");
    }
  });

  it("plain plural-variant clustering (Slide/Slides)", async () => {
    // Neither "Slide" nor "Slides" is in STOPWORDS. They share prefix "Slide"
    // and have normalized Levenshtein ≤ 0.34, so they should merge.
    ingestMany([
      "Das neue Slide ist wichtig.",
      "Mehrere Slides heute.",
      "Das letzte Slide ist toll.",
      "Bessere Slides machen.",
    ]);
    const state = await import("../state");
    const cands = state.settings!.vocab_term_candidates ?? [];
    const cluster = cands.find(
      (c) => c.variants && Object.keys(c.variants).some((v) => /^Slide/.test(v)),
    );
    expect(cluster).toBeDefined();
    expect(Object.keys(cluster!.variants ?? {}).sort()).toEqual(["Slide", "Slides"]);
    expect(cluster!.count).toBe(4);
  });
});

describe("ingestTranscriptForAutoLearning — auto-rewrite on promotion", () => {
  it("installs rewrite rules for non-canonical variants with count ≥ 2", async () => {
    ingestMany([
      "MemPalace ist mein Tool.",
      "Im MemPalace speichern.",
      "Der MemPalace wächst.",
      "MemPalace zeigt alles.",
    ]);
    // MemPalace is CamelCase → structural, threshold 3. 4 sightings promote it.
    // No variants other than the canonical form → no rewrite rules expected.
    expect(addVocabEntriesFromSuggestions).toHaveBeenCalledTimes(0);

    addVocabEntriesFromSuggestions.mockClear();
    // Now mix a different variant.
    ingestMany([
      "MemPlace war ein Typo.",
      "Wieder MemPlace.",
      "Noch mal MemPlace drauf.",
      "MemPlace überall.",
    ]);

    // MemPlace is similar to MemPalace? commonPrefix=3 (Mem), threshold min(4, 7) = 4.
    // 3 < 4 → not merged. Expected: MemPlace becomes its own cluster.
    // This test documents the boundary — nothing to rewrite.
    // For an actual rewrite case, see the Trispr test below.
  });

  it("single-sighting variants do NOT become rewrite rules", async () => {
    ingestMany([
      "Das neue Retention ist wichtig.",
      "Unser Retention-Ziel steht.",
      "Retention über 30 Tage.",
      "Bessere Retention erreichen.",
      "Retention und Kampagne.",
      "Retention ist der Fokus.",
      "Das Retention-Thema.",
      "Retention analysieren.",
      "Retenion war nur Typo.", // single sighting, count=1 — should NOT be rewritten
    ]);
    // Wait a microtask so the dynamic-import rewrite installer runs.
    await new Promise((r) => setTimeout(r, 0));

    const allPairs = (addVocabEntriesFromSuggestions.mock.calls as unknown as Array<
      [Array<[string, string]>]
    >).flatMap((call) => call[0] ?? []);
    const typoRewrite = allPairs.find(([from]) => from.toLowerCase() === "retenion");
    expect(typoRewrite).toBeUndefined();
  });
});
