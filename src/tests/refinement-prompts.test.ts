import { describe, expect, it } from "vitest";
import {
  normalizeRefinementPromptPreset,
  resolveEffectiveRefinementPrompt,
  resolveRefinementPresetPrompt,
} from "../refinement-prompts";

describe("refinement prompt presets", () => {
  it("normalizes unknown profile to wording", () => {
    expect(normalizeRefinementPromptPreset("unknown")).toBe("wording");
  });

  it("returns german preset text when language is de", () => {
    const prompt = resolveRefinementPresetPrompt("summary", "de");
    expect(prompt).toContain("Stichpunkten");
  });

  it("uses custom prompt when custom profile is active", () => {
    const prompt = resolveEffectiveRefinementPrompt("custom", "en", "Use this custom prompt.");
    expect(prompt).toBe("Use this custom prompt.");
  });
});

