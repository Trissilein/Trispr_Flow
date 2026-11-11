/**
 * Vocabulary learning rework: phrase pairs, similarity gate, one-edit-one-count,
 * transparency (toast + Undo), removal of term AND rules, TTL and caps.
 * Baseline behaviour (diff, accumulation, promotion) lives in vocab-auto-learn.test.ts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  renderLearnedVocabChips: vi.fn(),
  showToast: vi.fn(),
}));

vi.mock("../toast", () => ({ showToast: mocks.showToast }));
vi.mock("../settings/vocabulary.settings", () => ({
  renderLearnedVocabChips: mocks.renderLearnedVocabChips,
}));
vi.mock("../settings-persist", () => ({
  persistSettings: vi.fn(async () => { }),
}));

import {
  wordDiff,
  ingestEditDelta,
  classifyCorrection,
  dismissLearnedTerm,
  dismissLearnedPair,
  dismissPendingSubstitution,
  pruneStalePending,
  PROMOTION_THRESHOLD,
  PENDING_TTL_MS,
} from "../vocab-auto-learn";
import { setSettings, settings } from "../state";
import type { EditSubstitution, Settings } from "../types";

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    vocab_terms: [],
    edit_substitutions: [],
    edit_delta_migrated: true,
    postproc_custom_vocab: {},
    postproc_custom_vocab_enabled: false,
    ...overrides,
  } as unknown as Settings;
}

const DAY = 24 * 60 * 60 * 1000;

beforeEach(() => {
  setSettings(makeSettings());
  mocks.renderLearnedVocabChips.mockReset();
  mocks.showToast.mockReset();
});

afterEach(() => {
  setSettings(null);
});

// ---------------------------------------------------------------------------
// Phrase pairs (2:1 / 1:2)
// ---------------------------------------------------------------------------

describe("wordDiff — phrase pairs", () => {
  it("2 tokens -> 1 token yields one phrase pair", () => {
    expect(wordDiff("Wir nutzen Mem Palace jeden Tag.", "Wir nutzen MemPalace jeden Tag.")).toEqual([
      { from: "Mem Palace", to: "MemPalace" },
    ]);
  });

  it("1 token -> 2 tokens yields one phrase pair", () => {
    expect(wordDiff("Wir nutzen MemPalace jeden Tag.", "Wir nutzen Mem Palace jeden Tag.")).toEqual([
      { from: "MemPalace", to: "Mem Palace" },
    ]);
  });

  it("3:1 runs stay unsupported (no pair)", () => {
    expect(wordDiff("Wir nutzen Mem Pal Ace jeden Tag.", "Wir nutzen MemPalace jeden Tag.")).toEqual([]);
  });

  it("a learned phrase becomes a find-replace rule and a term", () => {
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) {
      ingestEditDelta("Wir nutzen Mem Palace jeden Tag.", "Wir nutzen MemPalace jeden Tag.");
    }
    expect(settings!.postproc_custom_vocab["Mem Palace"]).toBe("MemPalace");
    expect(settings!.vocab_terms).toContain("MemPalace");
  });
});

describe("wordDiff — compounds and broken characters", () => {
  it("a hyphenated compound teaches the changed part only", () => {
    expect(
      wordDiff("Schau dir die WhatsApp-Tickets an.", "Schau dir die WhatsUp-Tickets an."),
    ).toEqual([{ from: "WhatsApp", to: "WhatsUp" }]);
  });

  it("compound and bare word share one counter and promote together", () => {
    ingestEditDelta("Schau dir die WhatsApp-Tickets an.", "Schau dir die WhatsUp-Tickets an.");
    ingestEditDelta("Öffne WhatsApp bitte.", "Öffne WhatsUp bitte.");
    expect(settings!.postproc_custom_vocab["WhatsApp"]).toBe("WhatsUp");
    expect(settings!.vocab_terms).toContain("WhatsUp");
  });

  it("compounds with different part counts stay one pair", () => {
    expect(wordDiff("Das ist Kick-off Plan", "Das ist Kickoff Plan")).toEqual([
      { from: "Kick-off", to: "Kickoff" },
    ]);
  });

  it("a one-word dictation corrected as a whole is still a pair", () => {
    expect(wordDiff("WhatsApp", "WhatsUp")).toEqual([{ from: "WhatsApp", to: "WhatsUp" }]);
  });

  it("an unrelated one-word replacement is rejected by the similarity gate", () => {
    ingestEditDelta("Morgen", "Dienstag");
    ingestEditDelta("Morgen", "Dienstag");
    expect(settings!.postproc_custom_vocab["Morgen"]).toBeUndefined();
  });

  it("a typographic apostrophe swap is never a correction", () => {
    expect(classifyCorrection("Don't", "Don’t")).toBe("reject");
    expect(classifyCorrection("That's", "That’s")).toBe("reject");
    ingestEditDelta("Don't stop now please", "Don’t stop now please");
    ingestEditDelta("Don't stop now please", "Don’t stop now please");
    expect(settings!.postproc_custom_vocab["Don't"]).toBeUndefined();
    expect(settings!.edit_substitutions).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Similarity gate
// ---------------------------------------------------------------------------

describe("classifyCorrection", () => {
  it.each([
    ["Trispa", "Trispr"],
    ["Kodex", "Codex"],
    ["Gamma", "Gemma"],
    ["Wispr", "Whisper"],
    ["Mem Palace", "MemPalace"],
  ])("%s -> %s is a safe find-replace rule", (from, to) => {
    expect(classifyCorrection(from, to)).toBe("rule");
  });

  it.each([
    ["morgen", "heute"],
    ["treffen", "sehen"],
    ["gestern", "heute"],
    ["Jammer", "Gemma"],
  ])("%s -> %s is a rewording and rejected", (from, to) => {
    expect(classifyCorrection(from, to)).toBe("reject");
  });

  it.each([
    ["den", "dem"],
    ["der", "die"],
    ["und", "oder"],
    ["with", "wit"],
  ])("function-word edit %s -> %s is rejected", (from, to) => {
    expect(classifyCorrection(from, to)).toBe("reject");
  });

  it("loose matches only become hints, never find-replace rules", () => {
    expect(classifyCorrection("Kloud", "Claude")).toBe("hint");
    expect(classifyCorrection("Muller", "Müller")).toBe("hint"); // accents only: might be another word
  });

  it("casing-only edits are hint-level (a rule would recase every common word)", () => {
    expect(classifyCorrection("trispr", "Trispr")).toBe("casing");
  });

  it("one edit in a 3-4 letter word is not enough for a rule (Zug -> Zeug)", () => {
    expect(classifyCorrection("Zug", "Zeug")).toBe("hint");
  });
});

describe("ingestEditDelta — similarity gate", () => {
  it("rewordings are not even counted", () => {
    ingestEditDelta("Ich komme morgen vorbei.", "Ich komme heute vorbei.");
    ingestEditDelta("Wir können treffen jetzt.", "Wir können sehen jetzt.");
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
  });

  it("a repeated rewording never becomes a rule or a term", () => {
    for (let i = 0; i < 4; i++) {
      ingestEditDelta("Ich komme morgen vorbei.", "Ich komme heute vorbei.");
    }
    expect(settings!.postproc_custom_vocab).toEqual({});
    expect(settings!.vocab_terms).toEqual([]);
  });

  it("grammar edits between function words are not counted", () => {
    ingestEditDelta("Ich sehe den Hund heute.", "Ich sehe dem Hund heute.");
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
  });

  it("a loose (hint-level) correction is learned as term + pair but gets no find-replace rule", () => {
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) {
      ingestEditDelta("Ich nutze Kloud heute.", "Ich nutze Claude heute.");
    }
    const subs = settings!.edit_substitutions ?? [];
    expect(subs[0]).toMatchObject({ from: "Kloud", to: "Claude", promoted: true });
    expect(settings!.vocab_terms).toContain("Claude");
    expect(settings!.postproc_custom_vocab).toEqual({});
  });

  it("a casing-only correction is learned as term + pair but gets no find-replace rule", () => {
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) {
      ingestEditDelta("das trispr ist gut.", "das Trispr ist gut.");
    }
    expect(settings!.vocab_terms).toContain("Trispr");
    expect(settings!.postproc_custom_vocab).toEqual({});
  });
});

// ---------------------------------------------------------------------------
// Counting: separate edits only
// ---------------------------------------------------------------------------

describe("ingestEditDelta — one edit counts once", () => {
  it("the same correction twice inside ONE edit does not self-promote", () => {
    ingestEditDelta("Trispa läuft. Trispa startet.", "Trispr läuft. Trispr startet.");
    const subs = settings!.edit_substitutions ?? [];
    expect(subs).toHaveLength(1);
    expect(subs[0]).toMatchObject({ from: "Trispa", to: "Trispr", count: 1 });
    expect(subs[0].promoted).toBeFalsy();
    expect(settings!.postproc_custom_vocab).toEqual({});
    expect(settings!.vocab_terms).toEqual([]);
  });

  it("a second SEPARATE edit promotes", () => {
    ingestEditDelta("Trispa läuft. Trispa startet.", "Trispr läuft. Trispr startet.");
    ingestEditDelta("Trispa läuft.", "Trispr läuft.");
    expect(settings!.edit_substitutions?.[0]).toMatchObject({ count: 2, promoted: true });
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispr");
  });
});

// ---------------------------------------------------------------------------
// Transparency: toast with Undo, promoted pairs stay visible
// ---------------------------------------------------------------------------

describe("promotion transparency", () => {
  it("shows a 'learned' toast with from → to on promotion, not on the first sighting", () => {
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    expect(mocks.showToast).not.toHaveBeenCalled();
    ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    expect(mocks.showToast).toHaveBeenCalledTimes(1);
    const toast = mocks.showToast.mock.calls[0][0];
    expect(toast.message).toContain("Trispa → Trispr");
    expect(toast.actionLabel).toBe("Undo");
  });

  it("seeing a learned pair again does not toast again but keeps counting", () => {
    for (let i = 0; i < 3; i++) ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    expect(mocks.showToast).toHaveBeenCalledTimes(1);
    expect(settings!.edit_substitutions?.[0].count).toBe(3);
  });

  it("a learned pair seen again does not re-enable custom vocabulary the user switched off", () => {
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    expect(settings!.postproc_custom_vocab_enabled).toBe(true);
    settings!.postproc_custom_vocab_enabled = false;
    ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    expect(settings!.postproc_custom_vocab_enabled).toBe(false);
  });

  it("Undo removes the pair, its rule and its term", async () => {
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    await mocks.showToast.mock.calls[0][0].onAction();
    expect(settings!.edit_substitutions ?? []).toHaveLength(0);
    expect(settings!.postproc_custom_vocab).toEqual({});
    expect(settings!.vocab_terms).toEqual([]);
  });

  it("never overwrites a rule the user already has for the same word", () => {
    setSettings(makeSettings({ postproc_custom_vocab: { Trispa: "Trispa-Custom" } }));
    for (let i = 0; i < PROMOTION_THRESHOLD; i++) ingestEditDelta("Der Trispa ist.", "Der Trispr ist.");
    expect(settings!.postproc_custom_vocab["Trispa"]).toBe("Trispa-Custom");
  });
});

describe("× on learned items removes term AND rules", () => {
  it("dismissLearnedTerm removes the term, learned pairs and every rule pointing at it", async () => {
    setSettings(makeSettings({
      vocab_terms: ["Trispr", "Other"],
      postproc_custom_vocab: { Trispa: "Trispr", trispa: "Trispr", keep: "Other" },
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 2, first_seen_ms: 1, last_seen_ms: 2, promoted: true },
        { from: "pend", to: "Other", count: 1, first_seen_ms: 1, last_seen_ms: 2 },
      ],
    }));
    await dismissLearnedTerm("Trispr");
    expect(settings!.vocab_terms).toEqual(["Other"]);
    expect(settings!.postproc_custom_vocab).toEqual({ keep: "Other" });
    expect(settings!.edit_substitutions).toHaveLength(1);
    expect(settings!.edit_substitutions![0].from).toBe("pend");
  });

  it("dismissLearnedPair keeps the term while another learned pair still points at it", async () => {
    setSettings(makeSettings({
      vocab_terms: ["Codex"],
      postproc_custom_vocab: { Kodex: "Codex", Code: "Codex" },
      edit_substitutions: [
        { from: "Kodex", to: "Codex", count: 2, first_seen_ms: 1, last_seen_ms: 2, promoted: true },
        { from: "Code", to: "Codex", count: 2, first_seen_ms: 1, last_seen_ms: 3, promoted: true },
      ],
    }));
    await dismissLearnedPair("Kodex", "Codex");
    expect(settings!.vocab_terms).toEqual(["Codex"]);
    expect(settings!.postproc_custom_vocab).toEqual({ Code: "Codex" });
    await dismissLearnedPair("Code", "Codex");
    expect(settings!.vocab_terms).toEqual([]);
    expect(settings!.postproc_custom_vocab).toEqual({});
  });

  it("dismissPendingSubstitution leaves promoted pairs alone", async () => {
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 2, first_seen_ms: 1, last_seen_ms: 2, promoted: true },
      ],
    }));
    await dismissPendingSubstitution("Trispa", "Trispr");
    expect(settings!.edit_substitutions).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// TTL and caps
// ---------------------------------------------------------------------------

describe("pending TTL", () => {
  it("expires pending pairs older than the TTL on the next edit, keeps fresh and promoted ones", () => {
    const now = Date.now();
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Old", to: "Older", count: 1, first_seen_ms: 1, last_seen_ms: now - PENDING_TTL_MS - DAY },
        { from: "Fresh", to: "Fresher", count: 1, first_seen_ms: 1, last_seen_ms: now - PENDING_TTL_MS + DAY },
        { from: "Ancient", to: "Ancienter", count: 3, first_seen_ms: 1, last_seen_ms: 1, promoted: true },
      ],
    }));
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    const from = (settings!.edit_substitutions ?? []).map((s) => s.from).sort();
    expect(from).toEqual(["Ancient", "Fresh", "Trispa"]);
  });

  it("a pair seen again inside the window survives and counts on", () => {
    const now = Date.now();
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Trispa", to: "Trispr", count: 1, first_seen_ms: 1, last_seen_ms: now - PENDING_TTL_MS + DAY },
      ],
    }));
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    expect(settings!.edit_substitutions?.[0]).toMatchObject({ count: 2, promoted: true });
  });

  it("pruneStalePending drops expired pending pairs at startup", async () => {
    setSettings(makeSettings({
      edit_substitutions: [
        { from: "Old", to: "Older", count: 1, first_seen_ms: 1, last_seen_ms: 1 },
      ],
    }));
    await pruneStalePending();
    expect(settings!.edit_substitutions).toEqual([]);
  });
});

describe("caps", () => {
  it("keeps at most 50 pending pairs (newest win)", () => {
    const now = Date.now();
    const many: EditSubstitution[] = Array.from({ length: 60 }, (_, i) => ({
      from: `Pend${String(i).padStart(2, "0")}x`,
      to: `Pend${String(i).padStart(2, "0")}y`,
      count: 1,
      first_seen_ms: now - 1000 + i,
      last_seen_ms: now - 1000 + i,
    }));
    setSettings(makeSettings({ edit_substitutions: many }));
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    const pending = (settings!.edit_substitutions ?? []).filter((s) => !s.promoted);
    expect(pending).toHaveLength(50);
    expect(pending.some((s) => s.from === "Trispa")).toBe(true);
    expect(pending.some((s) => s.from === "Pend00x")).toBe(false);
  });

  it("keeps at most 100 learned pairs; the oldest lose their rule and term", () => {
    const learned: EditSubstitution[] = Array.from({ length: 101 }, (_, i) => ({
      from: `Zanth${i}a`,
      to: `Zanth${i}b`,
      count: 2,
      first_seen_ms: 1,
      last_seen_ms: 100 + i, // index 0 is the oldest
      promoted: true,
    }));
    setSettings(makeSettings({
      edit_substitutions: learned,
      vocab_terms: learned.map((l) => l.to),
      postproc_custom_vocab: Object.fromEntries(learned.map((l) => [l.from, l.to])),
    }));
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    expect((settings!.edit_substitutions ?? []).filter((s) => s.promoted)).toHaveLength(100);
    expect(settings!.vocab_terms).not.toContain("Zanth0b");
    expect(settings!.postproc_custom_vocab["Zanth0a"]).toBeUndefined();
    expect(settings!.postproc_custom_vocab["Zanth1a"]).toBe("Zanth1b");
  });

  it("keeps at most 200 terms (oldest dropped)", () => {
    const terms = Array.from({ length: 205 }, (_, i) => `Term${i}`);
    setSettings(makeSettings({ vocab_terms: terms }));
    ingestEditDelta("Ich Trispa.", "Ich Trispr.");
    expect(settings!.vocab_terms).toHaveLength(200);
    expect(settings!.vocab_terms).not.toContain("Term0");
    expect(settings!.vocab_terms).toContain("Term204");
  });
});
