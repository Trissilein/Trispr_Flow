/**
 * Block M — Workflow Agent Tests (M11)
 *
 * Covers:
 *  WA-S1: Candidate disambiguation (M9)
 *  WA-S2: Target language validation (M10)
 *  WA-S3: Candidate scoring edge cases
 */

import { describe, expect, it } from "vitest";
import {
  isAmbiguousSelection,
  isValidTargetLanguage,
  DISAMBIGUATION_SCORE_THRESHOLD,
  ALLOWED_TARGET_LANGUAGES,
} from "../workflow-agent-policy";
import type { TranscriptSessionCandidate } from "../types";

function makeCandidate(id: string, score: number): TranscriptSessionCandidate {
  return {
    session_id: id,
    start_ms: 0,
    end_ms: 0,
    entry_count: 1,
    source_mix: ["mic"],
    preview: "test preview",
    score,
    reasoning: "test",
  };
}

// ---------------------------------------------------------------------------
// WA-S1: Candidate disambiguation (M9)
// ---------------------------------------------------------------------------
describe("Block M WA-S1 — Candidate disambiguation", () => {
  it("flags ambiguous when top-2 scores differ by less than threshold", () => {
    const candidates = [makeCandidate("a", 0.85), makeCandidate("b", 0.80)];
    expect(isAmbiguousSelection(candidates)).toBe(true);
  });

  it("not ambiguous when top-2 scores differ by exactly the threshold", () => {
    // Use 0.75/0.5 with threshold=0.25: both are exact in IEEE 754 (powers of 2),
    // so 0.75 - 0.5 = 0.25 exactly → NOT < 0.25 → not ambiguous.
    const candidates = [makeCandidate("a", 0.75), makeCandidate("b", 0.5)];
    expect(isAmbiguousSelection(candidates, 0.25)).toBe(false);
  });

  it("not ambiguous when top-2 scores differ by more than threshold", () => {
    const candidates = [makeCandidate("a", 0.9), makeCandidate("b", 0.5)];
    expect(isAmbiguousSelection(candidates)).toBe(false);
  });

  it("single candidate is never ambiguous", () => {
    const candidates = [makeCandidate("a", 0.9)];
    expect(isAmbiguousSelection(candidates)).toBe(false);
  });

  it("empty list is never ambiguous", () => {
    expect(isAmbiguousSelection([])).toBe(false);
  });

  it("respects custom threshold parameter", () => {
    const candidates = [makeCandidate("a", 0.9), makeCandidate("b", 0.85)];
    expect(isAmbiguousSelection(candidates, 0.04)).toBe(false); // diff=0.05 > 0.04
    expect(isAmbiguousSelection(candidates, 0.06)).toBe(true);  // diff=0.05 < 0.06
  });
});

// ---------------------------------------------------------------------------
// WA-S2: Target language validation (M10)
// ---------------------------------------------------------------------------
describe("Block M WA-S2 — Target language validation", () => {
  it("accepts all allowed language codes", () => {
    for (const lang of ALLOWED_TARGET_LANGUAGES) {
      expect(isValidTargetLanguage(lang)).toBe(true);
    }
  });

  it("rejects empty or null values", () => {
    expect(isValidTargetLanguage("")).toBe(false);
    expect(isValidTargetLanguage(null)).toBe(false);
    expect(isValidTargetLanguage(undefined)).toBe(false);
  });

  it("rejects unknown language codes", () => {
    expect(isValidTargetLanguage("xx")).toBe(false);
    expect(isValidTargetLanguage("english")).toBe(false);
    expect(isValidTargetLanguage("deutsch")).toBe(false);
  });

  it("accepts 'source' as valid language", () => {
    expect(isValidTargetLanguage("source")).toBe(true);
  });

  it("is case-insensitive", () => {
    expect(isValidTargetLanguage("EN")).toBe(true);
    expect(isValidTargetLanguage("De")).toBe(true);
    expect(isValidTargetLanguage("SOURCE")).toBe(true);
  });

  it("ALLOWED_TARGET_LANGUAGES includes at minimum the 5 UI options", () => {
    const uiOptions = ["source", "en", "de", "fr", "es"];
    for (const lang of uiOptions) {
      expect(ALLOWED_TARGET_LANGUAGES).toContain(lang);
    }
  });
});

// ---------------------------------------------------------------------------
// WA-S3: Candidate score edge cases
// ---------------------------------------------------------------------------
describe("Block M WA-S3 — Score edge cases", () => {
  it("candidates with same score are both ambiguous", () => {
    const candidates = [makeCandidate("a", 0.7), makeCandidate("b", 0.7)];
    expect(isAmbiguousSelection(candidates)).toBe(true);
  });

  it("perfect score vs zero is not ambiguous", () => {
    const candidates = [makeCandidate("a", 1.0), makeCandidate("b", 0.0)];
    expect(isAmbiguousSelection(candidates)).toBe(false);
  });
});
