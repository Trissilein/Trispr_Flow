/**
 * Tests for the Edit-Delta vocabulary learning system.
 *
 * `./settings.persistSettings` is mocked (calls Tauri invoke, not available in jsdom).
 * `./event-listeners` is mocked so promotion rendering doesn't throw.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../settings", () => ({
  persistSettings: vi.fn(async () => {}),
}));

vi.mock("../event-listeners", () => ({
  renderLearnedVocabChips: vi.fn(),
}));

import {
  wordDiff,
  ingestEditDelta,
  runMigrationIfNeeded,
  dismissLearnedTerm,
  dismissPendingSubstitution,
  getAutoLearnSnapshot,
} from "../vocab-auto-learn";
import { setSettings, settings } from "../state";
import type { Settings } from "../types";

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    vocab_terms: [],
    edit_substitutions: [],
    edit_delta_migrated: true, // skip migration in most tests
    postproc_custom_vocab: {},
    postproc_custom_vocab_enabled: false,
    ...overrides,
  } as unknown as Settings;
}

beforeEach(() => {
  setSettings(makeSettings());
});

afterEach(() => {
  setSettings(null);
});

// ---------------------------------------------------------------------------
// wordDiff
// ---------------------------------------------------------------------------

describe("wordDiff", () => {
  it("identical strings produce no pairs", () => {
    expect(wordDiff("hello world", "hello world")).toEqual([]);
  });

  it("single token swap yields one pair", () => {
    expect(wordDiff("Ich habe Trispa geladen.", "Ich habe Trispr geladen.")).toEqual([
      { from: "Trispa", to: "Trispr" },
    ]);
  });

  it("empty inputs produce no pairs", () => {
    expect(wordDiff("", "hello")).toEqual([]);
    expect(wordDiff("hello", "")).toEqual([]);
  });

  it("pure insertion (no delete to pair with) produces no substitution pair", () => {
    const pairs = wordDiff("Ich Trispr.", "Ich Trispr Flow.");
    expect(pairs).toEqual([]);
  });

  it("pure deletion produces no substitution pair", () => {
    const pairs = wordDiff("Ich Trispr Flow.", "Ich Trispr.");
    expect(pairs).toEqual([]);
  });

  it("tokens shorter than 3 chars are filtered out", () => {
    // "ab" and "cd" are both < 3 chars → no pair
    const pairs = wordDiff("Ich ab weiß.", "Ich cd weiß.");
    expect(pairs).toEqual([]);
  });

  it("numeric-only tokens are filtered out", () => {
    const pairs = wordDiff("Version 123 ist.", "Version 456 ist.");
    expect(pairs).toEqual([]);
  });

  it(">50% tokens changed → empty (complete-rewrite heuristic)", () => {
    // 3 tokens deleted + 4 inserted out of 3+4=7 total → 100% > 50%
    const pairs = wordDiff("alpha beta gamma", "delta epsilon zeta theta");
    expect(pairs).toEqual([]);
  });

  it("strips edge punctuation from pair values", () => {
    const pairs = wordDiff("Das Trispa, war toll.", "Das Trispr, war toll.");
    expect(pairs).toEqual([{ from: "Trispa", to: "Trispr" }]);
  });
});

// ---------------------------------------------------------------------------
// runMigrationIfNeeded
// ---------------------------------------------------------------------------

describe("runMigrationIfNeeded", () => {
  it("clears postproc_custom_vocab on first run", () => {
    setSettings(makeSettings({
      edit_delta_migrated: false,
      postproc_custom_vocab: { Trispa: "Trispr", bad: "entry" },
    }));
    runMigrationIfNeeded();
    expect(settings!.postproc_custom_vocab).toEqual({});
  });

  it("resets edit_substitutions to empty array on first run", () => {
    setSettings(makeSettings({ edit_delta_migrated: false }));
    runMigrationIfNeeded();
    expect(settings!.edit_substitutions).toEqual([]);
  });

  it("sets edit_delta_migrated=true after running", () => {
    setSettings(makeSettings({ edit_delta_migrated: false }));
    runMigrationIfNeeded();
    expect(settings!.edit_delta_migrated).toBe(true);
  });

  it("does NOT run again if already migrated", () => {
    setSettings(makeSettings({
      edit_delta_migrated: true,
      postproc_custom_vocab: { keep: "me" },
    }));
    runMigrationIfNeeded();
    expect(settings!.postproc_custom_vocab).toEqual({ keep: "me" });
  });

  it("does NOT touch vocab_terms during migration", () => {
    setSettings(makeSettings({
      edit_delta_migrated: false,
      vocab_terms: ["GPT", "MemPalace"],
    }));
    runMigrationIfNeeded();
    expect(settings!.vocab_terms).toEqual(["GPT", "MemPalace"]);
  });
});

// ---------------------------------------------------------------------------
// ingestEditDelta — accumulation and promotion
// ---------------------------------------------------------------------------

describe("ingestEditDelta — accumulation", () => {
  it("first sighting creates an EditSubstitution with count=1", () => {
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs).toHaveLength(1);
    expect(subs[0]).toMatchObject({ from: "Trispa", to: "Trispr", count: 1 });
  });

  it("second sighting increments count to 2, not yet promoted", () => {
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    ingestEditDelta("Der Trispa ist fertig.", "Der Trispr ist fertig.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs).toHaveLength(1);
    expect(subs[0].count).toBe(2);
    expect(settings!.vocab_terms).not.toContain("Trispr");
  });

  it("third sighting promotes: removes from pending, adds to postproc_custom_vocab", () => {
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    ingestEditDelta("Das Trispa läuft.", "Das Trispr läuft.");
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispr");
  });

  it("promotion adds 'to' to vocab_terms", () => {
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Test Trispa Ende.", "Test Trispr Ende.");
    }
    expect(settings!.vocab_terms).toContain("Trispr");
  });

  it("promotion with mixed-case 'from' adds both original and lowercase postproc entries", () => {
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Der Trispa war.", "Der Trispr war.");
    }
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispr");
    expect(settings!.postproc_custom_vocab["trispa"]).toBe("Trispr");
  });

  it("all-lowercase 'from' does NOT create duplicate lowercase entry", () => {
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("the trispa was.", "the trispr was.");
    }
    const keys = Object.keys(settings!.postproc_custom_vocab).filter((k) =>
      k.includes("trispa"),
    );
    expect(keys).toHaveLength(1);
  });

  it("different (from,to) pairs accumulate independently", () => {
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    ingestEditDelta("Der Code war.", "Der Codex war.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs).toHaveLength(2);
    expect(subs.find((s) => s.from === "Trispa")?.count).toBe(1);
    expect(subs.find((s) => s.from === "Code")?.count).toBe(1);
  });

  it("no-op when pasted equals submitted", () => {
    ingestEditDelta("Identisch.", "Identisch.");
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// dismissLearnedTerm / dismissPendingSubstitution
// ---------------------------------------------------------------------------

describe("dismissLearnedTerm", () => {
  it("removes the term from vocab_terms", async () => {
    setSettings(makeSettings({ vocab_terms: ["GPT", "Trispr"] }));
    await dismissLearnedTerm("GPT");
    expect(settings!.vocab_terms).toEqual(["Trispr"]);
  });

  it("no-op if term not present", async () => {
    setSettings(makeSettings({ vocab_terms: ["GPT"] }));
    await dismissLearnedTerm("Missing");
    expect(settings!.vocab_terms).toEqual(["GPT"]);
  });
});

describe("dismissPendingSubstitution", () => {
  it("removes the matching substitution", async () => {
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 2, first_seen_ms: 0, last_seen_ms: 0 },
      ],
    }));
    await dismissPendingSubstitution("Trispa", "Trispr");
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
  });

  it("no-op if pair not found", async () => {
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 1, first_seen_ms: 0, last_seen_ms: 0 },
      ],
    }));
    await dismissPendingSubstitution("Nonexistent", "Pair");
    expect(settings!.edit_substitutions ?? []).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// getAutoLearnSnapshot
// ---------------------------------------------------------------------------

describe("getAutoLearnSnapshot", () => {
  it("returns learned terms and pending count", () => {
    setSettings(makeSettings({
      vocab_terms: ["GPT", "Trispr"],
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 1, first_seen_ms: 0, last_seen_ms: 0 },
      ],
    }));
    const snap = getAutoLearnSnapshot();
    expect(snap.learned).toEqual(["GPT", "Trispr"]);
    expect(snap.pendingCount).toBe(1);
  });

  it("handles missing settings gracefully", () => {
    setSettings(null);
    const snap = getAutoLearnSnapshot();
    expect(snap.learned).toEqual([]);
    expect(snap.pendingCount).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// 44c: Regression tests
// ---------------------------------------------------------------------------

describe("44c — casing-only correction is tracked (regression: v0.7.3 casing bug)", () => {
  it("capitalisation fix in sentence context accumulates correctly", () => {
    // 'trispr flow' → 'Trispr Flow': stripEdgePunct gives 'trispr'/'Trispr', 'flow'/'Flow'
    // Both differ → two substitution pairs
    ingestEditDelta("Ich nutze trispr flow täglich.", "Ich nutze Trispr Flow täglich.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs.length).toBeGreaterThanOrEqual(1);
    const trispr = subs.find((s) => s.from.toLowerCase() === "trispr");
    expect(trispr).toBeDefined();
  });

  it("single-word capitalisation fix is NOT ignored when surrounded by other words", () => {
    ingestEditDelta("das trispr ist gut.", "das Trispr ist gut.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs.find((s) => s.from === "trispr" && s.to === "Trispr")).toBeDefined();
  });
});

describe("44c — promotion activates postproc_custom_vocab_enabled", () => {
  it("postproc_custom_vocab_enabled becomes true after first promotion", () => {
    setSettings(makeSettings({ postproc_custom_vocab_enabled: false }));
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    }
    expect(settings!.postproc_custom_vocab_enabled).toBe(true);
  });

  it("already-true postproc_custom_vocab_enabled stays true after promotion", () => {
    setSettings(makeSettings({ postproc_custom_vocab_enabled: true }));
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    }
    expect(settings!.postproc_custom_vocab_enabled).toBe(true);
  });
});

describe("44c — vocab_terms deduplication on promotion", () => {
  it("does not add duplicate term if already in vocab_terms", () => {
    setSettings(makeSettings({ vocab_terms: ["Trispr"] }));
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Das Trispa läuft.", "Das Trispr läuft.");
    }
    const count = (settings!.vocab_terms ?? []).filter((t) => t === "Trispr").length;
    expect(count).toBe(1);
  });

  it("case-insensitive deduplication: 'trispr' already present prevents adding 'Trispr'", () => {
    setSettings(makeSettings({ vocab_terms: ["trispr"] }));
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Das Trispa läuft.", "Das Trispr läuft.");
    }
    const dupes = (settings!.vocab_terms ?? []).filter(
      (t) => t.toLowerCase() === "trispr",
    );
    expect(dupes).toHaveLength(1);
  });
});

describe("44c — postproc_custom_vocab merge on repeated promotion", () => {
  it("second promotion adds to existing vocab without overwriting prior entries", () => {
    setSettings(makeSettings({
      postproc_custom_vocab: { Trispa: "Trispr" },
      postproc_custom_vocab_enabled: true,
    }));
    for (let i = 0; i < 3; i++) {
      ingestEditDelta("Der Code war.", "Der Codex war.");
    }
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispr");
    expect(settings!.postproc_custom_vocab["Code"]).toBe("Codex");
  });
});

describe("44c — ingestEditDelta is a no-op when settings is null", () => {
  it("does not throw when settings is null", () => {
    setSettings(null);
    expect(() => ingestEditDelta("Trispa läuft.", "Trispr läuft.")).not.toThrow();
  });
});

describe("44c — wordDiff casing edge cases", () => {
  it("casing-only token swap produces a substitution pair", () => {
    // Context: surrounded by equal tokens so >50% bail does not trigger
    const pairs = wordDiff("der trispr ist fertig.", "der Trispr ist fertig.");
    expect(pairs).toEqual([{ from: "trispr", to: "Trispr" }]);
  });

  it("multi-word casing fix produces pairs for each changed token", () => {
    const pairs = wordDiff("mit trispr flow arbeiten.", "mit Trispr Flow arbeiten.");
    expect(pairs.length).toBeGreaterThanOrEqual(1);
    expect(pairs.find((p) => p.from === "trispr" && p.to === "Trispr")).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 44c — VS-Code / Monaco UIAutomation compatibility (window-shrinking)
// ---------------------------------------------------------------------------

describe("44c — Caret-range hierarchy: wordDiff with line-scoped submitted (selection-line pattern)", () => {
  it("treats short selection-line output exactly like a value-pattern read", () => {
    // selection-line returns the single line containing the caret; size is similar to pasted
    // → no window-shrinking activates, normal LCS diff runs
    const pasted = "Ich habe Trispa heute getestet.";
    const submitted = "Ich habe Trispr heute getestet.";
    const pairs = wordDiff(pasted, submitted);
    expect(pairs).toEqual([{ from: "Trispa", to: "Trispr" }]);
  });

  it("trailing newline / whitespace from ExpandToEnclosingUnit(Line) does not break diffing", () => {
    // selection-line often returns the line with a trailing newline
    const pasted = "Trispa läuft sauber.";
    const submitted = "Trispr läuft sauber.\n";
    const pairs = wordDiff(pasted, submitted);
    expect(pairs.find((p) => p.from === "Trispa" && p.to === "Trispr")).toBeDefined();
  });
});

describe("44c — wordDiff with oversized submitted (Monaco / VS-Code)", () => {
  it("detects edit when submitted is the entire document and pasted appears once with one edit", () => {
    const pasted = "Der Trispa ist fertig.";
    // Simulate Monaco TextPattern dump: lots of unrelated text around the edited paste
    const surrounding = Array.from({ length: 20 }, (_, i) => `boilerplate token${i}`).join(
      " ",
    );
    const submitted = `${surrounding} Der Trispr ist fertig. ${surrounding}`;
    const pairs = wordDiff(pasted, submitted);
    expect(pairs.find((p) => p.from === "Trispa" && p.to === "Trispr")).toBeDefined();
  });

  it("returns no pairs when pasted appears unchanged in oversized submitted", () => {
    const pasted = "Ich habe Trispr getestet.";
    const surrounding = Array.from({ length: 30 }, (_, i) => `noise${i}`).join(" ");
    const submitted = `${surrounding} Ich habe Trispr getestet. ${surrounding}`;
    expect(wordDiff(pasted, submitted)).toEqual([]);
  });

  it("ingestEditDelta accumulates correctly with VS-Code-style oversized submitted", () => {
    const pasted = "Heute war Trispa wieder schnell.";
    const surrounding = Array.from({ length: 25 }, (_, i) => `code_line_${i}`).join(" ");
    const submitted = `${surrounding} Heute war Trispr wieder schnell. ${surrounding}`;
    for (let i = 0; i < 3; i++) {
      ingestEditDelta(pasted, submitted);
    }
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispr");
    expect(settings!.vocab_terms).toContain("Trispr");
  });
});
